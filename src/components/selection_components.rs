//! Components for source selection, filtering, and presentation startup.
//!
//! Internal structure:
//! - `search_ui`: search input and result rendering
//! - `source_items`: source lists, detail modal, and drop-file ingestion
//! - `selected_list`: selected item list and reordering UI
//! - `sidebar`: source-category sidebar and ordering
//! - `presentation_options`: per-item presentation option editing

mod presentation_options;
pub(crate) mod search_ui;
mod selected_list;
pub(crate) mod sidebar;
pub(crate) mod source_items;

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
use crate::logic::settings::use_settings;
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
use crate::logic::export::{ExportError, ExportFormat, ExportedFile, song_from_content};
use crate::logic::pptx::{PptxConversion, PptxDeck, deck_from_slides};

const PPTX_EXPORT_JS: &str = include_str!("../../assets/pptx_export_inline.js");
/// PptxGenJS is bundled from `node_modules` so that an export works offline.
///
/// Minification has to stay off: the bundle publishes itself with a top-level
/// `var PptxGenJS = ...`, which only becomes a global because a classic script
/// puts its `var`s on `window`. The minifier renames it — the file then loads
/// without error and registers nothing at all.
#[cfg(not(target_arch = "wasm32"))]
const PPTXGEN_LIB: Asset = asset!(
    "/node_modules/pptxgenjs/dist/pptxgen.bundle.js",
    AssetOptions::js().with_minify(false)
);
/// The web build has no `node_modules`, so it takes the library from a CDN.
#[cfg(target_arch = "wasm32")]
const PPTXGEN_CDN_LIB: &str = "https://cdn.jsdelivr.net/npm/pptxgenjs@4.0.1/dist/pptxgen.bundle.js";
use cantara_songlib::song::Song;
use cantara_songlib::slides::SlideSettings;
#[cfg(feature = "desktop")]
use dioxus::desktop::tao;
use dioxus::prelude::*;
use dioxus_free_icons::icons::fa_solid_icons::{FaFileExport, FaFileImport, FaGear, FaPlay};
use dioxus_free_icons::Icon;
use rust_i18n::t;
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

    let wizard_completed = use_memo(move || settings.read().wizard_completed);

    use_effect(move || {
        if !wizard_completed() {
            nav.replace(Route::Wizard {});
        }
    });

    // Where a build starts. The desktop is built around assembling a
    // presentation, so it opens the selection; the web version is mostly used
    // to look songs up, so it opens the detail view instead — but only once
    // the wizard is out of the way, otherwise this would race the effect
    // above and the flag below would be spent on a redirect that immediately
    // got overridden.
    //
    // The "have we done this already" flag lives in `App`, so it survives
    // this component unmounting and remounting — a signal owned by
    // `Selection` itself would reset every time the view mounts, bouncing the
    // user straight back here whenever the footer button navigated to the
    // selection. The navigation call itself has to happen here rather than in
    // `App`, since only a descendant of `Router` can call `navigator()`.
    #[cfg(target_arch = "wasm32")]
    {
        let mut initial_route: crate::logic::states::InitialRouteState = use_context();
        use_effect(move || {
            if wizard_completed() && !(initial_route.redirected_to_detail)() {
                initial_route.redirected_to_detail.set(true);
                nav.replace(Route::Detail {});
            }
        });
    }

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
                    source_files,
                    active_detailed_item_id,
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
                    crate::components::detail_components::ViewModeToggle {}
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

/// Read and parse every song of the selection.
///
/// Items that are not songs — pictures, PDFs, Markdown — are skipped rather
/// than reported: a selection usually mixes them, and the user asked to export
/// the songs.
fn songs_of_selection(
    selected_items: &[SelectedItemRepresentation],
) -> Result<Vec<Song>, ExportError> {
    selected_items
        .iter()
        .filter(|item| item.source_file.file_type == SourceFileType::Song)
        .map(|item| {
            let content = read_source_file_content(&item.source_file)?;
            let file_name = item
                .source_file
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&item.source_file.name);
            song_from_content(file_name, &content)
        })
        .collect()
}

