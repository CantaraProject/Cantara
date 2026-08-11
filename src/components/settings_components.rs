//! This module contains components for displaying and manipulating the program and presentation settings

use super::directory_browser::DirectoryBrowserModal;
use super::shared_components::{
    DeleteIcon, EditIcon, PresentationDesignSelector, js_message_box, js_yes_no_box, translate,
};
use crate::logic::sourcefiles::SourceFile;
use super::song_slide_settings_components::SongSlideSettingsSection;
#[cfg(feature = "desktop")]
use crate::logic::screens::{MonitorInfo, enumerate_monitors};
use crate::{Route, logic::settings::*};
use dioxus::logger::tracing;
use dioxus::prelude::*;
#[cfg(feature = "desktop")]
use rfd::FileDialog;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// The component representing the settings page in Cantara. It loads settings from persistence
/// and provides the structure of the settings page.
#[component]
pub fn SettingsPage() -> Element {
    let nav = use_navigator();
    let settings = use_settings();

    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar",
                h2 { { t!("settings.settings").to_string() } }
            }
            main {
                class: "container-fluid content height-100",
                SettingsContent {}
            }
            footer {
                class: "bottom-bar",
                button {
                    onclick: move |_| {
                        // Leaving the settings is the moment they have to be on
                        // disk, so a failure here is told rather than logged:
                        // everything the user just configured is otherwise lost
                        // when the program ends, without a word.
                        let save_error = settings.read().try_save().err();

                        // The unpacked copies of the remote repositories are
                        // deliberately kept. Throwing them away here meant
                        // every visit to the settings cost a fresh download of
                        // every remote library afterwards — and worse, the
                        // songs already in the selection pointed into a
                        // directory that no longer existed. They belong to the
                        // program's lifetime and are removed when it ends; a
                        // repository the user actually deletes is cleaned up
                        // where that happens.
                        async move {
                            if let Some(error) = save_error {
                                let message = t!("dialogs.settings_not_saved", error = error)
                                    .to_string();
                                let _ = document::eval(&js_message_box(message)).await;
                            }
                            nav.replace(Route::Selection {});
                        }
                    },
                    { t!("settings.close").to_string() }
                }
            }
        }
    }
}

/// Middleware component between SettingsPage and its children.
#[component]
fn SettingsContent() -> Element {
    let mut settings = use_settings();
    let song_slide_settings: Signal<Vec<SongSlideSettings>> =
        use_signal(|| settings.read().song_slide_settings.clone());

    // `SongSlideSettings` edits a copy. Without mirroring it back, adding or
    // removing a slide setting was lost the moment the page was left, and
    // nothing was ever written to disk.
    //
    // `peek()` reads the settings without subscribing to them, so writing them
    // here cannot re-trigger this effect.
    use_effect(move || {
        let edited = song_slide_settings.read().clone();
        if settings.peek().song_slide_settings != edited {
            settings.write().song_slide_settings = edited;
            settings.peek().save();
        }
    });

    // Here the element order of the settings can be defined
    rsx! {
        RepositorySettings {}
        hr {}
        ScreenSettings {}
        hr {}
        PresentationSettings {}
        hr {}
        SongSlideSettingsSection {
            song_slide_settings
        }
        StreamSettingsSection {}
    }
}

/// How the running presentation is offered to the local network.
///
/// Only the *how* is here. Whether a given presentation is streamed is
/// switched on beside its other options in the selection window, so that a
/// program which was streaming once does not quietly start doing it again the
/// next time it opens.
///
/// Absent from the web build, along with the server it configures. A page
/// cannot listen on a port, and settings for something that can never happen
/// are worse than no settings at all.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn StreamSettingsSection() -> Element {
    let mut settings = use_settings();
    let port = use_memo(move || settings.read().stream.port);
    let password = use_memo(move || settings.read().stream.password.clone());

    rsx! {
        hgroup {
            h3 { { t!("settings.stream_headline").to_string() } }
            p { { t!("settings.stream_description").to_string() } }
        }
        article {
            class: "listed-article",
            label {
                { t!("settings.stream_port").to_string() }
                input {
                    r#type: "number",
                    min: "1024",
                    max: "65535",
                    value: "{port}",
                    onchange: move |event| {
                        // Anything that is not a port at all leaves the setting
                        // alone: a half-typed number should not silently move
                        // the server somewhere unexpected.
                        if let Ok(chosen) = event.value().parse::<u16>()
                            && chosen >= 1024
                        {
                            settings.write().stream.port = chosen;
                            settings.read().save();
                        }
                    },
                }
            }
            label {
                { t!("settings.stream_password").to_string() }
                input {
                    r#type: "password",
                    value: "{password}",
                    onchange: move |event| {
                        settings.write().stream.password = event.value();
                        settings.read().save();
                    },
                }
            }
            p {
                style: "font-size: 0.85em; color: var(--pico-muted-color);",
                { t!("settings.stream_plain_http_note").to_string() }
            }
        }
        hr {}
    }
}

