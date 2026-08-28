//! Components for source selection, filtering, and presentation startup.
//!
//! Internal structure:
//! - `search_ui`: search input and result rendering
//! - `source_items`: source lists, detail modal, and drop-file ingestion
//! - `selected_list`: selected item list and reordering UI
//! - `sidebar`: source-category sidebar and ordering
//! - `presentation_options`: per-item presentation option editing
//! - `export_ui`: the export menu and where its files go

mod export_ui;
mod import_ui;
mod presentation_options;
pub(crate) mod search_ui;
mod selected_list;
pub(crate) mod sidebar;
pub(crate) mod source_items;

use self::export_ui::ExportMenu;
use self::presentation_options::PresentationOptions;
use self::search_ui::{SearchInput, SearchResults};
use self::selected_list::SelectedItems;
use self::sidebar::SelectionFilterSideBar;
use self::source_items::{
    process_dropped_files, ImageSourceItems, MarkdownSourceItems, PdfSourceItems, SongSourceItems,
    VideoSourceItems,
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
#[cfg(target_arch = "wasm32")]
use crate::logic::web_storage;
use crate::Route;
use crate::logic::export::{ExportError, ExportFormat, ExportedFile, song_from_content};

use cantara_songlib::song::Song;
use cantara_songlib::slides::SlideSettings;
#[cfg(feature = "desktop")]
use dioxus::desktop::tao;
use dioxus::prelude::*;
use dioxus_free_icons::icons::fa_solid_icons::{FaFileExport, FaGear, FaPlay};
use dioxus_free_icons::Icon;
use rust_i18n::t;
use std::rc::Rc;

rust_i18n::i18n!("locales", fallback = "en");

/// How many panels the narrow layout swipes between: the library, the running
/// order, and the options. There is one dot for each.
const PANEL_COUNT: usize = 3;

/// Brings one of the swipe panels into view.
///
/// The panel is scrolled to rather than the row being scrolled by a computed
/// distance: the container snaps, so bringing the panel into view lands exactly
/// on it whatever the width happens to be.
async fn show_panel(panel_handles: Signal<Vec<Option<Rc<MountedData>>>>, panel: usize) {
    let handle = panel_handles.read().get(panel).cloned().flatten();
    if let Some(handle) = handle {
        let _ = handle.scroll_to(ScrollBehavior::Smooth).await;
    }
}

#[component]
pub fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();

    let filter_string: Signal<String> = use_signal(|| "".to_string());
    let mut search_results: Signal<Vec<SearchResult>> = use_signal(Vec::new);
    let mut search_visible: Signal<bool> = use_signal(|| false);

    let source_files: Signal<Vec<SourceFile>> = use_context();
    let mut selected_items: Signal<Vec<SelectedItemRepresentation>> = use_context();
    let active_selected_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_detailed_item_id: Signal<Option<usize>> = use_signal(|| None);
    // Shared with the detail view and kept across mounts — see
    // [`crate::logic::states::LibraryFilterState`].
    let active_selection_filter: Signal<SelectionSidebarType> =
        use_context::<crate::logic::states::LibraryFilterState>().active;
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    let mut drag_over_source: Signal<bool> = use_signal(|| false);

    let input_element_signal: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    // The three panels the narrow layout swipes between, and which of them is
    // in view. Both used to live in `positioning.js`: the dots were given their
    // `active` class from a scroll listener it attached, and a tap on one
    // called a `scrollToPanel` it defined. The panels are held by their mounted
    // handles so that a tap can scroll one into view without naming it in a
    // selector.
    let mut panel_handles: Signal<Vec<Option<Rc<MountedData>>>> =
        use_signal(|| vec![None; PANEL_COUNT]);
    let mut active_panel: Signal<usize> = use_signal(|| 0);

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

    // Which of the configured designs and divisions that is, is the service's
    // choice — made in the general half of the presentation options.
    let default_presentation_design_memo =
        use_memo(move || settings.read().default_presentation_design());

    let default_song_slide_settings_memo =
        use_memo(move || settings.read().default_song_slide_settings());

    // What the phones are shown, generally: the service's choice, resolved
    // from the design and slide-settings lists the user maintains.
    let stream_defaults_memo = use_memo(move || {
        crate::logic::stream_view::StreamDefaults::of(&settings.read())
    });

    // The installation's tag reading rules, which the slides are built with.
    let tag_mappings_memo = use_memo(move || settings.read().tag_mappings.clone());

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
        let initial_route: crate::logic::states::InitialRouteState = use_context();
        let mut redirected = initial_route.redirected_to_detail;
        use_effect(move || {
            if wizard_completed() && !redirected() {
                redirected.set(true);
                nav.replace(Route::Detail { element: vec![] });
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
            // Alt and a digit take the result of that number. The digit alone
            // used to do it, which meant the search could not be used to look
            // for anything with a number in it — the library has "1000 Gründe"
            // and "1000 reasons", and typing the first `1` picked a result
            // instead of narrowing the search.
            onkeydown: move |event: Event<KeyboardData>| {
                if !search_visible() || !event.modifiers().alt() {
                    return;
                }
                let key = event.key().to_string();
                let Some(digit) = key.chars().next().filter(|_| key.len() == 1).and_then(|c| c.to_digit(10)) else {
                    return;
                };
                let index = if digit == 0 { 9 } else { (digit as usize) - 1 };
                let Some(result) = search_results.read().get(index).cloned() else {
                    return;
                };
                selected_items
                    .write()
                    .push(SelectedItemRepresentation::new_with_sourcefile(result.source_file));
                search_visible.set(false);
                event.prevent_default();
                event.stop_propagation();
            },
            header { class: "top-bar no-padding",
                SearchInput {
                    input_signal: filter_string,
                    element_signal: input_element_signal,
                    on_escape: move |_| search_visible.set(false),
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
                                &stream_defaults_memo(),
                                &tag_mappings_memo(),
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
                                    &stream_defaults_memo(),
                                    &tag_mappings_memo(),
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
                // Typing anywhere in the library goes to the search field, so
                // that looking a song up needs no click first.
                //
                // Which key events arrive here is the whole rule. A field that
                // is being typed into stops its own keys from bubbling this far
                // — see the note on the fields in `presentation_options.rs` —
                // so a key that reaches this handler is one that was pressed
                // with nothing in particular focused. This used to be a script
                // asking the page for its `document.activeElement` on every
                // keystroke, and waiting for the answer before deciding.
                onkeydown: move |event: Event<KeyboardData>| async move {
                    let key = event.key().to_string();
                    if search_visible() && key.len() == 1
                        && key.chars().next().is_some_and(|c| c.is_ascii_digit())
                    {
                        return;
                    }
                    if let Some(searchinput) = input_element_signal() {
                        let _ = searchinput.set_focus(true).await;
                    }
                },
                div {
                    class: "grid swipe-container height-100",
                    // Which panel the swipe is on, so the dots below can say so.
                    // The panels are a scroll-snapping row, so the one in view
                    // is the one whose width the container is scrolled by.
                    onscroll: move |event: Event<ScrollData>| {
                        let width = event.data().client_width();
                        if width <= 0 {
                            return;
                        }
                        let panel =
                            (event.data().scroll_left() / width as f64).round().max(0.0) as usize;
                        if active_panel() != panel {
                            active_panel.set(panel.min(PANEL_COUNT - 1));
                        }
                    },

                    div {
                        class: if drag_over_source() { "height-100 swipe-panel drop-zone drag-active" } else { "height-100 swipe-panel drop-zone" },
                        onmounted: move |event: Event<MountedData>| {
                            panel_handles.write()[0] = Some(event.data());
                        },
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
                        if active_selection_filter() == SelectionSidebarType::Videos {
                            VideoSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                            }
                        }
                    }

                    div {
                        class: "height-100 scrollable-container swipe-panel",
                        onmounted: move |event: Event<MountedData>| {
                            panel_handles.write()[1] = Some(event.data());
                        },
                        if !selected_items.read().is_empty() {
                            SelectedItems {
                                selected_items,
                                active_selected_item_id,
                            }
                        }
                    }

                    div {
                        class: "swipe-panel",
                        onmounted: move |event: Event<MountedData>| {
                            panel_handles.write()[2] = Some(event.data());
                        },
                        PresentationOptions { selected_items, active_selected_item_id }
                    }
                }
            }
            div {
                class: "swipe-indicator",
                role: "tablist",
                aria_label: t!("selection.panels").to_string(),
                for panel in 0..PANEL_COUNT {
                    div {
                        key: "{panel}",
                        class: if active_panel() == panel { "swipe-dot active" } else { "swipe-dot" },
                        role: "tab",
                        tabindex: 0,
                        aria_selected: (active_panel() == panel).to_string(),
                        aria_label: t!("selection.panel", number = panel + 1).to_string(),
                        onclick: move |_| async move { show_panel(panel_handles, panel).await },
                        // A `div` is not a button, however much it is dressed
                        // as one: it takes focus because of its `tabindex` and
                        // then does nothing when it is pressed. These are dots
                        // ten pixels across, and a real `<button>` would come
                        // with Pico's button drawn all over it.
                        onkeydown: move |event: Event<KeyboardData>| {
                            let activated = matches!(event.key(), Key::Enter)
                                || matches!(event.key(), Key::Character(ref c) if c == " ");
                            async move {
                                if activated {
                                    show_panel(panel_handles, panel).await;
                                }
                            }
                        },
                    }
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
                    import_ui::ImportButton { selected_items }
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
                                    &stream_defaults_memo(),
                                    &settings.read(),
                                );
                            } else {
                                presentation::update_presentation(
                                    &selected_items.read(),
                                    &mut running_presentations,
                                    &default_presentation_design_memo(),
                                    &default_song_slide_settings_memo(),
                                    &stream_defaults_memo(),
                                    &tag_mappings_memo(),
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
/// Supports multiscreen placement and optional presenter console.
#[cfg(feature = "desktop")]
fn start_presentation(
    selected_items: &Vec<SelectedItemRepresentation>,
    running_presentations: &mut Signal<Vec<RunningPresentation>>,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    stream_defaults: &crate::logic::stream_view::StreamDefaults,
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
        stream_defaults,
        &settings_read.tag_mappings,
    )
    .is_some()
    {
        let desktop = dioxus::desktop::window();
        let monitors = enumerate_monitors(&desktop);

        let presentation_monitor =
            resolve_monitor(&monitors, &settings_read.presentation_screen, false);

        if let Some(ref monitor) = presentation_monitor
            && let Some(rp) = running_presentations.write().last_mut() {
                rp.presentation_resolution = monitor.size;
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
    stream_defaults: &crate::logic::stream_view::StreamDefaults,
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
        stream_defaults,
        &settings_read.tag_mappings,
    ) else {
        return;
    };

    let nav = navigator();
    if settings_read.show_presenter_console {
        // Store the presentation data in localStorage for the new-tab presentation
        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(json) = serde_json::to_string(&rp) {
                web_storage::write_text(SYNC_KEY_PRESENTATION, &json);
                web_storage::write_text(SYNC_KEY_ACTIVE, "true");
                web_storage::remove_all(&[
                    SYNC_KEY_QUIT,
                    SYNC_KEY_POSITION,
                    SYNC_KEY_POSITION_FROM_CONSOLE,
                ]);
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
                                && let Some(bytes) = RepositoryType::web_read_file(&base_path) {
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
                if !files.is_empty() {
                    web_storage::write(SYNC_KEY_FILES, &files);
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