fn read_source_file_content(source_file: &SourceFile) -> Result<String, ExportError> {
    let unreadable = |reason: String| ExportError::Unreadable {
        name: source_file.name.clone(),
        reason,
    };

    #[cfg(target_arch = "wasm32")]
    {
        use crate::logic::settings::RepositoryType;

        let path_str = source_file.path.to_string_lossy().to_string();
        let bytes = RepositoryType::web_read_file(&path_str)
            .ok_or_else(|| unreadable("not found in the web storage".to_string()))?;
        String::from_utf8(bytes).map_err(|error| unreadable(error.to_string()))
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read_to_string(&source_file.path).map_err(|error| unreadable(error.to_string()))
    }
}

/// Turn an [`ExportError`] into a message in the user's language.
fn export_error_message(error: &ExportError) -> String {
    match error {
        ExportError::NoSongs => t!("selection.export_error_no_songs").to_string(),
        ExportError::Unreadable { name, reason } => {
            t!("selection.export_error_unreadable", name = name, reason = reason).to_string()
        }
        ExportError::UnsupportedFormat { name } => {
            t!("selection.export_error_unsupported", name = name).to_string()
        }
        ExportError::Unparsable { name, reason } => {
            t!("selection.export_error_unparsable", name = name, reason = reason).to_string()
        }
        ExportError::SongFailed { title, reason } => {
            t!("selection.export_error_song_failed", title = title, reason = reason).to_string()
        }
        ExportError::Render(reason) => {
            t!("selection.export_error_render", reason = reason).to_string()
        }
        ExportError::Write { path, reason } => {
            t!("selection.export_error_write", path = path, reason = reason).to_string()
        }
    }
}

/// How a save attempt ended.
enum SaveOutcome {
    /// Files were written.
    Written(usize),
    /// The user closed the file dialog without choosing.
    Cancelled,
}

/// Write the rendered files, asking the user where they should go.
///
/// A format that produces one file per song asks for a directory; a single
/// document asks for a file name.
///
/// Only the desktop has a native file dialog — `rfd` is a desktop-only
/// dependency. Mobile and the web hand the file to the WebView instead, which
/// is why the split is by feature rather than by `wasm32`: Android is neither
/// wasm nor a desktop, and gating on the target arch alone compiled `rfd` into
/// a build that does not have it.
#[cfg(feature = "desktop")]
fn save_exported_files(
    files: &[ExportedFile],
    format: ExportFormat,
) -> Result<SaveOutcome, ExportError> {
    let write = |path: &std::path::Path, content: &str| {
        std::fs::write(path, content).map_err(|error| ExportError::Write {
            path: path.display().to_string(),
            reason: error.to_string(),
        })
    };

    if format.one_file_per_song() && files.len() > 1 {
        let Some(directory) = rfd::FileDialog::new()
            .set_title(t!("selection.export_choose_folder").to_string())
            .pick_folder()
        else {
            return Ok(SaveOutcome::Cancelled);
        };

        for file in files {
            write(
                &directory.join(format!("{}.{}", file.name, format.extension())),
                &file.content,
            )?;
        }
        return Ok(SaveOutcome::Written(files.len()));
    }

    let Some(file) = files.first() else {
        return Ok(SaveOutcome::Written(0));
    };

    let Some(path) = rfd::FileDialog::new()
        .set_file_name(format!("{}.{}", file.name, format.extension()))
        .save_file()
    else {
        return Ok(SaveOutcome::Cancelled);
    };

    write(&path, &file.content)?;
    Ok(SaveOutcome::Written(1))
}

/// Without a file dialog — the web and mobile — each file is offered to the
/// WebView as a download, which is where the platform then puts it.
#[cfg(not(feature = "desktop"))]
fn save_exported_files(
    files: &[ExportedFile],
    format: ExportFormat,
) -> Result<SaveOutcome, ExportError> {
    for file in files {
        let name = serde_json::to_string(&format!("{}.{}", file.name, format.extension()))
            .map_err(|error| ExportError::Render(error.to_string()))?;
        let content = serde_json::to_string(&file.content)
            .map_err(|error| ExportError::Render(error.to_string()))?;

        spawn(async move {
            let js = format!(
                r#"
                (function() {{
                    const blob = new Blob([{content}], {{ type: 'text/plain;charset=utf-8' }});
                    const url = URL.createObjectURL(blob);
                    const link = document.createElement('a');
                    link.href = url;
                    link.download = {name};
                    document.body.appendChild(link);
                    link.click();
                    document.body.removeChild(link);
                    URL.revokeObjectURL(url);
                }})();
                "#
            );
            let _ = document::eval(&js).await;
        });
    }

    Ok(SaveOutcome::Written(files.len()))
}