/// There is no server inside a browser, so the web build has nothing to set up.
#[cfg(target_arch = "wasm32")]
#[component]
fn StreamSettingsSection() -> Element {
    rsx! {}
}

/// Implements logic for adding, editing, and deleting repositories.
#[component]
fn RepositorySettings() -> Element {
    let mut settings = use_settings();
    let mut show_dir_browser: Signal<bool> = use_signal(|| false);

    // How many files each repository holds, by its position in the list.
    let mut repository_file_counts: Signal<Vec<usize>> = use_signal(Vec::new);

    // Counting depends on the repositories alone. Reading the whole settings
    // here subscribed this to every switch on the page, so flipping "start
    // fullscreen" counted every library again — which, back when a count was
    // a full scan, read every file on disk a second time.
    let repositories = use_memo(move || settings.read().repositories.clone());
    let mut count_generation: Signal<u64> = use_signal(|| 0);

    // The counts are recomputed whenever the list of repositories changes,
    // which covers adding and removing one; nothing else has to ask for them.
    //
    // A repository that still has to be downloaded takes a while to answer,
    // so a second run can start while the first is going. Each claims a
    // generation and only publishes its result while that generation is still
    // the current one — see the same reasoning in `main`.
    use_effect(move || {
        let repositories = repositories();
        let generation = *count_generation.peek() + 1;
        count_generation.set(generation);

        spawn(async move {
            let mut counts = Vec::with_capacity(repositories.len());
            for repository in &repositories {
                counts.push(repository.get_source_file_count_async().await);
            }
            if *count_generation.peek() == generation {
                repository_file_counts.set(counts);
            }
        });
    });

    // Only the desktop branch writes through this closure; on mobile and the
    // web the body is compiled out.
    #[allow(unused_mut)]
    let mut select_directory = move || {
        #[cfg(feature = "desktop")]
        if let Some(path) = FileDialog::new().pick_folder()
            && path.is_dir() && path.exists() {
                let chosen_directory = path.to_str().unwrap_or_default().to_string();
                settings.write().add_repository_folder(chosen_directory);
            }
    };

    rsx! {
        hgroup {
            h3 { { t!("settings.repositories_headline").to_string() } }
            p { { t!("settings.repositories_description").to_string() } }
        }
        for (index, repository) in settings.read().repositories.clone().into_iter().enumerate() {
            article {
                class: "listed-article",
                h6 {
                    { repository.name.clone() }
                    div {
                        style: "float:right",
                        span {
                            onclick: move |_| {
                                async move {
                                    let new_name = match document::eval("return prompt('Please enter a new name: ', '');").await {
                                        Ok(str) => Some(str.to_string().replace("\"", "")),
                                        Err(_) => None,
                                    };
                                    if let Some(name) = new_name
                                        && !name.trim().is_empty() && name != "null" {
                                            settings.write().repositories[index].name = name.trim().to_string();
                                        }
                                }
                            },
                            EditIcon {}
                        }
                        if settings.read().repositories.len() > 1 && settings.read().repositories[index].removable {
                            span {
                                style: "float:right",
                                onclick: move |_| {
                                    // Clean up the repository before removing it
                                    let repo = settings.read().repositories[index].clone();
                                    repo.cleanup();

                                    settings.write().repositories.remove(index);
                                },
                                DeleteIcon {}
                            }
                        }
                    }
                }
                match &repository.repository_type {
                    RepositoryType::LocaleFilePath(string) => {
                        rsx! {
                            div { { t!("settings.repositories_local_dir").to_string() }
                                br {}
                                pre { { string.clone() } }
                            }
                        }
                    }
                    RepositoryType::Remote(string) => {
                        rsx! {
                            div { { t!("settings.repositories_remote_dir").to_string() }
                                br {}
                                { string.clone() }
                            }
                        }
                    }
                    RepositoryType::RemoteZip(string) => {
                        rsx! {
                            div { { t!("settings.repositories_remote_zip").to_string() }
                                br {}
                                { string.clone() }
                            }
                        }
                    }
                    RepositoryType::GitHub { owner, repo, token } => {
                        rsx! {
                            div { { t!("settings.repositories_github").to_string() }
                                br {}
                                a {
                                    href: format!("https://github.com/{}/{}", owner, repo),
                                    target: "_blank",
                                    { format!("{}/{}", owner, repo) }
                                }
                                if token.is_some() {
                                    span {
                                        style: "margin-left: 8px; font-style: italic;",
                                        { format!("({})", t!("settings.repositories_github_authenticated")) }
                                    }
                                }
                            }
                        }
                    }
                }
                // Display source file count
                {
                    let file_count = repository_file_counts.read().get(index).copied().unwrap_or(0);

                    rsx! {
                        div {
                            style: "margin-top: 10px; font-style: italic;",
                            { t!("settings.source_files_count", count = file_count).to_string() }
                        }
                    }
                }
            }
        }
        div {
            class: "grid",
            if cfg!(feature = "desktop") {
                button {
                    class: "smaller-buttons",
                    onclick: move |_| select_directory(),
                    { t!("settings.add_folder").to_string() }
                }
            }
            if cfg!(feature = "mobile") {
                button {
                    class: "smaller-buttons",
                    onclick: move |_| show_dir_browser.set(true),
                    { t!("settings.add_folder").to_string() }
                }
                DirectoryBrowserModal {
                    show: show_dir_browser,
                    on_select: move |path: String| {
                        settings.write().add_repository_folder(path);
                    }
                }
            }
            button {
                class: "smaller-buttons",
                onclick: move |_| {
                    async move {
                        let prompt_text = t!("settings.remote_repository_url").to_string();
                        let js_prompt = format!("return prompt('{}', '');", prompt_text);
                        let url = match document::eval(&js_prompt).await {
                            Ok(str) => Some(str.to_string().replace("\"", "")),
                            Err(_) => None,
                        };

                        if let Some(url) = url
                            && !url.trim().is_empty() && url != "null" {
                                // Basic URL validation
                                if url.starts_with("http://") || url.starts_with("https://") {
                                    // Add the repository
                                    settings.write().add_remote_zip_repository_url(url.trim().to_string());

                                    // Show success message
                                    let success_msg = t!("settings.remote_repository_url_valid").to_string();
                                    let _ = document::eval(&js_yes_no_box(success_msg)).await;
                                } else {
                                    // Show error message
                                    let error_msg = t!("settings.remote_repository_url_invalid").to_string();
                                    let _ = document::eval(&js_yes_no_box(error_msg)).await;
                                }
                            }
                    }
                },
                { t!("settings.add_remote_repository").to_string() }
            }
            button {
                class: "smaller-buttons",
                onclick: move |_| {
                    async move {
                        // Prompt for GitHub repository (owner/repo or full URL)
                        let prompt_text = t!("settings.github_repository_prompt").to_string();
                        let js_prompt = format!("return prompt('{}', '');", prompt_text);
                        let input = match document::eval(&js_prompt).await {
                            Ok(str) => Some(str.to_string().replace("\"", "")),
                            Err(_) => None,
                        };

                        if let Some(input) = input
                            && !input.trim().is_empty() && input != "null" {
                                match RepositoryType::parse_github_repo(&input) {
                                    Some((owner, repo)) => {
                                        // Prompt for optional token (for private repos)
                                        let token_prompt = t!("settings.github_token_prompt").to_string();
                                        let js_token_prompt = format!("return prompt('{}', '');", token_prompt);
                                        let token = match document::eval(&js_token_prompt).await {
                                            Ok(str) => {
                                                let t = str.to_string().replace("\"", "");
                                                if t.trim().is_empty() || t == "null" {
                                                    None
                                                } else {
                                                    Some(t.trim().to_string())
                                                }
                                            }
                                            Err(_) => None,
                                        };

                                        // Add the repository
                                        settings.write().add_github_repository(owner, repo, token);

                                        // Show success message
                                        let success_msg = t!("settings.github_repository_added").to_string();
                                        let _ = document::eval(&js_yes_no_box(success_msg)).await;
                                    }
                                    None => {
                                        // Show error message
                                        let error_msg = t!("settings.github_repository_invalid").to_string();
                                        let _ = document::eval(&js_yes_no_box(error_msg)).await;
                                    }
                                }
                            }
                    }
                },
                { t!("settings.add_github_repository").to_string() }
            }
        }
    }
}

