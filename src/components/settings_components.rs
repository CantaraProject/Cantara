//! This module contains components for displaying and manipulating the program and presentation settings

use super::directory_browser::DirectoryBrowserModal;
use super::dialogs::{confirm_box, message_box, prompt_box};
use super::shared_components::{DeleteIcon, EditIcon, PresentationDesignSelector, translate};
use crate::logic::sourcefiles::SourceFile;
use crate::logic::reorder::index_after_move;
use crate::logic::tag_mapping::TagMapping;
use super::jump_sidebar::{JumpSidebar, JumpTarget, scroll_to_section, use_section_spy};
use super::song_slide_settings_components::SongSlideSettingsSection;
#[cfg(feature = "desktop")]
use crate::logic::screens::{MonitorInfo, enumerate_monitors, screen_key, upgrade_screen_key};
#[cfg(feature = "desktop")]
use super::screen_map::{ChipKind, ScreenMap, settings_chips};
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
                                message_box(message).await;
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
    // The sections, in the order they appear. Named here rather than read out
    // of the page: this is the list the sidebar shows, and a heading the user
    // reads should be the heading the list names — the same translation key
    // feeds both, so they cannot drift apart.
    //
    // The section elements below carry these ids, and nothing else on the page
    // does; `use_section_spy` watches exactly them.
    let sections: Vec<JumpTarget> = vec![
        JumpTarget::section("settings-repositories", t!("settings.repositories_headline")),
        JumpTarget::section("settings-screens", t!("settings.screen_headline")),
        JumpTarget::section("settings-presentation", t!("settings.presentation_headline")),
        JumpTarget::section("settings-slides", t!("settings.song_slide_headline")),
        JumpTarget::section("settings-tag-mapping", t!("settings.tag_mapping_headline")),
        // The stream section is not in the web build, and neither is its entry.
        #[cfg(not(target_arch = "wasm32"))]
        JumpTarget::section("settings-stream", t!("settings.stream_headline")),
        // Last, and nothing to configure: the way to the about page. It sits
        // in this list because the list is what the page is read through — a
        // section the sidebar does not name is one a reader scrolls past
        // without knowing it was there.
        JumpTarget::section("settings-about", t!("about.settings_headline")),
    ];

    let ids: Vec<String> = sections.iter().map(|target| target.id.clone()).collect();
    let mut active_section = use_section_spy(ids.clone());

    // Marking a section because the reader went to it, rather than waiting for
    // the scrolling to say so. Two things ask for this and the watcher can
    // answer neither: a click in the list is an answer in itself, and clicking
    // *into* a section says where the reader is without moving the page at all.
    //
    // Named by its id and not by its position, so that the marking and the
    // element it belongs to carry the same word — a number here would have to
    // be kept in step with the list above by hand.
    let id_list: Signal<Vec<String>> = use_signal(|| ids.clone());
    let mut mark = move |id: &str| {
        if let Some(index) = id_list.read().iter().position(|known| known == id) {
            active_section.set(Some(index));
        }
    };

    rsx! {
        div { class: "jump-layout",
            JumpSidebar {
                targets: sections.clone(),
                active: active_section(),
                // The settings are long and the reader may want the width for
                // a wide form, so this one folds away.
                collapsible: true,
                on_select: move |index: usize| {
                    if let Some(id) = id_list.read().get(index) {
                        mark(id);
                        scroll_to_section(id);
                    }
                },
            }

            div { class: "jump-layout-content",
                section { id: "settings-repositories", onclick: move |_| mark("settings-repositories"), RepositorySettings {} }
                hr {}
                section { id: "settings-screens", onclick: move |_| mark("settings-screens"), ScreenSettings {} }
                hr {}
                section { id: "settings-presentation", onclick: move |_| mark("settings-presentation"), PresentationSettings {} }
                hr {}
                section { id: "settings-slides", onclick: move |_| mark("settings-slides"),
                    SongSlideSettingsSection {}
                }
                hr {}
                section { id: "settings-tag-mapping", onclick: move |_| mark("settings-tag-mapping"), TagMappingSection {} }

                // Left out of the web build along with its entry in the list.
                // The section itself is what the list points at, and one
                // without an entry is an anchor nothing can reach — on the web
                // an empty one at that, since the stream settings are not
                // there either.
                if cfg!(not(target_arch = "wasm32")) {
                    section { id: "settings-stream", onclick: move |_| mark("settings-stream"), StreamSettingsSection {} }
                }

                hr {}
                section { id: "settings-about", onclick: move |_| mark("settings-about"), AboutSection {} }
            }
        }
    }
}