/// Writes the current selection as a PowerPoint deck.
///
/// The file is built by PptxGenJS from the deck description in
/// [`crate::logic::pptx`], so no native PowerPoint library is needed.
///
/// Where the bytes go differs by target, exactly as it does for the text
/// formats: the desktop asks Rust to write the file to a path the user picked,
/// the web lets the browser download it. Letting PptxGenJS download on the
/// desktop does not work — the WebView drops the `<a download>` click without
/// an error, so the export reports success and produces no file.
fn export_pptx(
    deck: &PptxDeck,
    file_name: &str,
    mut message: Signal<Option<String>>,
    mut close_dialog: Signal<bool>,
) {
    // On the desktop the user picks the destination first: it is the one step
    // that can be cancelled, and doing it before the work avoids building a
    // deck nobody asked to keep.
    #[cfg(feature = "desktop")]
    let target_path = {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(file_name)
            .save_file()
        else {
            return;
        };
        path
    };

    let Ok(deck_json) = serde_json::to_string(deck) else {
        message.set(Some(t!("selection.export_error_render", reason = "deck").to_string()));
        return;
    };
    let Ok(name_json) = serde_json::to_string(file_name) else {
        return;
    };
    let Ok(url_json) = serde_json::to_string(&pptxgen_url()) else {
        return;
    };

    // Only the desktop takes the bytes back into Rust to write them itself;
    // everywhere else the WebView receives the file directly.
    let mode = if cfg!(feature = "desktop") {
        "\"base64\""
    } else {
        "\"download\""
    };

    let script = PPTX_EXPORT_JS
        .replace("__DECK__", &deck_json)
        .replace("__FILE_NAME__", &name_json)
        .replace("__PPTXGEN_URL__", &url_json)
        .replace("__MODE__", mode);

    spawn(async move {
        // The shim returns `{ok, error, data}`; anything else means the
        // evaluation itself broke. Either way the user hears about it — a file
        // that never arrives is otherwise indistinguishable from a dead button.
        let outcome = match document::eval(&script).await {
            Ok(value) => {
                if value.get("ok").and_then(|ok| ok.as_bool()) == Some(true) {
                    Ok(value
                        .get("data")
                        .and_then(|data| data.as_str())
                        .unwrap_or_default()
                        .to_string())
                } else {
                    Err(value
                        .get("error")
                        .and_then(|error| error.as_str())
                        .unwrap_or("unknown")
                        .to_string())
                }
            }
            Err(error) => Err(format!("{error:?}")),
        };

        #[cfg(feature = "desktop")]
        let outcome = outcome.and_then(|data| write_pptx_file(&data, &target_path));

        match outcome {
            Ok(_) => close_dialog.set(false),
            Err(reason) => {
                log::error!("could not write the PowerPoint file: {reason}");
                message.set(Some(
                    t!("selection.export_error_render", reason = reason).to_string(),
                ));
            }
        }
    });
}

/// Decodes the deck PptxGenJS produced and writes it to `path`.
#[cfg(feature = "desktop")]
fn write_pptx_file(base64_data: &str, path: &std::path::Path) -> Result<String, String> {
    use base64::Engine;

    if base64_data.is_empty() {
        return Err("PptxGenJS returned no data".to_string());
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|error| error.to_string())?;

    std::fs::write(path, bytes).map_err(|error| error.to_string())?;
    Ok(path.display().to_string())
}

/// Where PptxGenJS is loaded from.
fn pptxgen_url() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("{}", PPTXGEN_LIB)
    }
    #[cfg(target_arch = "wasm32")]
    {
        PPTXGEN_CDN_LIB.to_string()
    }
}

/// Copy a string to the system clipboard.
///
/// The desktop build goes through the webview so that the same code path works
/// on every platform Cantara ships on.
fn copy_to_clipboard(text: &str) {
    let Ok(payload) = serde_json::to_string(text) else {
        return;
    };

    spawn(async move {
        let script = format!(
            r#"
            (async function() {{
                const text = {payload};
                try {{
                    await navigator.clipboard.writeText(text);
                }} catch (error) {{
                    // Older webviews have no async clipboard API.
                    const area = document.createElement('textarea');
                    area.value = text;
                    area.style.position = 'fixed';
                    area.style.opacity = '0';
                    document.body.appendChild(area);
                    area.select();
                    document.execCommand('copy');
                    document.body.removeChild(area);
                }}
            }})();
            "#
        );
        let _ = document::eval(&script).await;
    });
}

