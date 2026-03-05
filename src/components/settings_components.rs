//! This module contains components for displaying and manipulating the program and presentation settings

use super::directory_browser::DirectoryBrowserModal;
use super::shared_components::{DeleteIcon, EditIcon, PresentationDesignSelector, js_yes_no_box};
use super::song_slide_settings_components::SongSlideSettings;
#[cfg(feature = "desktop")]
use crate::logic::screens::{MonitorInfo, enumerate_monitors};
use crate::{Route, logic::settings::*};
use cantara_songlib::slides::SlideSettings;
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
    let mut settings = use_settings();
    let presentation_designs: Signal<Vec<PresentationDesign>> =
        use_signal(|| settings.read().presentation_designs.clone());

    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar",
                h2 { { t!("settings.settings").to_string() } }
            }
            main {
                class: "container-fluid content height-100",
                SettingsContent {
                    presentation_designs
                }
            }
            footer {
                class: "bottom-bar",
                button {
                    onclick: move |_| {
                        settings.write().presentation_designs = presentation_designs.read().clone();
                        settings.read().save();

                        // Clean up any temporary directories before navigating away
                        // This helps ensure that resources are properly cleaned up
                        settings.read().cleanup_all_repositories();

                        nav.replace(Route::Selection {});
                    },
                    { t!("settings.close").to_string() }
                }
            }
        }
    }
}

/// Middleware component between SettingsPage and its children.
#[component]
fn SettingsContent(presentation_designs: Signal<Vec<PresentationDesign>>) -> Element {
    let mut settings = use_settings();
    let song_slide_settings: Signal<Vec<SlideSettings>> =
        use_signal(|| settings.read().song_slide_settings.clone());

    rsx! {
        RepositorySettings {}
        hr {}
        ScreenSettings {}
        hr {}
        PresentationSettings {
            presentation_designs
        }
        hr {}
        SongSlideSettings {
            song_slide_settings
        }
    }
}

/// Implements logic for adding, editing, and deleting repositories.
#[component]
fn RepositorySettings() -> Element {
    let mut settings = use_settings();
    let mut repository_file_counts: Signal<Vec<(usize, usize)>> = use_signal(Vec::new);
    let mut show_dir_browser: Signal<bool> = use_signal(|| false);

    // Load file counts for each repository
    use_effect(move || {
        let repositories = settings.read().repositories.clone();

        use_future(move || {
            let repos = repositories.clone();
            async move {
                let mut counts = Vec::new();
                for (idx, repo) in repos.iter().enumerate() {
                    let count = repo.get_source_file_count_async().await;
                    counts.push((idx, count));
                }
                repository_file_counts.set(counts);
            }
        });
    });

    let mut select_directory = move || {
        #[cfg(feature = "desktop")]
        if let Some(path) = FileDialog::new().pick_folder() {
            if path.is_dir() && path.exists() {
                let chosen_directory = path.to_str().unwrap_or_default().to_string();
                settings.write().add_repository_folder(chosen_directory);

                // Trigger a refresh of the file counts
                let repositories = settings.read().repositories.clone();
                use_future(move || {
                    let repos = repositories.clone();
                    async move {
                        let mut counts = Vec::new();
                        for (idx, repo) in repos.iter().enumerate() {
                            let count = repo.get_source_file_count_async().await;
                            counts.push((idx, count));
                        }
                        repository_file_counts.set(counts);
                    }
                });
            }
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
                                    if let Some(name) = new_name {
                                        if !name.trim().is_empty() && name != "null" {
                                            settings.write().repositories[index].name = name.trim().to_string();
                                        }
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

                                    // Trigger a refresh of the file counts
                                    let repositories = settings.read().repositories.clone();
                                    use_future(move || {
                                        let repos = repositories.clone();
                                        async move {
                                            let mut counts = Vec::new();
                                            for (idx, repo) in repos.iter().enumerate() {
                                                let count = repo.get_source_file_count_async().await;
                                                counts.push((idx, count));
                                            }
                                            repository_file_counts.set(counts);
                                        }
                                    });
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
                                        { format!("({})", t!("settings.repositories_github_authenticated").to_string()) }
                                    }
                                }
                            }
                        }
                    }
                }
                // Display source file count
                {
                    let file_count = repository_file_counts.read().iter()
                        .find(|(idx, _)| *idx == index)
                        .map(|(_, count)| *count)
                        .unwrap_or(0);

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

                        // Trigger a refresh of the file counts
                        let repositories = settings.read().repositories.clone();
                        spawn(async move {
                            let mut counts = Vec::new();
                            for (idx, repo) in repositories.iter().enumerate() {
                                let count = repo.get_source_file_count_async().await;
                                counts.push((idx, count));
                            }
                            repository_file_counts.set(counts);
                        });
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

                        if let Some(url) = url {
                            if !url.trim().is_empty() && url != "null" {
                                // Basic URL validation
                                if url.starts_with("http://") || url.starts_with("https://") {
                                    // Add the repository
                                    settings.write().add_remote_zip_repository_url(url.trim().to_string());

                                    // Trigger a refresh of the file counts
                                    let repositories = settings.read().repositories.clone();
                                    let mut counts = Vec::new();
                                    for (idx, repo) in repositories.iter().enumerate() {
                                        let count = repo.get_source_file_count_async().await;
                                        counts.push((idx, count));
                                    }
                                    repository_file_counts.set(counts);

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

                        if let Some(input) = input {
                            if !input.trim().is_empty() && input != "null" {
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

                                        // Trigger a refresh of the file counts
                                        let repositories = settings.read().repositories.clone();
                                        let mut counts = Vec::new();
                                        for (idx, repo) in repositories.iter().enumerate() {
                                            let count = repo.get_source_file_count_async().await;
                                            counts.push((idx, count));
                                        }
                                        repository_file_counts.set(counts);

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
                    }
                },
                { t!("settings.add_github_repository").to_string() }
            }
        }
    }
}