/// The way to the about page, at the foot of the settings.
///
/// Here rather than on a menu because this is where a user goes looking for
/// what a program is — and because the settings are the one page that is
/// already about the program rather than about a service.
#[component]
fn AboutSection() -> Element {
    let nav = use_navigator();
    let mut entry: crate::logic::states::AboutEntryState = use_context();

    rsx! {
        hgroup {
            h3 { { t!("about.settings_headline").to_string() } }
            p { { t!("about.settings_description").to_string() } }
        }
        button {
            id: "settings-about-button",
            class: "outline",
            onclick: move |_| {
                // The about page's way out goes back where the reader came
                // from, and only this door can say that there *is* a way back
                // inside Cantara — see `states::AboutEntryState`.
                entry.from_inside.set(true);
                nav.push(Route::AboutPage {});
            },
            { t!("about.settings_button").to_string() }
        }
    }
}

/// Reading one collection's tag names as another's.
///
/// Sits after the slide settings because that is where the names it talks
/// about are used: the meta line of a slide is a template asking for tags by
/// name, and this is what decides which names a song answers to. See
/// [`crate::logic::tag_mapping`] for what a rule does and, just as
/// importantly, what it does not do — no file is ever changed.
#[component]
fn TagMappingSection() -> Element {
    let mut settings = use_settings();
    let mappings = use_memo(move || settings.read().tag_mappings.clone());

    rsx! {
        hgroup {
            h3 { { t!("settings.tag_mapping_headline").to_string() } }
            p { { t!("settings.tag_mapping_description").to_string() } }
        }
        article {
            class: "listed-article",

            if mappings().is_empty() {
                p {
                    style: "color: var(--pico-muted-color);",
                    { t!("settings.tag_mapping_empty").to_string() }
                }
            }

            for (index, mapping) in mappings().into_iter().enumerate() {
                div {
                    key: "{index}",
                    class: "tag-mapping-row",

                    label {
                        { t!("settings.tag_mapping_from").to_string() }
                        input {
                            r#type: "text",
                            value: "{mapping.from}",
                            placeholder: "author",
                            oninput: move |event| {
                                if let Some(rule) = settings.write().tag_mappings.get_mut(index) {
                                    rule.from = event.value();
                                }
                                settings.read().save();
                            },
                        }
                    }

                    span { class: "tag-mapping-arrow", "→" }

                    label {
                        { t!("settings.tag_mapping_to").to_string() }
                        input {
                            r#type: "text",
                            value: "{mapping.to}",
                            placeholder: "composer",
                            oninput: move |event| {
                                if let Some(rule) = settings.write().tag_mappings.get_mut(index) {
                                    rule.to = event.value();
                                }
                                settings.read().save();
                            },
                        }
                    }

                    button {
                        class: "outline secondary",
                        title: t!("settings.tag_mapping_remove").to_string(),
                        onclick: move |_| {
                            settings.write().tag_mappings.remove(index);
                            settings.read().save();
                        },
                        DeleteIcon {}
                    }
                }

                // A rule that is still being typed is marked rather than
                // refused: it simply does not take effect until it is whole.
                if let Some(problem) = mapping.problem() {
                    p {
                        class: "tag-mapping-problem",
                        { translate(problem, &[]) }
                    }
                }
            }

            button {
                class: "outline",
                onclick: move |_| {
                    settings.write().tag_mappings.push(TagMapping::default());
                    settings.read().save();
                },
                { t!("settings.tag_mapping_add").to_string() }
            }
        }
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
    let remote_password = use_memo(move || settings.read().stream.remote_password.clone());

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
            label {
                { t!("settings.remote_password").to_string() }
                input {
                    r#type: "password",
                    value: "{remote_password}",
                    onchange: move |event| {
                        settings.write().stream.remote_password = event.value();
                        settings.read().save();
                    },
                }
            }
            p {
                style: "font-size: 0.85em; color: var(--pico-muted-color);",
                { t!("settings.remote_password_note").to_string() }
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
                            onclick: {
                                // The field starts out holding the name the
                                // repository has, which is what makes this a
                                // rename rather than a re-entry. The web view's
                                // `prompt()` could be given a default too, but
                                // this one was never handed one — and on
                                // Android it does not exist at all, so the
                                // button did nothing there.
                                let current_name = repository.name.clone();
                                move |_| {
                                    let current_name = current_name.clone();
                                    async move {
                                        if let Some(name) = prompt_box(
                                            t!("settings.repository_rename_prompt").to_string(),
                                            current_name,
                                        )
                                        .await
                                        {
                                            settings.write().repositories[index].name = name;
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
                        let Some(url) = prompt_box(
                            t!("settings.remote_repository_url").to_string(),
                            String::new(),
                        )
                        .await
                        else {
                            return;
                        };

                        // Basic URL validation
                        if url.starts_with("http://") || url.starts_with("https://") {
                            settings.write().add_remote_zip_repository_url(url);
                            message_box(t!("settings.remote_repository_url_valid").to_string())
                                .await;
                        } else {
                            message_box(t!("settings.remote_repository_url_invalid").to_string())
                                .await;
                        }
                    }
                },
                { t!("settings.add_remote_repository").to_string() }
            }
            button {
                class: "smaller-buttons",
                onclick: move |_| {
                    async move {
                        // Which repository — `owner/repo`, or the full URL.
                        let Some(input) = prompt_box(
                            t!("settings.github_repository_prompt").to_string(),
                            String::new(),
                        )
                        .await
                        else {
                            return;
                        };

                        let Some((owner, repo)) = RepositoryType::parse_github_repo(&input) else {
                            message_box(t!("settings.github_repository_invalid").to_string()).await;
                            return;
                        };

                        // And a token, if the repository is a private one.
                        // Blank means none, which is what a public repository
                        // needs.
                        let token = prompt_box(
                            t!("settings.github_token_prompt").to_string(),
                            String::new(),
                        )
                        .await;

                        settings.write().add_github_repository(owner, repo, token);
                        message_box(t!("settings.github_repository_added").to_string()).await;
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
                active_item: selected_presentation_design_index,
                on_move: move |(from, to): (usize, usize)| {
                    // Through the settings, which renumber every stored
                    // choice of a design along with the move — the default,
                    // the stream, every view.
                    let landed = settings.write().move_presentation_design(from, to);
                    if let Some(landed) = landed {
                        // The design open beside the list is the one it was,
                        // wherever that is now.
                        if let Some(selected) = selected_presentation_design_index() {
                            selected_presentation_design_index
                                .set(Some(index_after_move(selected, from, landed)));
                        }
                        settings.read().save();
                    }
                },
            }

            div {
                class: "presentation-design-actions",
                div {
                    if let Some(selected_presentation) = selected_presentation_design() {
                        PresentationDesignCard {
                            presentation_design: selected_presentation,
                            index: selected_presentation_design_index(),
                            // The last design cannot go: nothing could be
                            // shown without one.
                            deletable: presentation_designs.read().len() > 1,
                            onclone: move |_| {
                                if let Some(design) = selected_presentation_design() {
                                    settings.write().presentation_designs.push(design);
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
                                        // Deleting a design moves every stored
                                        // choice that sat after it, so this is
                                        // one operation on the settings rather
                                        // than three calls here.
                                        settings.write().delete_presentation_design(index);
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

/// Rewrites every screen a settings file names as a [`screen_key`].
///
/// The settings page used to store a bare `monitor.name`; see
/// [`crate::logic::screens::upgrade_screen_key`] for why that is not enough.
/// Nothing is written unless something actually changed, so a visit to the
/// settings does not touch the file for no reason.
#[cfg(feature = "desktop")]
fn upgrade_stored_screen_keys(settings: &mut Signal<Settings>, monitors: &[MonitorInfo]) {
    if monitors.is_empty() {
        return;
    }

    let mut changed = false;
    {
        let mut writing = settings.write();

        let upgraded = upgrade_screen_key(&writing.presenter_screen, monitors);
        if upgraded != writing.presenter_screen {
            writing.presenter_screen = upgraded;
            changed = true;
        }

        for view in writing.views.iter_mut() {
            if let ViewOutput::Screen { monitor_name } = &view.output {
                let upgraded = upgrade_screen_key(monitor_name, monitors);
                if upgraded != *monitor_name {
                    view.output = ViewOutput::Screen {
                        monitor_name: upgraded,
                    };
                    changed = true;
                }
            }
        }
    }

    if changed {
        settings.read().save();
    }
}

/// The view the projection's drop-down stands for, and the screen it names.
///
/// The reference view is the projection: it is the sequence of slides
/// everything else is described against, and the one view a service cannot do
/// without. `None` when it has no screen to set — every view is served to the
/// network, or there are none at all.
#[cfg(feature = "desktop")]
fn reference_screen_view(settings: &Settings) -> Option<(usize, Option<String>)> {
    let index = settings.reference_view_position()?;
    match &settings.views.get(index)?.output {
        ViewOutput::Screen { monitor_name } => Some((index, monitor_name.clone())),
        ViewOutput::Network { .. } => None,
    }
}

/// What a screen is called in a drop-down.
///
/// A monitor's reported name is often empty, and on some platforms always is,
/// so a list of blank entries is a real possibility. The size stands in: two
/// screens on a machine are nearly always different sizes, and "1920 × 1080"
/// is something the user can match against what is in front of them.
#[cfg(feature = "desktop")]
fn screen_choice_label(monitor: &MonitorInfo) -> String {
    let size = format!("{} × {}", monitor.size.0, monitor.size.1);
    match monitor.name.trim().is_empty() {
        true => format!("{} ({size})", monitor.id + 1),
        false => format!("{} ({size})", monitor.name),
    }
}

/// Puts a window on a screen, or back to "automatic".
///
/// What a drop on the map does, and what the drop-downs beside it do — one
/// function, so that the two ways of saying the same thing cannot come to mean
/// different things.
///
/// `key` is a [`crate::logic::screens::screen_key`], never a bare name: a name
/// is frequently empty and frequently shared between two identical monitors,
/// and a view pointed at one would match the first screen or every screen
/// (D2).
#[cfg(feature = "desktop")]
fn assign_screen(settings: &mut Signal<Settings>, kind: ChipKind, key: Option<String>) {
    {
        let mut writing = settings.write();
        match kind {
            ChipKind::View(index) => {
                if let Some(view) = writing.views.get_mut(index) {
                    view.output = ViewOutput::Screen { monitor_name: key };
                }
            }
            ChipKind::PresenterConsole => writing.presenter_screen = key,
            // Where the main window sits is the window manager's business.
            // It is drawn so that the operator can tell which rectangle they
            // are looking at the map on, and nothing more (D3).
            ChipKind::MainWindow => {}
        }
    }
    settings.read().save();
}

/// Component for configuring screen/monitor settings for multi-screen presentation.
/// Only available on desktop platforms.
#[cfg(feature = "desktop")]
#[component]
fn ScreenSettings() -> Element {
    let mut settings = use_settings();
    let mut monitors: Signal<Vec<MonitorInfo>> = use_signal(Vec::new);

    // Enumerate monitors on mount, and upgrade anything the old settings
    // page stored as a bare name while the screens are in hand (D2). It runs
    // on every visit and does nothing the second time: a value that is already
    // a key is left alone.
    use_effect(move || {
        let desktop = dioxus::desktop::window();
        let found = enumerate_monitors(&desktop);
        upgrade_stored_screen_keys(&mut settings, &found);
        monitors.set(found);
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
                // The screens as they are actually arranged, with the windows
                // that go on them — see `docs/specs/0007-screen-map.md`. The
                // drop-downs below say the same thing and are the way to set
                // it without a pointer (D7); this is the picture that answers
                // "which rectangle is the beamer?".
                p {
                    class: "screen-map-intro",
                    { t!("settings.screen_map.description").to_string() }
                }
                ScreenMap {
                    monitors: monitors.read().clone(),
                    chips: settings_chips(&settings.read()),
                    on_assign: move |(kind, key): (ChipKind, Option<String>)| {
                        assign_screen(&mut settings, kind, key);
                    },
                }
            }
        }

        // Screen selection dropdowns
        if !monitors.read().is_empty() {
            div {
                class: "grid",
                div {
                    label { { t!("settings.presentation_screen").to_string() } }

                    // The projection's screen, written through to the
                    // reference view rather than to a field of its own (D1).
                    // `presentation_screen` is read by nothing but the
                    // migration in `ensure_views`, so a drop-down writing it
                    // was a control that did nothing on every installation
                    // that has been started since views arrived.
                    if let Some((index, chosen)) = reference_screen_view(&settings.read()) {
                        select {
                            onchange: move |event| {
                                let value = event.value();
                                let key = (value != "auto").then_some(value);
                                assign_screen(&mut settings, ChipKind::View(index), key);
                            },
                            option {
                                value: "auto",
                                selected: chosen.is_none(),
                                { t!("settings.automatic").to_string() }
                            }
                            for monitor in monitors.read().iter() {
                                option {
                                    value: screen_key(monitor),
                                    selected: chosen.as_deref() == Some(screen_key(monitor).as_str()),
                                    { screen_choice_label(monitor) }
                                }
                            }
                        }
                    } else {
                        // Nothing to point at: every view is served to the
                        // network, or there are no views at all. Disabled and
                        // said outright, rather than quietly writing somewhere
                        // nothing reads.
                        select { disabled: true,
                            option { { t!("settings.automatic").to_string() } }
                        }
                        p {
                            class: "screen-map-note",
                            { t!("settings.no_screen_view").to_string() }
                        }
                    }
                }
                div {
                    label { { t!("settings.presenter_screen").to_string() } }
                    // The same state the console's chip on the map shows, set
                    // the same way — through `assign_screen`, so the picture
                    // and the drop-down cannot come to mean different things.
                    // This is the keyboard path (D7).
                    select {
                        onchange: move |evt| {
                            let value = evt.value();
                            let key = (value != "auto").then_some(value);
                            assign_screen(&mut settings, ChipKind::PresenterConsole, key);
                        },
                        option {
                            value: "auto",
                            selected: settings.read().presenter_screen.is_none(),
                            { t!("settings.automatic").to_string() }
                        }
                        for monitor in monitors.read().iter() {
                            option {
                                // The whole key, not the bare name: a name is
                                // often empty and often shared between two
                                // identical monitors (D2).
                                value: screen_key(monitor),
                                selected: settings.read().presenter_screen.as_deref()
                                    == Some(screen_key(monitor).as_str()),
                                { screen_choice_label(monitor) }
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
    /// Whether the design may be deleted — not when it is the only one.
    deletable: bool,
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
                if deletable {
                    button {
                        class: "secondary",
                        onclick: move |event| {
                            event.prevent_default();
                            let question = t!("dialogs.confirm_deletion").to_string();
                            async move {
                                if confirm_box(question).await {
                                    tracing::debug!("Deletion confirmed.");
                                    ondelete.call(());
                                } else {
                                    tracing::debug!("Deletion aborted.");
                                }
                            }
                        },
                        { t!("general.delete").to_string() }
                    }
                }
            }
            if let Some(message) = export_error() {
                p { class: "export-save-error", role: "alert", {message} }
            }
        }
    }
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::*;
    use crate::logic::settings::{View, ViewFocus};

    fn view(name: &str, output: ViewOutput) -> View {
        View {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            design_index: None,
            slide_settings_index: None,
            output,
            enabled: true,
            focus: ViewFocus::Follow,
        }
    }

    fn screen_view(name: &str, monitor_name: Option<&str>) -> View {
        view(
            name,
            ViewOutput::Screen {
                monitor_name: monitor_name.map(str::to_string),
            },
        )
    }

    /// The decision D1 rests on: the projection's drop-down stands for the
    /// reference view, so what it shows is that view's screen — not a field
    /// of its own that nothing reads.
    #[test]
    fn the_projections_drop_down_reads_the_reference_views_screen() {
        let settings = Settings {
            views: vec![
                screen_view("Projection", Some("Beamer#1")),
                screen_view("Stage", Some("Bühne#2")),
            ],
            reference_view_index: 0,
            ..Default::default()
        };

        assert_eq!(
            reference_screen_view(&settings),
            Some((0, Some("Beamer#1".to_string())))
        );
    }

    /// It follows the reference wherever it is, rather than assuming the
    /// projection is the first view in the list.
    #[test]
    fn it_follows_the_reference_view_rather_than_the_first_one() {
        let settings = Settings {
            views: vec![
                screen_view("Stage", Some("Bühne#2")),
                screen_view("Projection", Some("Beamer#1")),
            ],
            reference_view_index: 1,
            ..Default::default()
        };

        assert_eq!(
            reference_screen_view(&settings),
            Some((1, Some("Beamer#1".to_string())))
        );
    }

    /// A reference view that names no screen is "automatic", which is a real
    /// answer and not an absent one: the drop-down is still live and shows it.
    #[test]
    fn a_reference_view_with_no_screen_chosen_is_still_offered() {
        let settings = Settings {
            views: vec![screen_view("Projection", None)],
            reference_view_index: 0,
            ..Default::default()
        };

        assert_eq!(reference_screen_view(&settings), Some((0, None)));
    }

    /// A reference view served to the network has no screen to set, and the
    /// drop-down says so instead of writing somewhere nothing reads.
    #[test]
    fn a_network_reference_view_offers_no_screen() {
        let settings = Settings {
            views: vec![view(
                "Stream",
                ViewOutput::Network {
                    path: "/".to_string(),
                },
            )],
            reference_view_index: 0,
            ..Default::default()
        };

        assert_eq!(reference_screen_view(&settings), None);
    }

    /// No views at all — nothing to point the drop-down at.
    #[test]
    fn no_views_offer_no_screen() {
        let settings = Settings {
            views: Vec::new(),
            ..Default::default()
        };

        assert_eq!(reference_screen_view(&settings), None);
    }

    /// A stored reference position that is out of date still names a view,
    /// the same one the rest of the program would use. A service is not the
    /// place to find out that an index has gone stale.
    #[test]
    fn a_stale_reference_position_still_names_a_view() {
        let settings = Settings {
            views: vec![screen_view("Projection", Some("Beamer#1"))],
            reference_view_index: 17,
            ..Default::default()
        };

        assert_eq!(
            reference_screen_view(&settings),
            Some((0, Some("Beamer#1".to_string())))
        );
    }
}