#[component]
fn ExportMenu(
    /// Signal to control the visibility of the export menu
    show_export_menu: Signal<bool>,
    /// The currently selected items to export
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    let settings = use_settings();
    let mut export_format: Signal<ExportFormat> = use_signal(|| ExportFormat::PlainText);
    let mut template: Signal<String> =
        use_signal(|| ExportFormat::default_template().to_string());
    let mut copied = use_signal(|| false);

    let song_count = use_memo(move || {
        selected_items
            .read()
            .iter()
            .filter(|item| item.source_file.file_type == SourceFileType::Song)
            .count()
    });

    // A PowerPoint deck is built from the presentation slides rather than from
    // the songs, so it takes its own path through the design.
    let conversion: Memo<Option<PptxConversion>> = use_memo(move || {
        if *export_format.read() != ExportFormat::Pptx {
            return None;
        }
        let design = settings
            .read()
            .presentation_designs
            .first()
            .cloned()
            .unwrap_or_default();
        let slide_settings = settings
            .read()
            .song_slide_settings
            .first()
            .cloned()
            .unwrap_or_default();

        crate::logic::presentation::build_presentation(
            &selected_items.read(),
            &design,
            &slide_settings,
        )
        .map(|presentation| {
            let slides: Vec<_> = presentation
                .presentation
                .iter()
                .flat_map(|chapter| chapter.slides.clone())
                .collect();
            deck_from_slides(&slides, &design)
        })
    });

    // The document is rendered as soon as the dialog opens and again whenever
    // the format or the template changes, so the preview always shows what
    // would be saved — and a format that cannot be produced says why instead of
    // doing nothing when the button is pressed.
    let rendered: Memo<Result<Vec<ExportedFile>, String>> = use_memo(move || {
        let format = *export_format.read();

        if format == ExportFormat::Pptx {
            return match &*conversion.read() {
                Some(conversion) if !conversion.deck.is_empty() => {
                    let mut note =
                        t!("selection.export_pptx_note", count = conversion.deck.slides.len())
                            .to_string();
                    // PowerPoint has no way to draw a staff, so a notation
                    // layout silently loses it. Say so rather than let the user
                    // discover it in the finished file.
                    if conversion.skipped_notation > 0 {
                        note.push('\n');
                        note.push_str(&t!(
                            "selection.export_pptx_notation_note",
                            count = conversion.skipped_notation
                        ));
                    }
                    Ok(vec![ExportedFile {
                        name: "cantara-presentation".to_string(),
                        content: format!("{}\n\n{}", note, conversion.deck.to_json()),
                    }])
                }
                _ => Err(export_error_message(&ExportError::NoSongs)),
            };
        }

        let selected = selected_items.read().clone();
        let template = template.read().clone();
        songs_of_selection(&selected)
            .and_then(|songs| format.render_with(&songs, &template))
            .map_err(|error| export_error_message(&error))
    });

    let preview_text = use_memo(move || match &*rendered.read() {
        Ok(files) => files
            .first()
            .map(|file| file.content.clone())
            .unwrap_or_default(),
        Err(message) => message.clone(),
    });

    // Shown in the dialog when a save fails, so the dialog stays open with a
    // reason instead of quietly doing nothing.
    let mut save_error: Signal<Option<String>> = use_signal(|| None);

    let handle_save = move |_| {
        let format = *export_format.read();
        save_error.set(None);

        // The binary formats are written by the browser, not by Rust.
        if format.is_binary() {
            if let Some(conversion) = conversion.read().clone() {
                export_pptx(
                    &conversion.deck,
                    "cantara-presentation.pptx",
                    save_error,
                    show_export_menu,
                );
            }
            return;
        }

        let outcome = match &*rendered.read() {
            Ok(files) => {
                save_exported_files(files, format).map_err(|error| export_error_message(&error))
            }
            Err(message) => Err(message.clone()),
        };

        match outcome {
            Ok(SaveOutcome::Written(count)) => {
                log::info!("exported {count} file(s) as {}", format.id());
                show_export_menu.set(false);
            }
            // Closing the file dialog is not a failure and needs no message.
            Ok(SaveOutcome::Cancelled) => {}
            Err(message) => {
                log::warn!("export failed: {message}");
                save_error.set(Some(message));
            }
        }
    };

    rsx! {
        div {
            class: "modal-overlay export-menu-overlay",
            onclick: move |_| {
                show_export_menu.set(false);
            },
            div {
                class: "export-menu-modal",
                onclick: move |event: Event<MouseData>| {
                    event.stop_propagation();
                },
                h3 { {t!("selection.export_title").to_string()} }

                div { class: "export-menu-body",
                    div { class: "export-menu-options",
                        p {
                            {t!("selection.export_description", count = song_count()).to_string()}
                        }

                        label {
                            {t!("selection.export_format").to_string()}
                            select {
                                value: export_format.read().id(),
                                onchange: move |event| {
                                    if let Some(format) = ExportFormat::from_id(&event.value()) {
                                        export_format.set(format);
                                        copied.set(false);
                                    }
                                },
                                for format in ExportFormat::ALL {
                                    option { value: format.id(), {t!(format.label_key()).to_string()} }
                                }
                            }
                        }

                        if export_format.read().needs_template() {
                            label {
                                {t!("selection.export_template").to_string()}
                                textarea {
                                    class: "export-template-input",
                                    rows: "6",
                                    spellcheck: false,
                                    value: "{template}",
                                    oninput: move |event| template.set(event.value()),
                                }
                                small { {t!("selection.export_template_hint").to_string()} }
                            }
                        }

                        if let Ok(files) = &*rendered.read() {
                            if files.len() > 1 {
                                small {
                                    {
                                        t!("selection.export_files_note", count = files.len())
                                            .to_string()
                                    }
                                }
                            }
                        }
                    }

                    div { class: "export-menu-preview",
                        label { {t!("selection.export_preview").to_string()} }
                        textarea {
                            class: if rendered.read().is_err() { "export-preview-text has-error" } else { "export-preview-text" },
                            readonly: true,
                            spellcheck: false,
                            wrap: "off",
                            value: "{preview_text}",
                        }
                        button {
                            r#type: "button",
                            class: "outline",
                            disabled: rendered.read().is_err(),
                            onclick: move |_| {
                                copy_to_clipboard(&preview_text.read());
                                copied.set(true);
                            },
                            if copied() {
                                {t!("selection.export_copied").to_string()}
                            } else {
                                {t!("selection.export_copy").to_string()}
                            }
                        }
                    }
                }

                if let Some(message) = save_error() {
                    p { class: "export-save-error", role: "alert", {message} }
                }

                div { class: "export-menu-actions",
                    button {
                        class: "outline secondary",
                        onclick: move |_| {
                            show_export_menu.set(false);
                        },
                        {t!("settings.close").to_string()}
                    }
                    button {
                        class: "primary",
                        disabled: rendered.read().is_err(),
                        onclick: handle_save,
                        {t!("selection.export_save").to_string()}
                    }
                }
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("cantara-pptx-test-{name}.pptx"))
    }

    /// The bytes PptxGenJS produced have to reach the disk unchanged — a PPTX
    /// is a ZIP, and one wrong byte makes PowerPoint refuse the whole file.
    #[test]
    fn test_the_decoded_deck_is_written_verbatim() {
        // A minimal ZIP: the signature PowerPoint looks for first.
        let content = b"PK\x03\x04hello";
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(content)
        };
        let path = temp_path("verbatim");

        write_pptx_file(&encoded, &path).expect("the file should be written");

        let written = std::fs::read(&path).expect("the file should exist");
        assert_eq!(written, content);
        assert_eq!(&written[..2], b"PK", "the ZIP signature was mangled");

        let _ = std::fs::remove_file(&path);
    }

    /// An export that produced nothing must say so instead of leaving a
    /// zero-byte file behind that PowerPoint cannot open.

    #[test]
    fn test_empty_data_is_refused() {
        let path = temp_path("empty");

        assert!(write_pptx_file("", &path).is_err());
        assert!(!path.exists(), "an empty file was left behind");
    }

    #[test]
    fn test_damaged_data_is_refused() {
        let path = temp_path("damaged");

        assert!(write_pptx_file("not base64 !!!", &path).is_err());
        assert!(!path.exists(), "a damaged file was left behind");
    }
}
