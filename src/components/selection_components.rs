//! Components for source selection, filtering, and presentation startup.
//!
//! Internal structure:
//! - `search_ui`: search input and result rendering
//! - `source_items`: source lists, detail modal, and drop-file ingestion
//! - `selected_list`: selected item list and reordering UI
//! - `sidebar`: source-category sidebar and ordering
//! - `presentation_options`: per-item presentation option editing

mod presentation_options;
mod search_ui;
mod selected_list;
mod sidebar;
mod source_items;

use self::presentation_options::PresentationOptions;
use self::search_ui::{SearchInput, SearchResults};
use self::selected_list::SelectedItems;
use self::sidebar::SelectionFilterSideBar;
use self::source_items::{
    process_dropped_files, ImageSourceItems, MarkdownSourceItems, PdfSourceItems, SongSourceItems,
    SourceDetailView,
};
use crate::logic::presentation;
use crate::logic::search::{search_source_files, SearchResult};
use crate::logic::settings::PresentationDesign;
use crate::logic::settings::SelectionSidebarType;
use crate::logic::settings::Settings;
use crate::logic::sourcefiles::{SourceFile, SourceFileType};
use crate::logic::states::{RunningPresentation, SelectedItemRepresentation};
#[cfg(target_arch = "wasm32")]
use crate::logic::sync::{
    SYNC_KEY_ACTIVE, SYNC_KEY_FILES, SYNC_KEY_POSITION, SYNC_KEY_POSITION_FROM_CONSOLE,
    SYNC_KEY_PRESENTATION, SYNC_KEY_QUIT,
};
use crate::Route;
use cantara_songlib::exporter::lilypond::{LilypondSettings, lilypond_from_song};
use cantara_songlib::exporter::text::{TextFormat, TextSettings, text_from_songs};
use cantara_songlib::importer::{ccli, classic_song, cssf, song_yml};
use cantara_songlib::song::Song;
use cantara_songlib::slides::SlideSettings;
#[cfg(feature = "desktop")]
use dioxus::desktop::tao;
use dioxus::prelude::*;
use dioxus_free_icons::icons::fa_solid_icons::{FaFileExport, FaFileImport, FaGear, FaPlay};
use dioxus_free_icons::Icon;
use rust_i18n::t;
use std::path::Path;
use std::rc::Rc;

rust_i18n::i18n!("locales", fallback = "en");