/// Component for modifying presentation design settings.
#[component]
fn PresentationSettings() -> Element {
    let mut selected_presentation_design_index = use_signal(|| Some(0));
    let mut settings = use_settings();

    // The designs are read from — and written straight back to — the settings.
    // They used to be edited in a copy that was only handed over when the
    // settings page was closed, which meant a design that had just been
    // created did not exist yet for the editor the "edit" button opens: it
    // looks the design up by its position in the settings and found nothing
    // there.
    let presentation_designs =
        use_memo(move || settings.read().presentation_designs.clone());

    let selected_presentation_design = use_memo(move || {
        selected_presentation_design_index()
            .and_then(|index| presentation_designs.read().get(index).cloned())
    });

    rsx! {
        hgroup {
            h4 { { t!("settings.presentation_headline").to_string() } }
            p { { t!("settings.presentation_description").to_string() } }
        }

        // Always Start Fullscreen by Default switch
        article {
            class: "listed-article",
            div {
                div {
                    h6 { { t!("settings.always_start_fullscreen_title").to_string() } }
                    p { { t!("settings.always_start_fullscreen_description").to_string() } }
                }
                div {
                    label {
                        class: "switch",
                        input {
                            r#type: "checkbox",
                            role: "switch",
                            checked: settings.read().always_start_fullscreen,
                            onchange: move |event| {
                                settings.write().always_start_fullscreen = event.value().parse().unwrap_or(false);
                            }
                        }
                        span { class: "slider" }
                    }
                }
            }
        }

        // The designs lie beside each other and wrap onto the next line — see
        // `.presentation-design-selector`. What can be done with the chosen
        // one stands to the right of them where the window is wide enough for
        // a tile and a column of buttons, and underneath them where it is not.
        div {
            class: "presentation-design-layout",
            PresentationDesignSelector {
                presentation_designs,
                viewer_width: 400,
                active_item: selected_presentation_design_index
            }

            div {
                class: "presentation-design-actions",
                div {
                    if let Some(selected_presentation) = selected_presentation_design() {
                        PresentationDesignCard {
                            presentation_design: selected_presentation,
                            index: selected_presentation_design_index(),
                            onclone: move |_| {
                                if let Some(design) = selected_presentation_design() {
                                    {
                                        let mut settings_write = settings.write();
                                        settings_write.presentation_designs.push(design);
                                        // Ensure there are enough slide settings for all presentation designs
                                        settings_write.ensure_slide_settings_for_designs();
                                    }
                                    let new_len = presentation_designs.read().len();
                                    tracing::debug!("Cloned design. New length: {}", new_len);

                                    // Written out at once: the editor the user is
                                    // about to open reads the design from the
                                    // settings, and a design that only exists in
                                    // memory is one it cannot show.
                                    settings.read().save();
                                }
                            },
                            ondelete: move |_| {
                                if let Some(index) = selected_presentation_design_index()
                                    && index < presentation_designs.read().len() {
                                        {
                                            let mut settings_write = settings.write();
                                            // Also remove the corresponding slide setting if it exists
                                            if index < settings_write.song_slide_settings.len() {
                                                settings_write.song_slide_settings.remove(index);
                                            }
                                            settings_write.presentation_designs.remove(index);
                                            // Ensure slide settings and presentation designs stay in sync
                                            settings_write.ensure_slide_settings_for_designs();
                                        }
                                        selected_presentation_design_index.set((!presentation_designs.read().is_empty()).then_some(0));
                                        settings.read().save();
                                    }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Component for configuring screen/monitor settings for multi-screen presentation.
/// Only available on desktop platforms.
#[cfg(feature = "desktop")]
#[component]
fn ScreenSettings() -> Element {
    let mut settings = use_settings();
    let mut monitors: Signal<Vec<MonitorInfo>> = use_signal(Vec::new);

    // Enumerate monitors on mount
    use_effect(move || {
        let desktop = dioxus::desktop::window();
        monitors.set(enumerate_monitors(&desktop));
    });

    let refresh_monitors = move |_| {
        let desktop = dioxus::desktop::window();
        monitors.set(enumerate_monitors(&desktop));
    };

    rsx! {
        hgroup {
            h3 { { t!("settings.screen_headline").to_string() } }
            p { { t!("settings.screen_description").to_string() } }
        }

        // Show Presenter Console toggle
        article {
            class: "listed-article",
            div {
                div {
                    h6 { { t!("settings.show_presenter_console_title").to_string() } }
                    p { { t!("settings.show_presenter_console_description").to_string() } }
                }
                div {
                    label {
                        class: "switch",
                        input {
                            r#type: "checkbox",
                            role: "switch",
                            checked: settings.read().show_presenter_console,
                            onchange: move |event| {
                                settings.write().show_presenter_console = event.value().parse().unwrap_or(true);
                            }
                        }
                        span { class: "slider" }
                    }
                }
            }
        }

        // Presenter console in main window toggle
        if settings.read().show_presenter_console {
            article {
                class: "listed-article",
                div {
                    div {
                        h6 { { t!("settings.presenter_console_in_main_window_title").to_string() } }
                        p { { t!("settings.presenter_console_in_main_window_description").to_string() } }
                    }
                    div {
                        label {
                            class: "switch",
                            input {
                                r#type: "checkbox",
                                role: "switch",
                                checked: settings.read().presenter_console_in_main_window,
                                onchange: move |event| {
                                    settings.write().presenter_console_in_main_window = event.value().parse().unwrap_or(true);
                                }
                            }
                            span { class: "slider" }
                        }
                    }
                }
            }
        }

        // Detected monitors
        article {
            class: "listed-article",
            h6 {
                { t!("settings.detected_monitors").to_string() }
                button {
                    class: "outline secondary smaller-buttons",
                    style: "float: right; margin: 0; padding: 4px 12px;",
                    onclick: refresh_monitors,
                    { t!("settings.refresh_monitors").to_string() }
                }
            }

            if monitors.read().is_empty() {
                p {
                    style: "font-style: italic;",
                    { t!("settings.no_monitors_detected").to_string() }
                }
            } else {
                for monitor in monitors.read().iter() {
                    div {
                        style: "margin-bottom: 5px; padding: 5px; border-bottom: 1px solid var(--pico-muted-border-color);",
                        strong { { monitor.name.clone() } }
                        if monitor.name.is_empty() {
                            strong { { format!("Monitor {}", monitor.id + 1) } }
                        }
                        span {
                            style: "margin-left: 10px; color: var(--pico-muted-color);",
                            { format!("{}x{}", monitor.size.0, monitor.size.1) }
                        }
                        if monitor.is_primary {
                            span {
                                style: "margin-left: 10px; font-style: italic;",
                                { format!("({})", t!("settings.primary_monitor")) }
                            }
                        }
                    }
                }
            }
        }

        // Screen selection dropdowns
        if !monitors.read().is_empty() {
            div {
                class: "grid",
                div {
                    label { { t!("settings.presentation_screen").to_string() } }
                    select {
                        onchange: move |evt| {
                            let val = evt.value();
                            settings.write().presentation_screen = if val == "auto" {
                                None
                            } else {
                                Some(val)
                            };
                        },
                        option {
                            value: "auto",
                            selected: settings.read().presentation_screen.is_none(),
                            { t!("settings.automatic").to_string() }
                        }
                        for monitor in monitors.read().iter() {
                            option {
                                value: monitor.name.clone(),
                                selected: settings.read().presentation_screen.as_ref() == Some(&monitor.name),
                                {
                                    if monitor.name.is_empty() {
                                        format!("Monitor {}", monitor.id + 1)
                                    } else {
                                        monitor.name.clone()
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    label { { t!("settings.presenter_screen").to_string() } }
                    select {
                        onchange: move |evt| {
                            let val = evt.value();
                            settings.write().presenter_screen = if val == "auto" {
                                None
                            } else {
                                Some(val)
                            };
                        },
                        option {
                            value: "auto",
                            selected: settings.read().presenter_screen.is_none(),
                            { t!("settings.automatic").to_string() }
                        }
                        for monitor in monitors.read().iter() {
                            option {
                                value: monitor.name.clone(),
                                selected: settings.read().presenter_screen.as_ref() == Some(&monitor.name),
                                {
                                    if monitor.name.is_empty() {
                                        format!("Monitor {}", monitor.id + 1)
                                    } else {
                                        monitor.name.clone()
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Non-desktop platforms show only the presenter console toggle.
/// Monitor selection is not available on the web.
#[cfg(not(feature = "desktop"))]
#[component]
fn ScreenSettings() -> Element {
    let mut settings = use_settings();

    rsx! {
        hgroup {
            h3 { { t!("settings.screen_headline").to_string() } }
            p { { t!("settings.screen_description").to_string() } }
        }

        // Show Presenter Console toggle
        article {
            class: "listed-article",
            div {
                div {
                    h6 { { t!("settings.show_presenter_console_title").to_string() } }
                    p { { t!("settings.show_presenter_console_description").to_string() } }
                }
                div {
                    label {
                        class: "switch",
                        input {
                            r#type: "checkbox",
                            role: "switch",
                            checked: settings.read().show_presenter_console,
                            onchange: move |event| {
                                settings.write().show_presenter_console = event.value().parse().unwrap_or(true);
                            }
                        }
                        span { class: "slider" }
                    }
                }
            }
        }
    }
}

/// Displays an article with details and actions for a presentation design.
#[component]
fn PresentationDesignCard(
    presentation_design: PresentationDesign,
    index: Option<usize>,
    onclone: EventHandler<()>,
    ondelete: EventHandler<()>,
) -> Element {
    let nav = use_navigator();
    let mut export_error: Signal<Option<String>> = use_signal(|| None);

    // The design and everything it needs to look right, as one file to hand
    // on — see [`crate::logic::settings_io`].
    // Cloned for the closure: the card also renders the name and the
    // description, and a closure that took the design would take those with it.
    let design_to_export = presentation_design.clone();
    let export = move |_| {
        export_error.set(None);
        let design = design_to_export.clone();
        let written = crate::logic::settings_io::write_design(&design, &|file: &SourceFile| {
            crate::logic::sourcefiles::read_source_file_bytes(file)
        });

        let outcome = match written {
            Ok((name, bytes)) => {
                crate::components::shared_components::save_file(&name, &bytes).map(|_| ())
            }
            Err(error) => {
                let (key, parameters) = error.message_key();
                Err(translate(key, &parameters))
            }
        };

        if let Err(message) = outcome {
            log::warn!("the design could not be exported: {message}");
            export_error.set(Some(message));
        }
    };

    rsx! {
        article {
            h6 { { presentation_design.name.clone() } }
            p { { presentation_design.description.clone() } }
            if let Some(index) = index {
                button {
                    onclick: move |_| {
                        nav.push(Route::PresentationDesignSettingsPage { index: index as u16 });
                    },
                    { t!("general.edit").to_string() }
                }
                button {
                    class: "secondary",
                    onclick: move |_| onclone.call(()),
                    { t!("general.duplicate").to_string() }
                }
                button {
                    class: "secondary",
                    onclick: export,
                    { t!("settings.export_design").to_string() }
                }
                button {
                    class: "secondary",
                    onclick: move |event| {
                        event.prevent_default();
                        let js = t!("dialogs.confirm_deletion").to_string();
                        async move {
                            match document::eval(&js_yes_no_box(js)).await {
                                Ok(value) if value.as_bool().unwrap_or(false) => {
                                    tracing::debug!("Deletion confirmed.");
                                    ondelete.call(());
                                }
                                _ => tracing::debug!("Deletion aborted or failed."),
                            }
                        }
                    },
                    { t!("general.delete").to_string() }
                }
            }
            if let Some(message) = export_error() {
                p { class: "export-save-error", role: "alert", {message} }
            }
        }
    }
}
