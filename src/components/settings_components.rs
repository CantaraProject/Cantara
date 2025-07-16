//! This module contains components for displaying and manipulating the program and presentation settings

use super::shared_components::{DeleteIcon, EditIcon, PresentationDesignSelector, js_yes_no_box};
use crate::{Route, logic::settings::*};
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_router::prelude::*;
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
                h2 { { t!("settings.settings") } }
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
                    { t!("settings.close") }
                }
            }
        }
    }
}

/// Middleware component between SettingsPage and its children.
#[component]
fn SettingsContent(presentation_designs: Signal<Vec<PresentationDesign>>) -> Element {
    rsx! {
        RepositorySettings {}
        hr {}
        PresentationSettings {
            presentation_designs
        }
    }
}

/// Implements logic for adding, editing, and deleting repositories.
#[component]
fn RepositorySettings() -> Element {
    let mut settings = use_settings();
    let mut repository_file_counts: Signal<Vec<(usize, usize)>> = use_signal(Vec::new);

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
            h3 { { t!("settings.repositories_headline") } }
            p { { t!("settings.repositories_description") } }
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
                            div { { t!("settings.repositories_local_dir") }
                                br {}
                                pre { { string.clone() } }
                            }
                        }
                    }
                    RepositoryType::Remote(string) => {
                        rsx! {
                            div { { t!("settings.repositories_remote_dir") }
                                br {}
                                { string.clone() }
                            }
                        }
                    }
                    RepositoryType::RemoteZip(string) => {
                        rsx! {
                            div { { t!("settings.repositories_remote_zip") }
                                br {}
                                { string.clone() }
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
                            { t!("settings.source_files_count", count = file_count) }
                        }
                    }
                }
            }
        }
        div {
            class: "grid",
            button {
                class: "smaller-buttons",
                onclick: move |_| select_directory(),
                { t!("settings.add_folder") }
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
                { t!("settings.add_remote_repository") }
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
            h4 { { t!("settings.presentation_headline") } }
            p { { t!("settings.presentation_description") } }
        }

        // Always Start Fullscreen by Default switch
        article {
            class: "listed-article",
            div {
                div {
                    h6 { { t!("settings.always_start_fullscreen_title") } }
                    p { { t!("settings.always_start_fullscreen_description") } }
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
                            }
                        },
                        ondelete: move |_| {
                            if let Some(index) = selected_presentation_design_index() {
                                if index < presentation_designs.read().len() {
                                    presentation_designs.write().remove(index);
                                    selected_presentation_design_index.set(Some(0).filter(|_| !presentation_designs.read().is_empty()));
                                }
                            }
                        }
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
                    { t!("general.edit") }
                }
                button {
                    class: "secondary",
                    onclick: move |_| onclone.call(()),
                    { t!("general.duplicate") }
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
                    { t!("general.delete") }
                }
            }
        }
    }
}