#[component]
pub fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();

    let filter_string: Signal<String> = use_signal(|| "".to_string());
    let mut search_results: Signal<Vec<SearchResult>> = use_signal(Vec::new);
    let mut search_visible: Signal<bool> = use_signal(|| false);

    let mut source_files: Signal<Vec<SourceFile>> = use_context();
    let mut selected_items: Signal<Vec<SelectedItemRepresentation>> = use_context();
    let active_selected_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_detailed_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_selection_filter: Signal<SelectionSidebarType> =
        use_signal(|| SelectionSidebarType::Songs);
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    let mut drag_over_source: Signal<bool> = use_signal(|| false);

    let input_element_signal: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    let mut show_export_menu: Signal<bool> = use_signal(|| false);

    use_effect(move || {
        let query = filter_string.read().clone();
        if !query.is_empty() {
            let results = search_source_files(&source_files.read(), &query);
            let has_results = !results.is_empty();
            search_results.set(results);
            search_visible.set(has_results);
        } else {
            search_results.set(Vec::new());
            search_visible.set(false);
        }
    });

    let default_presentation_design_memo =
        use_memo(move || match settings.read().presentation_designs.first() {
            Some(design) => design.clone(),
            None => PresentationDesign::default(),
        });

    let default_song_slide_settings_memo = use_memo(move || {
        settings
            .read()
            .song_slide_settings
            .first()
            .unwrap_or(&SlideSettings::default())
            .clone()
    });

    use_effect(move || {
        if !settings.read().wizard_completed {
            nav.replace(Route::Wizard {});
        }

        spawn(async move {
            let files = settings.read().get_sourcefiles_async().await;
            source_files.set(files.clone());

            #[cfg(not(target_arch = "wasm32"))]
            std::thread::spawn(move || {
                crate::logic::search::refresh_search_cache(&files);
            });
            #[cfg(target_arch = "wasm32")]
            crate::logic::search::refresh_search_cache(&files);
        });
    });

    #[cfg(feature = "desktop")]
    use_effect(|| {
        let _ = document::eval(
            r#"
            (function() {
                if (window._cantara_drop_patched) return;
                if (!window.interpreter || typeof window.interpreter.handleWindowsDragDrop !== 'function') return;
                window._cantara_drop_patched = true;
                var orig = window.interpreter.handleWindowsDragDrop.bind(window.interpreter);
                window.interpreter.handleWindowsDragDrop = function() {
                    if (!window.dxDragLastElement) {
                        window.dxDragLastElement =
                            document.getElementById('selection-content') || document.body;
                    }
                    orig();
                };
            })();
        "#,
        );
    });

    rsx! {
        div {
            class: "wrapper",
            style: "position: relative;",
            onkeydown: move |event: Event<KeyboardData>| {
                if search_visible() {
                    let key_str = event.key().to_string();
                    if key_str.len() == 1 {
                        if let Some(digit) = key_str.chars().next().and_then(|c| c.to_digit(10))
                        {
                            let index = if digit == 0 { 9 } else { (digit as usize) - 1 };
                            let results = search_results.read();
                            if index < results.len() {
                                selected_items
                                    .write()
                                    .push(
                                        SelectedItemRepresentation::new_with_sourcefile(
                                            results[index].source_file.clone(),
                                        ),
                                    );
                                search_visible.set(false);
                                event.stop_propagation();
                            }
                        }
                    }
                }
            },
            header { class: "top-bar no-padding",
                SearchInput {
                    input_signal: filter_string,
                    element_signal: input_element_signal,
                }
            }

            // Running presentation indicator bar
            if !running_presentations.read().is_empty() {
                div { class: "running-presentation-bar",
                    span { {t!("selection.presentation_running").to_string()} }
                    button {
                        class: "outline",
                        onclick: move |_| {
                            presentation::update_presentation(
                                &selected_items.read(),
                                &mut running_presentations,
                                &default_presentation_design_memo(),
                                &default_song_slide_settings_memo(),
                            );
                        },
                        {t!("selection.update_presentation").to_string()}
                    }
                    if settings.read().presenter_console_in_main_window
                        && settings.read().show_presenter_console
                    {
                        button {
                            onclick: move |_| {
                                // Update the presentation with current selection before returning
                                presentation::update_presentation(
                                    &selected_items.read(),
                                    &mut running_presentations,
                                    &default_presentation_design_memo(),
                                    &default_song_slide_settings_memo(),
                                );
                                nav.push(crate::Route::PresenterConsolePage {});
                            },
                            {t!("selection.return_to_presenter").to_string()}
                        }
                    }
                }
            }

            // Display search results if there are any and search_visible is true
            if search_visible() {
                SearchResults {
                    search_results,
                    query: filter_string,
                    selected_items,
                    search_visible,
                }
            }
            main {
                id: "selection-content",
                class: "content content-background height-100",
                onclick: move |_| {
                    if search_visible() {
                        search_visible.set(false);
                    }
                },
                ondragover: move |event: DragEvent| {
                    event.prevent_default();
                },
                ondrop: move |event: DragEvent| async move {
                    event.prevent_default();
                    drag_over_source.set(false);
                    process_dropped_files(event, source_files, selected_items).await;
                },
                onmounted: move |_| async move {
                    let _ = document::eval("initSelectionLayout();").await;
                },
                onkeydown: move |event: Event<KeyboardData>| async move {
                    let key = event.key().to_string();
                    if search_visible() && key.len() == 1
                        && key.chars().next().is_some_and(|c| c.is_ascii_digit())
                    {
                        return;
                    }
                    let is_other_input_focused = document::eval(
                            r#"
                                                                                                                                                                        (function() {
                                                                                                                                                                            var a = document.activeElement;
                                                                                                                                                                            return a && (a.tagName === 'TEXTAREA' || (a.tagName === 'INPUT' && a.id !== 'searchinput'));
                                                                                                                                                                        })()
                                                                                                                                                                    "#,
                        )
                        .await
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if is_other_input_focused {
                        return;
                    }
                    if let Some(searchinput) = input_element_signal() {
                        let _ = searchinput.set_focus(true).await;
                    }
                },
                div { class: "grid swipe-container height-100",

                    div {
                        class: if drag_over_source() { "height-100 swipe-panel drop-zone drag-active" } else { "height-100 swipe-panel drop-zone" },
                        ondragover: move |event: DragEvent| {
                            event.prevent_default();
                            drag_over_source.set(true);
                        },
                        ondragleave: move |_| {
                            drag_over_source.set(false);
                        },
                        ondrop: move |event: DragEvent| async move {
                            event.prevent_default();
                            event.stop_propagation();
                            drag_over_source.set(false);
                            process_dropped_files(event, source_files, selected_items).await;
                        },
                        SelectionFilterSideBar { active_selection: active_selection_filter }
                        if active_selection_filter() == SelectionSidebarType::Songs {
                            SongSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                            }
                        }
                        if active_selection_filter() == SelectionSidebarType::Pictures {
                            ImageSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                            }
                        }
                        if active_selection_filter() == SelectionSidebarType::Pdfs {
                            PdfSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                            }
                        }
                        if active_selection_filter() == SelectionSidebarType::Markdown {
                            MarkdownSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                            }
                        }
                    }

                    div { class: "height-100 scrollable-container swipe-panel",
                        if !selected_items.read().is_empty() {
                            SelectedItems {
                                selected_items,
                                active_selected_item_id,
                            }
                        }
                    }

                    div { class: "swipe-panel",
                        PresentationOptions { selected_items, active_selected_item_id }
                    }
                }
            }
            div { class: "swipe-indicator",
                div {
                    class: "swipe-dot active",
                    onclick: move |_| {
                        let _ = document::eval("scrollToPanel(0);");
                    },
                }
                div {
                    class: "swipe-dot",
                    onclick: move |_| {
                        let _ = document::eval("scrollToPanel(1);");
                    },
                }
                div {
                    class: "swipe-dot",
                    onclick: move |_| {
                        let _ = document::eval("scrollToPanel(2);");
                    },
                }
            }
            footer { class: "bottom-bar",
                div { class: "no-padding width-100", role: "group",
                    button {
                        onclick: move |_| {
                            nav.push(crate::Route::SettingsPage {});
                        },
                        class: "outline secondary smaller-buttons",
                        span { class: "mobile-only",
                            Icon { icon: FaGear }
                        }
                        span { class: "desktop-only", {t!("settings.settings_button").to_string()} }
                    }
                    button { class: "outline secondary smaller-buttons",
                        span { class: "mobile-only",
                            Icon { icon: FaFileImport }
                        }
                        span { class: "desktop-only", {t!("selection.import").to_string()} }
                    }
                    button {
                        class: "outline secondary smaller-buttons",
                        onclick: move |_| {
                            show_export_menu.set(true);
                        },
                        span { class: "mobile-only",
                            Icon { icon: FaFileExport }
                        }
                        span { class: "desktop-only", {t!("selection.export").to_string()} }
                    }
                    button {
                        class: "primary smaller-buttons",
                        onclick: move |_| {
                            if running_presentations.read().is_empty() {
                                start_presentation(
                                    &selected_items.read().clone(),
                                    &mut running_presentations,
                                    &default_presentation_design_memo(),
                                    &default_song_slide_settings_memo(),
                                    &settings.read(),
                                );
                            } else {
                                presentation::update_presentation(
                                    &selected_items.read(),
                                    &mut running_presentations,
                                    &default_presentation_design_memo(),
                                    &default_song_slide_settings_memo(),
                                );
                                if settings.read().presenter_console_in_main_window
                                    && settings.read().show_presenter_console
                                {
                                    nav.push(crate::Route::PresenterConsolePage {});
                                }
                            }
                        },
                        span { class: "mobile-only",
                            Icon { icon: FaPlay }
                        }
                        span { class: "desktop-only",
                            if running_presentations.read().is_empty() {
                                {t!("selection.start_presentation").to_string()}
                            } else {
                                {t!("selection.update_presentation").to_string()}
                            }
                        }
                    }
                }
            }
        }

        if active_detailed_item_id.read().is_some() {
            SourceDetailView { source_files, active_detailed_item_id }
        }

        // Export menu overlay
        if show_export_menu() {
            ExportMenu { show_export_menu, selected_items }
        }
    }
}