/// Component for modifying presentation design settings.
#[component]
fn PresentationSettings(presentation_designs: Signal<Vec<PresentationDesign>>) -> Element {
    let mut selected_presentation_design_index = use_signal(|| Some(0));
    let mut selected_presentation_design = use_signal(|| None::<PresentationDesign>);
    let mut settings = use_settings();

    use_effect(move || {
        let new_value = selected_presentation_design_index()
            .and_then(|index| presentation_designs.read().get(index).cloned());
        selected_presentation_design.set(new_value);
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

        div {
            class: "grid",
            div {
                PresentationDesignSelector {
                    presentation_designs,
                    viewer_width: 400,
                    active_item: selected_presentation_design_index
                }
            }
            div {
                if let Some(selected_presentation) = selected_presentation_design() {
                    PresentationDesignCard {
                        presentation_design: selected_presentation,
                        index: selected_presentation_design_index(),
                        onclone: move |_| {
                            if let Some(design) = selected_presentation_design() {
                                presentation_designs.write().push(design);
                                let new_len = presentation_designs.read().len();
                                tracing::debug!("Cloned design. New length: {}", new_len);
                                
                                // Ensure there are enough slide settings for all presentation designs
                                settings.write().ensure_slide_settings_for_designs();
                            }
                        },
                        ondelete: move |_| {
                            if let Some(index) = selected_presentation_design_index() {
                                if index < presentation_designs.read().len() {
                                    // Also remove the corresponding slide setting if it exists
                                    if index < settings.read().song_slide_settings.len() {
                                        settings.write().song_slide_settings.remove(index);
                                    }
                                    
                                    presentation_designs.write().remove(index);
                                    selected_presentation_design_index.set(Some(0).filter(|_| !presentation_designs.read().is_empty()));
                                    
                                    // Ensure slide settings and presentation designs stay in sync
                                    settings.write().ensure_slide_settings_for_designs();
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
                                { format!("({})", t!("settings.primary_monitor").to_string()) }
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
    rsx! {
        article {
            h6 { { presentation_design.name } }
            p { { presentation_design.description } }
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
        }
    }
}