/// Helper function to start a presentation from the selection page.
/// Supports multi-screen placement and optional presenter console.
#[cfg(feature = "desktop")]
fn start_presentation(
    selected_items: &Vec<SelectedItemRepresentation>,
    running_presentations: &mut Signal<Vec<RunningPresentation>>,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    settings_read: &Settings,
) {
    use super::presentation_components::PresentationPage;
    use super::presenter_console_components::PresenterConsolePage;
    use crate::logic::screens::{enumerate_monitors, resolve_monitor};
    use dioxus::desktop::Config;

    if presentation::add_presentation(
        selected_items,
        running_presentations,
        default_presentation_design,
        default_slide_settings,
    )
    .is_some()
    {
        let desktop = dioxus::desktop::window();
        let monitors = enumerate_monitors(&desktop);

        let presentation_monitor =
            resolve_monitor(&monitors, &settings_read.presentation_screen, false);

        if let Some(ref monitor) = presentation_monitor {
            if let Some(rp) = running_presentations.write().last_mut() {
                rp.presentation_resolution = monitor.size;
            }
        }

        let presenter_monitor = resolve_monitor(&monitors, &settings_read.presenter_screen, true);

        let show_presenter_console = settings_read.show_presenter_console;
        let always_fullscreen = settings_read.always_start_fullscreen;

        let mut presentation_window_builder = tao::window::WindowBuilder::new()
            .with_resizable(true)
            .with_visible(true);

        if let Some(ref monitor) = presentation_monitor {
            presentation_window_builder = presentation_window_builder
                .with_position(tao::dpi::PhysicalPosition::new(
                    monitor.position.0,
                    monitor.position.1,
                ))
                .with_inner_size(tao::dpi::PhysicalSize::new(monitor.size.0, monitor.size.1))
                .with_decorations(false)
                .with_fullscreen(Some(tao::window::Fullscreen::Borderless(None)));
        } else if always_fullscreen {
            presentation_window_builder = presentation_window_builder
                .with_decorations(false)
                .with_fullscreen(Some(tao::window::Fullscreen::Borderless(None)));
        } else {
            presentation_window_builder = presentation_window_builder
                .with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0))
                .with_maximized(true);
        }

        let presentation_dom =
            VirtualDom::new(PresentationPage).with_root_context(*running_presentations);

        dioxus::desktop::window().new_window(
            presentation_dom,
            Config::new()
                .with_menu(None)
                .with_disable_drag_drop_handler(true)
                .with_window(presentation_window_builder),
        );

        if show_presenter_console {
            if settings_read.presenter_console_in_main_window {
                let nav = navigator();
                nav.push(crate::Route::PresenterConsolePage {});
            } else {
                let mut console_window_builder = tao::window::WindowBuilder::new()
                    .with_resizable(true)
                    .with_decorations(true)
                    .with_visible(true)
                    .with_title("Cantara - Presenter Console");

                if let Some(ref monitor) = presenter_monitor {
                    console_window_builder = console_window_builder
                        .with_position(tao::dpi::PhysicalPosition::new(
                            monitor.position.0,
                            monitor.position.1,
                        ))
                        .with_inner_size(tao::dpi::PhysicalSize::new(
                            monitor.size.0,
                            monitor.size.1,
                        ))
                        .with_maximized(true);
                } else {
                    console_window_builder = console_window_builder
                        .with_inner_size(tao::dpi::LogicalSize::new(900.0, 700.0))
                        .with_maximized(true);
                }

                let console_dom =
                    VirtualDom::new(PresenterConsolePage).with_root_context(*running_presentations);

                dioxus::desktop::window().new_window(
                    console_dom,
                    Config::new()
                        .with_menu(None)
                        .with_disable_drag_drop_handler(true)
                        .with_window(console_window_builder),
                );
            }
        }
    }
}

#[cfg(not(feature = "desktop"))]
fn start_presentation(
    selected_items: &Vec<SelectedItemRepresentation>,
    running_presentations: &mut Signal<Vec<RunningPresentation>>,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    settings_read: &Settings,
) {
    // Build the presentation data without writing to any signal yet.
    // On web, window.open() must be called BEFORE signal writes because
    // window.open() can trigger synchronous browser callbacks (e.g. focus/blur)
    // that re-enter the Dioxus diff engine. If signals are dirty at that point,
    // the diff engine tries to process the dirty component whose scope is still
    // borrowed from the current event handler, causing a "RefCell already
    // borrowed" panic.
    let Some(rp) = presentation::build_presentation(
        selected_items,
        default_presentation_design,
        default_slide_settings,
    ) else {
        return;
    };

    let nav = navigator();
    if settings_read.show_presenter_console {
        // Store the presentation data in localStorage for the new-tab presentation
        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(json) = serde_json::to_string(&rp) {
                let _ = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .map(|s| {
                        let _ = s.set_item(SYNC_KEY_PRESENTATION, &json);
                        let _ = s.set_item(SYNC_KEY_ACTIVE, "true");
                        let _ = s.remove_item(SYNC_KEY_QUIT);
                        let _ = s.remove_item(SYNC_KEY_POSITION);
                        let _ = s.remove_item(SYNC_KEY_POSITION_FROM_CONSOLE);
                    });
            }

            // Collect VFS files (e.g. PDFs) needed by the presentation and store
            // them in localStorage so the new tab can populate its own VFS.
            {
                use crate::logic::presentation::get_picture_path;
                use crate::logic::settings::RepositoryType;
                use cantara_songlib::slides::SlideContent;
                use std::collections::HashMap;

                let mut files: HashMap<String, String> = HashMap::new();
                for chapter in &rp.presentation {
                    for slide in &chapter.slides {
                        if let SlideContent::SimplePicture(ref pic) = slide.slide_content {
                            let path = get_picture_path(pic);
                            let base_path = path.split('#').next().unwrap_or(&path).to_string();
                            if base_path.to_lowercase().ends_with(".pdf")
                                && !files.contains_key(&base_path)
                            {
                                if let Some(bytes) = RepositoryType::web_read_file(&base_path) {
                                    files.insert(
                                        base_path,
                                        base64::Engine::encode(
                                            &base64::engine::general_purpose::STANDARD,
                                            &bytes,
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
                if !files.is_empty() {
                    if let Ok(files_json) = serde_json::to_string(&files) {
                        let _ = web_sys::window()
                            .and_then(|w| w.local_storage().ok().flatten())
                            .map(|s| s.set_item(SYNC_KEY_FILES, &files_json));
                    }
                }
            }
            // Open the presentation in a new browser tab.
            // This MUST happen before the signal write below — see comment above.
            // Derive the URL from window.location at runtime so that any deployment
            // base path (e.g. /Cantara/ on GitHub Pages) is handled automatically.
            if let Some(win) = web_sys::window() {
                let location = win.location();
                match (location.origin(), location.pathname()) {
                    (Ok(origin), Ok(pathname)) => {
                        // The Selection page is the root route "/". Stripping the
                        // trailing slash gives the deployment base, e.g. "/Cantara"
                        // for GitHub Pages or "" for local dev.
                        let base = pathname.trim_end_matches('/');
                        let url = format!("{}{}/presentation", origin, base);
                        match win.open_with_url_and_target(&url, "_blank") {
                            Ok(Some(_)) => {
                                // Successfully opened new tab/window.
                            }
                            Ok(None) | Err(_) => {
                                // Popup likely blocked or failed to open; inform the user.
                                let _ = win.alert_with_message(
                                    "Unable to open the presentation in a new tab.\n\
Please allow pop-ups for this site or open the presentation manually.",
                                );
                            }
                        }
                    }
                    _ => {
                        // Location API unavailable (should not happen in a browser).
                        let _ = win.alert_with_message(
                            "Unable to determine the app URL. \
Please open the presentation tab manually.",
                        );
                    }
                }
            }
        }

        // NOW write to the signal — window.open() is done, so browser callbacks
        // won't re-enter Dioxus while the signal is dirty.
        running_presentations.write().clear();
        running_presentations.write().push(rp);

        // Navigate the current tab to the presenter console
        nav.push(crate::Route::PresenterConsolePage {});
    } else {
        // No presenter console: write to signal and start presentation in same tab
        running_presentations.write().clear();
        running_presentations.write().push(rp);
        nav.push(crate::Route::PresentationPage {});
    }
}

fn build_export_document(
    selected_items: &[SelectedItemRepresentation],
    export_format: &str,
) -> Result<(String, String), String> {
    let songs: Vec<Song> = selected_items
        .iter()
        .filter(|item| item.source_file.file_type == SourceFileType::Song)
        .map(song_from_selected_item)
        .collect::<Result<Vec<_>, _>>()?;

    if songs.is_empty() {
        return Err("Es sind keine Lied-Dateien für den Export ausgewählt.".to_string());
    }

    match export_format {
        "text" => {
            let settings = TextSettings::with_format(TextFormat::Plain);
            let content = text_from_songs(&songs, &settings)
                .map_err(|err| format!("Text-Export fehlgeschlagen: {err}"))?;
            Ok((content, "txt".to_string()))
        }
        "telegram" => {
            let settings = TextSettings::with_format(TextFormat::Telegram);
            let content = text_from_songs(&songs, &settings)
                .map_err(|err| format!("Telegram-Export fehlgeschlagen: {err}"))?;
            Ok((content, "txt".to_string()))
        }
        "markdown" => {
            let settings = TextSettings::with_format(TextFormat::Markdown);
            let content = text_from_songs(&songs, &settings)
                .map_err(|err| format!("Markdown-Export fehlgeschlagen: {err}"))?;
            Ok((content, "md".to_string()))
        }
        "lilypond" => {
            let mut sections: Vec<String> = Vec::new();
            for song in &songs {
                let lilypond = lilypond_from_song(song, &LilypondSettings::default())
                    .map_err(|err| format!("LilyPond-Export für '{}' fehlgeschlagen: {err}", song.title))?;
                sections.push(format!("% {}\n{}", song.title, lilypond));
            }
            Ok((
                sections.join("\n\n% ------------------------------------------------------------\n\n"),
                "ly".to_string(),
            ))
        }
        _ => Err("Unbekanntes Exportformat.".to_string()),
    }
}

fn song_from_selected_item(item: &SelectedItemRepresentation) -> Result<Song, String> {
    song_from_source_file(&item.source_file)
}

fn song_from_source_file(source_file: &SourceFile) -> Result<Song, String> {
    let content = read_source_file_content(source_file)?;
    parse_song_from_content(&source_file.path, &content)
}

fn parse_song_from_content(path: &Path, content: &str) -> Result<Song, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("song");
    let file_name_lower = file_name.to_lowercase();
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "song" => classic_song::import_song(content).map_err(|err| err.to_string()),
        "ccli" => ccli::import_from_ccli_string(content).map_err(|err| err.to_string()),
        "cssf" => cssf::import_input_string(content.to_string(), file_name.to_string())
            .map_err(|err| err.to_string()),
        "yml" | "yaml"
            if file_name_lower.ends_with(".song.yml")
                || file_name_lower.ends_with(".song.yaml") =>
        {
            song_yml::import_from_yml_string(content).map_err(|err| err.to_string())
        }
        _ => Err(format!(
            "Nicht unterstütztes Liedformat: {}",
            path.display()
        )),
    }
}

fn read_source_file_content(source_file: &SourceFile) -> Result<String, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::logic::settings::RepositoryType;

        let path_str = source_file.path.to_string_lossy().to_string();
        let bytes = RepositoryType::web_read_file(&path_str).ok_or_else(|| {
            format!(
                "Datei '{}' konnte im Web-Speicher nicht gelesen werden.",
                source_file.name
            )
        })?;
        String::from_utf8(bytes).map_err(|err| {
            format!(
                "Datei '{}' ist kein gültiger UTF-8-Text: {err}",
                source_file.name
            )
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read_to_string(&source_file.path)
            .map_err(|err| format!("Datei '{}' konnte nicht gelesen werden: {err}", source_file.path.display()))
    }
}

fn save_export_document(
    content: &str,
    extension: &str,
    selected_items: &[SelectedItemRepresentation],
) -> Result<(), String> {
    let base_name = if selected_items.len() == 1 {
        selected_items[0].source_file.name.clone()
    } else {
        "cantara-export".to_string()
    };
    let file_name = format!("{}.{}", base_name, extension);

    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&file_name)
            .save_file()
        {
            std::fs::write(&path, content).map_err(|err| {
                format!("Exportdatei konnte nicht geschrieben werden ({}): {err}", path.display())
            })?;
            Ok(())
        } else {
            Err("Export wurde abgebrochen.".to_string())
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        let file_name_json = serde_json::to_string(&file_name)
            .map_err(|err| format!("Dateiname konnte nicht serialisiert werden: {err}"))?;
        let content_json = serde_json::to_string(content)
            .map_err(|err| format!("Exportinhalt konnte nicht serialisiert werden: {err}"))?;

        spawn(async move {
            let js = format!(
                r#"
                (function() {{
                    const filename = {file_name_json};
                    const content = {content_json};
                    const blob = new Blob([content], {{ type: 'text/plain;charset=utf-8' }});
                    const url = URL.createObjectURL(blob);
                    const link = document.createElement('a');
                    link.href = url;
                    link.download = filename;
                    document.body.appendChild(link);
                    link.click();
                    document.body.removeChild(link);
                    URL.revokeObjectURL(url);
                }})();
                "#
            );
            let _ = document::eval(&js).await;
        });

        Ok(())
    }
}

fn show_export_error(error_message: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = rfd::MessageDialog::new()
            .set_title("Cantara Export")
            .set_description(error_message)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }

    #[cfg(target_arch = "wasm32")]
    {
        if let Ok(message_json) = serde_json::to_string(error_message) {
            spawn(async move {
                let _ = document::eval(&format!("window.alert({message_json});")).await;
            });
        }
    }
}

/// Export menu component that provides various export options for selected songs.
/// The menu can be extended with additional export formats in the future.
#[component]
fn ExportMenu(
    /// Signal to control the visibility of the export menu
    show_export_menu: Signal<bool>,
    /// The currently selected items to export
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    let mut export_format: Signal<String> = use_signal(|| "text".to_string());

    let handle_export = move |_| {
        let selected = selected_items.read().clone();
        let format = export_format.read().clone();

        match build_export_document(&selected, &format)
            .and_then(|(content, extension)| save_export_document(&content, &extension, &selected))
        {
            Ok(_) => {
                log::info!("Export abgeschlossen: {} item(s) als {}", selected.len(), format);
                show_export_menu.set(false);
            }
            Err(err) => {
                log::warn!("Export fehlgeschlagen: {}", err);
                if err != "Export wurde abgebrochen." {
                    show_export_error(&err);
                }
            }
        }
    };

    rsx! {
        div {
            class: "modal-overlay",
            style: "position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.5); z-index: 2000; display: flex; align-items: center; justify-content: center;",
            onclick: move |_| {
                show_export_menu.set(false);
            },
            div {
                class: "export-menu-modal",
                style: "background: white; padding: 2em; border-radius: 8px; max-width: 500px; width: 90%; position: relative; box-shadow: 0 4px 6px rgba(0,0,0,0.1);",
                onclick: move |event: Event<MouseData>| {
                    event.stop_propagation();
                },
                h3 { style: "margin-top: 0; margin-bottom: 1em;",
                    {t!("selection.export_title").to_string()}
                }
                p { style: "margin-bottom: 1em; color: #666;",
                    {
                        t!("selection.export_description", count = selected_items.read().len())
                            .to_string()
                    }
                }
                div { style: "margin-bottom: 1.5em;",
                    label { style: "display: block; margin-bottom: 0.5em; font-weight: bold;",
                        {t!("selection.export_format").to_string()}
                    }
                    select {
                        class: "form-control",
                        value: "{export_format()}",
                        onchange: move |event| {
                            export_format.set(event.value());
                        },
                        option { value: "text", {t!("selection.export_format_text").to_string()} }
                        option { value: "telegram", {t!("selection.export_format_telegram").to_string()} }
                        option { value: "markdown", {t!("selection.export_format_markdown").to_string()} }
                        option { value: "lilypond", {t!("selection.export_format_lilypond").to_string()} }
                        option { value: "pdf", disabled: true,
                            {t!("selection.export_format_pdf").to_string()}
                        }
                    }
                }
                div { style: "display: flex; gap: 1em; justify-content: flex-end;",
                    button {
                        class: "outline secondary",
                        onclick: move |_| {
                            show_export_menu.set(false);
                        },
                        {t!("settings.close").to_string()}
                    }
                    button { class: "primary", onclick: handle_export,
                        {t!("selection.export_button").to_string()}
                    }
                }
            }
        }
    }
}
