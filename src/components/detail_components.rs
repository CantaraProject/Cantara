//! The detail view: looking at one element, and changing it.
//!
//! The selection view answers "what goes into the presentation"; this one
//! answers "what is in this element". The left-hand side — the search and the
//! list of source files — is the *same* set of components the selection view
//! uses, so the two stay in step; only the right-hand side differs.
//!
//! Which element types exist, what they can be looked at through and whether
//! they can be edited is decided in [`crate::logic::detail`], not here. This
//! module only draws what that model describes.

use crate::components::presentation_components::AbcNotationRenderer;
use crate::components::selection_components::sidebar::SelectionFilterSideBar;
use crate::components::selection_components::search_ui::{SearchInput, SearchResults};
use crate::components::selection_components::source_items::{
    ImageSourceItems, ItemClickAction, MarkdownSourceItems, PdfSourceItems, SongSourceItems,
};
use crate::logic::detail::{DetailMode, DetailSubject, DetailTab};
use crate::logic::presentation::get_markdown_html;
use crate::logic::settings::SelectionSidebarType;
use crate::logic::sourcefiles::{read_source_file, SourceFile};
use crate::logic::states::SelectedItemRepresentation;
use cantara_songlib::exporter::abc::{AbcSettings, abc_from_song};
use cantara_songlib::exporter::song_yml::song_yml_from_song;
use cantara_songlib::song::Song;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_solid_icons::{FaCircleInfo, FaGear, FaList, FaPencil, FaPlus};
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// Writes an element's text back.
///
/// The web build has nowhere to write to, so editing there stays a preview.
fn write_text(file: &SourceFile, content: &str) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::write(&file.path, content).map_err(|error| error.to_string())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (file, content);
        Err(t!("detail.no_write_on_web").to_string())
    }
}

/// Renames an element's file, keeping its suffix.
///
/// The list shows the display name, so that is what the user edits; the suffix
/// decides the format and must survive untouched.
#[cfg(not(target_arch = "wasm32"))]
fn rename_element(file: &SourceFile, new_display_name: &str) -> Result<(), String> {
    let trimmed = new_display_name.trim();
    if trimmed.is_empty() {
        return Err(t!("detail.empty_name").to_string());
    }
    // A name is a file name, not a path: a slash would move the file somewhere
    // else entirely.
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(t!("detail.name_has_separator").to_string());
    }

    let current = file.file_name().to_string();
    let suffix = current
        .strip_prefix(&file.name)
        .unwrap_or("")
        .to_string();

    let target = file.path.with_file_name(format!("{trimmed}{suffix}"));
    if target == file.path {
        return Ok(());
    }
    if target.exists() {
        return Err(t!("detail.name_taken").to_string());
    }

    std::fs::rename(&file.path, &target).map_err(|error| error.to_string())
}

#[cfg(target_arch = "wasm32")]
fn rename_element(_file: &SourceFile, _new_display_name: &str) -> Result<(), String> {
    Err(t!("detail.no_write_on_web").to_string())
}

/// Whether a song can be written back after an inline change.
///
/// The song library exports YAML, so a song already kept that way is saved in
/// place. A classic `.song` or a CCLI download is *not* silently rewritten into
/// a different format behind the user's back — that would change what the file
/// is, and Cantara has no importer-preserving writer for those.
fn is_editable_in_place(file: &SourceFile) -> bool {
    let name = file.file_name().to_lowercase();
    name.ends_with(".song.yml") || name.ends_with(".song.yaml")
}

/// Writes a song back to its file as YAML.
fn save_song(file: &SourceFile, song: &Song) -> Result<(), String> {
    let yml = song_yml_from_song(song)?;
    write_text(file, &yml)
}

/// One field that can be changed in place.
///
/// Reading and editing are the same view: the pencil sits in the corner and a
/// double-click on it turns the text into an input. Escape abandons the change,
/// so a mis-click costs nothing.
#[component]
fn InlineEditable(
    /// What the field currently holds.
    value: String,
    /// Whether the text runs over several lines, like a stanza.
    #[props(default)]
    multiline: bool,
    /// Whether the field may be changed at all.
    #[props(default = true)]
    editable: bool,
    /// Called with the new text once the user is done.
    on_commit: EventHandler<String>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| value.clone());

    // A different element may be opened while this field exists.
    use_effect(use_reactive!(|value| {
        draft.set(value.clone());
        editing.set(false);
    }));

    if !editable {
        return rsx! {
            div { class: "inline-editable",
                if multiline {
                    pre { "{value}" }
                } else {
                    span { "{value}" }
                }
            }
        };
    }

    rsx! {
        div { class: "inline-editable",
            if editing() {
                if multiline {
                    textarea {
                        class: "inline-editable-input",
                        rows: "{draft.read().lines().count().max(3)}",
                        spellcheck: false,
                        value: "{draft}",
                        onmounted: move |element| async move {
                            let _ = element.set_focus(true).await;
                        },
                        oninput: move |event| draft.set(event.value()),
                        onblur: {
                            let value = value.clone();
                            move |_| {
                                editing.set(false);
                                let text = draft();
                                if text != value {
                                    on_commit.call(text);
                                }
                            }
                        },
                        onkeydown: move |event: Event<KeyboardData>| {
                            if event.key() == Key::Escape {
                                editing.set(false);
                            }
                        },
                    }
                } else {
                    input {
                        class: "inline-editable-input",
                        r#type: "text",
                        value: "{draft}",
                        onmounted: move |element| async move {
                            let _ = element.set_focus(true).await;
                        },
                        oninput: move |event| draft.set(event.value()),
                        onblur: {
                            let value = value.clone();
                            move |_| {
                                editing.set(false);
                                let text = draft();
                                if text != value {
                                    on_commit.call(text);
                                }
                            }
                        },
                        onkeydown: {
                            let value = value.clone();
                            move |event: Event<KeyboardData>| {
                                match event.key() {
                                    Key::Escape => editing.set(false),
                                    Key::Enter => {
                                        editing.set(false);
                                        let text = draft();
                                        if text != value {
                                            on_commit.call(text);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        },
                    }
                }
            } else {
                if multiline {
                    pre { ondoubleclick: move |_| editing.set(true), "{value}" }
                } else {
                    span { ondoubleclick: move |_| editing.set(true), "{value}" }
                }

                button {
                    r#type: "button",
                    class: "inline-editable-pencil",
                    title: t!("detail.edit_field").to_string(),
                    aria_label: t!("detail.edit_field").to_string(),
                    ondoubleclick: move |_| editing.set(true),
                    Icon { icon: FaPencil }
                }
            }
        }
    }
}

/// Puts the identifier of the open element into the address bar.
///
/// Deliberately not a navigation, which is what this used to be. The whole
/// route tree hangs in an animated outlet, and that outlet cross-fades on any
/// change of the route *value* — not only between one view and another. Going
/// from `/detail` to `/detail/a3f9c2b1` is such a change, so opening an element
/// mounted the detail view a second time next to the one already on screen and
/// faded between them: every element visibly loaded twice.
///
/// A link still *opens* an element through the route, which is the direction
/// that matters; only the write-back goes around the router. Nothing is pushed
/// onto the history either way, so the back button still leaves the view rather
/// than stepping through everything that was looked at in it.
///
/// The desktop has no address bar to keep in step, so there this does nothing.
fn show_element_in_address(id: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        // The address is built from the one in the bar, so that a deployment
        // under a base path — `/Cantara/` on GitHub Pages — is carried over
        // without having to be known here.
        let js = format!(
            "var base = location.pathname.split('/detail')[0].replace(/\\/$/, '');\
             history.replaceState(null, '', base + '/detail/' + {} + location.search + location.hash);",
            serde_json::to_string(id).unwrap_or_else(|_| "''".to_string())
        );
        let _ = document::eval(&js);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
    }
}

/// The detail view: the same element list as the selection view, with one
/// element opened beside it.
///
/// `element` is what the URL carries after `/detail`: the identifier of the
/// element to open, or nothing. The view and the address bar are kept in step
/// in both directions — a link opens an element, and opening an element makes
/// the link, so it can be copied out of the address bar. See
/// [`crate::logic::element_id`] for what the identifier is.
#[component]
pub fn Detail(element: Vec<String>) -> Element {
    let source_files: Signal<Vec<SourceFile>> = use_context();
    let selected_items: Signal<Vec<SelectedItemRepresentation>> = use_context();
    let mut active_detailed_item_id: Signal<Option<usize>> = use_signal(|| None);
    // Not a signal of this view's own: opening an element replaces the address,
    // which re-mounts this component, and a fresh signal would send the list
    // back to the songs every time a picture or a PDF was opened. See
    // [`crate::logic::states::LibraryFilterState`].
    let active_selection_filter: Signal<SelectionSidebarType> =
        use_context::<crate::logic::states::LibraryFilterState>().active;

    // What the URL asks for. Only the first segment means anything, so
    // `/detail/a3f9c2b1/whatever` opens the same element rather than nothing.
    // Held in a signal because the two effects below have to react to it, and a
    // value captured from the props would stay at whatever it was on the first
    // render.
    let mut requested_id: Signal<Option<String>> = use_signal(|| element.first().cloned());
    use_effect(use_reactive!(|element| {
        let from_url = element.first().cloned();
        if *requested_id.peek() != from_url {
            requested_id.set(from_url);
        }
    }));

    // The URL decides which element is open — on arrival, and whenever it
    // changes from outside (the back button, an edited address).
    //
    // The library is read here, so this runs again once the scan has finished:
    // a link that is opened cold arrives long before there is anything to
    // resolve it against. An identifier that names nothing is left alone; the
    // view then shows no element, which is what a link to a song that has since
    // been removed should do.
    use_effect(move || {
        let library = source_files.read();
        let resolved = requested_id()
            .and_then(|id| crate::logic::element_id::resolve(&library, &id));

        if resolved.is_some() && resolved != *active_detailed_item_id.peek() {
            active_detailed_item_id.set(resolved);
        }
    });

    // ... and the other way round: opening an element writes the address, so
    // that it always shows a link to what is on screen and can be copied out.
    //
    // Reading the identifier from the URL with `peek` keeps this effect out of
    // the other direction: it reacts to the element that is open, not to the
    // address that it writes itself.
    use_effect(move || {
        let library = source_files.read();
        let open_id = active_detailed_item_id()
            .and_then(|index| library.get(index))
            .map(|file| crate::logic::element_id::of(file, &library));

        // While the library is still being scanned there is nothing to derive
        // an identifier from; writing the address then would throw away the
        // one the user arrived with before it could be resolved.
        if let Some(id) = open_id
            && Some(&id) != requested_id.peek().as_ref()
        {
            requested_id.set(Some(id.clone()));
            show_element_in_address(&id);
        }
    });

    // The search is the selection view's, so a library is searched the same way
    // whichever view the user is in.
    let filter_string: Signal<String> = use_signal(String::new);
    let mut search_results: Signal<Vec<crate::logic::search::SearchResult>> = use_signal(Vec::new);
    let mut search_visible: Signal<bool> = use_signal(|| false);
    let input_element_signal: Signal<Option<std::rc::Rc<MountedData>>> = use_signal(|| None);

    use_effect(move || {
        let query = filter_string.read().clone();
        if query.is_empty() {
            search_results.set(Vec::new());
            search_visible.set(false);
        } else {
            let results = crate::logic::search::search_source_files(&source_files.read(), &query);
            let has_results = !results.is_empty();
            search_results.set(results);
            search_visible.set(has_results);
        }
    });

    // Bring the open element into view. Opening a song from the search would
    // otherwise leave the list wherever it happened to be scrolled to. Done
    // from here rather than in the item, because an item only learns about a
    // change if it happens to re-mount — the presenter console scrolls its
    // active slide the same way.
    use_effect(move || {
        if active_detailed_item_id().is_some() {
            let _ = document::eval(
                "requestAnimationFrame(function () {
                     var el = document.querySelector('.selection_item-active');
                     if (el) { el.scrollIntoView({ behavior: 'smooth', block: 'nearest' }); }
                 });",
            );
        }
    });

    // The element currently open, derived from the list selection so that the
    // two can never disagree.
    let subject = use_memo(move || {
        active_detailed_item_id()
            .and_then(|index| source_files.read().get(index).cloned())
            .and_then(|file| DetailSubject::of(&file))
    });

    rsx! {
        div { class: "wrapper",
            header { class: "top-bar no-padding",
                SearchInput {
                    input_signal: filter_string,
                    element_signal: input_element_signal,
                    on_escape: move |_| search_visible.set(false),
                }
            }

            if search_visible() {
                SearchResults {
                    search_results,
                    selected_items,
                    search_visible,
                    source_files,
                    active_detailed_item_id,
                    click_action: ItemClickAction::OpenDetail,
                }
            }

            // The same shell the selection view uses: the floating icon bar and
            // the list rely on its layout, and building a grid of my own here
            // pushed them apart.
            main {
                id: "selection-content",
                class: "content content-background height-100",
                onmounted: move |_| async move {
                    let _ = document::eval("initSelectionLayout();").await;
                },
                div { class: "grid swipe-container height-100",
                    div { class: "height-100 swipe-panel",
                        SelectionFilterSideBar { active_selection: active_selection_filter }
                        if active_selection_filter() == SelectionSidebarType::Songs {
                            SongSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                                click_action: ItemClickAction::OpenDetail,
                            }
                        }
                        if active_selection_filter() == SelectionSidebarType::Pictures {
                            ImageSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                                click_action: ItemClickAction::OpenDetail,
                            }
                        }
                        if active_selection_filter() == SelectionSidebarType::Pdfs {
                            PdfSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                                click_action: ItemClickAction::OpenDetail,
                            }
                        }
                        if active_selection_filter() == SelectionSidebarType::Markdown {
                            MarkdownSourceItems {
                                source_files,
                                active_detailed_item_id,
                                selected_items,
                                click_action: ItemClickAction::OpenDetail,
                            }
                        }
                    }

                    // The panel and the scrolling area have to be two elements.
                    // `adjustDivHeight` sizes every `.scrollable-container` and
                    // then clears the height of every `.swipe-panel`; on one
                    // element carrying both classes the second step wipes the
                    // first, leaving the pane unsized.
                    div { class: "swipe-panel",
                        div { class: "detail-pane scrollable-container",
                            match subject() {
                                Some(subject) => rsx! { DetailPane { subject } },
                                None => rsx! {
                                    p { class: "detail-empty", {t!("detail.nothing_open").to_string()} }
                                },
                            }
                        }
                    }
                }
            }

            DetailFooter {}
        }
    }
}

/// The footer of the detail view. Mirrors the selection view's, minus the
/// controls that only make sense while assembling a presentation.
#[component]
fn DetailFooter() -> Element {
    let nav = navigator();

    rsx! {
        footer { class: "bottom-bar",
            div { class: "no-padding width-100", role: "group",
                button {
                    class: "outline secondary smaller-buttons",
                    title: t!("settings.settings_button").to_string(),
                    onclick: move |_| {
                        nav.push(crate::Route::SettingsPage {});
                    },
                    // The selection view's footer says it this way too, and a
                    // button that keeps its words where the ones beside it
                    // have gone to icons looks like a different kind of thing.
                    span { class: "mobile-only",
                        Icon { icon: FaGear }
                    }
                    span { class: "desktop-only", {t!("settings.settings_button").to_string()} }
                }
                ViewModeToggle {}
            }
        }
    }
}

/// Switches between the two ways of working with a library.
///
/// Which one a build starts in differs by platform: the desktop is built around
/// assembling a presentation, while the web version is mostly used to look
/// things up. The button is the same in both, so neither is a dead end.
#[component]
pub fn ViewModeToggle() -> Element {
    let nav = navigator();
    let route: crate::Route = use_route();
    let in_detail = matches!(route, crate::Route::Detail { .. });

    rsx! {
        button {
            class: "outline secondary smaller-buttons",
            title: if in_detail { t!("detail.to_selection").to_string() } else { t!("detail.to_detail").to_string() },
            onclick: move |_| {
                if in_detail {
                    nav.push(crate::Route::Selection {});
                } else {
                    nav.push(crate::Route::Detail { element: vec![] });
                }
            },
            // Like every other button of the bar: the icon where the bar is
            // narrow, the words where there is room. This one had only the
            // words, so on a narrow window it was the one control that still
            // said anything while the rest showed their icons.
            span { class: "mobile-only",
                if in_detail {
                    Icon { icon: FaList }
                } else {
                    Icon { icon: FaCircleInfo }
                }
            }
            span { class: "desktop-only",
                if in_detail {
                    {t!("detail.to_selection").to_string()}
                } else {
                    {t!("detail.to_detail").to_string()}
                }
            }
        }
    }
}

/// One opened element: its heading, its tabs, and the viewer or editor.
#[component]
fn DetailPane(subject: DetailSubject) -> Element {
    let tabs = subject.tabs();
    let mut active_tab = use_signal(|| tabs[0]);
    let mut mode = use_signal(DetailMode::default);

    // A different element may not have the tab the previous one was showing.
    let current_tab = if tabs.contains(&active_tab()) {
        active_tab()
    } else {
        tabs[0]
    };

    let file = subject.source_file().clone();

    rsx! {
        div { class: "detail-header",
            h3 {
                InlineEditable {
                    value: file.name.clone(),
                    // Renaming means moving the file, which the web build
                    // cannot do.
                    editable: cfg!(not(target_arch = "wasm32")),
                    on_commit: {
                        let file = file.clone();
                        move |new_name: String| {
                            if let Err(error) = rename_element(&file, &new_name) {
                                log::error!("could not rename {}: {error}", file.name);
                            }
                        }
                    },
                }
            }

            div { class: "detail-header-actions",
                if subject.is_editable() {
                    button {
                        class: if mode() == DetailMode::Edit { "smaller-buttons" } else { "outline secondary smaller-buttons" },
                        onclick: move |_| {
                            let next = if mode() == DetailMode::Edit {
                                DetailMode::View
                            } else {
                                DetailMode::Edit
                            };
                            mode.set(next);
                        },
                        if mode() == DetailMode::Edit {
                            {t!("detail.stop_editing").to_string()}
                        } else {
                            {t!("detail.edit").to_string()}
                        }
                    }
                }
            }
        }

        if tabs.len() > 1 && mode() == DetailMode::View {
            div { class: "detail-tabs", role: "group",
                for tab in tabs.iter().copied() {
                    button {
                        key: "{tab.id()}",
                        class: if tab == current_tab { "smaller-buttons" } else { "outline secondary smaller-buttons" },
                        onclick: move |_| active_tab.set(tab),
                        {t!(tab.label_key()).to_string()}
                    }
                }
            }
        }

        // Every element type decides here what it looks like. Adding a variant
        // to `DetailSubject` makes this match incomplete, so a new type cannot
        // silently fall through to a blank pane.
        match (&subject, mode()) {
            (DetailSubject::Image(file), _) => rsx! { ImageViewer { file: file.clone() } },
            (DetailSubject::Pdf(file), _) => rsx! { PdfViewer { file: file.clone() } },
            (DetailSubject::Markdown(file), DetailMode::View) => rsx! {
                MarkdownViewer { file: file.clone() }
            },
            (DetailSubject::Markdown(file), DetailMode::Edit) => rsx! {
                TextEditor { file: file.clone(), kind: EditorKind::Markdown }
            },
            (DetailSubject::Song(file), DetailMode::View) => rsx! {
                SongViewer { file: file.clone(), tab: current_tab }
            },
            (DetailSubject::Song(file), DetailMode::Edit) => rsx! {
                TextEditor { file: file.clone(), kind: EditorKind::Song }
            },
        }
    }
}

#[component]
fn ImageViewer(file: SourceFile) -> Element {
    // Inline rather than by path: see [`crate::logic::images`].
    let source = crate::logic::images::image_data_url(&file.path);

    rsx! {
        div { class: "detail-image",
            match source {
                Some(source) => rsx! { img { src: "{source}", alt: "{file.name}" } },
                None => rsx! { p { class: "detail-empty", {t!("detail.picture_unreadable").to_string()} } },
            }
        }
    }
}

/// A PDF, read the way a document is read: scrolled through, one page under
/// the next.
///
/// It used to be paged with a previous/next button, which is how one moves
/// through *slides* — but nothing here is being presented, and looking
/// something up in a twenty-page handout that way is tedious. The pages are
/// drawn as they come near the viewport, so a long document costs no more to
/// open than a short one; see `pdf_scroll_inline.js`.
///
#[component]
fn PdfViewer(file: SourceFile) -> Element {
    let path = file.path.to_str().unwrap_or_default().to_string();

    // Counting the pages means parsing the document, so it is done once per
    // file and not on every render of this view.
    let pages = use_memo(use_reactive!(|file| {
        crate::logic::search::pdf_page_count(&file.path)
            .unwrap_or(1)
            .max(1)
    }));

    rsx! {
        div { class: "detail-pdf",
            crate::components::presentation_components::PdfScrollView {
                pdf_path: path.clone(),
                pages: pages(),
            }
        }
    }
}

#[component]
fn MarkdownViewer(file: SourceFile) -> Element {
    let content = use_memo(use_reactive!(|file| read_source_file(&file)));

    rsx! {
        match &*content.read() {
            Ok(text) => {
                let html = get_markdown_html(text).map(|html| html.to_string());
                match html {
                    Some(html) => rsx! { div { class: "detail-markdown", dangerous_inner_html: "{html}" } },
                    None => rsx! { pre { class: "detail-plain", "{text}" } },
                }
            }
            Err(error) => rsx! { p { class: "detail-error", "{error}" } },
        }
    }
}

/// A song, read either as words or as music.
#[component]
fn SongViewer(file: SourceFile, tab: DetailTab) -> Element {
    let mut song: Signal<Result<Song, String>> = use_signal(|| Err(String::new()));
    let mut status: Signal<Option<String>> = use_signal(|| None);

    // Re-read whenever a different song is opened.
    use_effect(use_reactive!(|file| {
        let parsed = read_source_file(&file).and_then(|content| {
            crate::logic::export::song_from_content(file.file_name(), &content)
                .map_err(|error| format!("{error:?}"))
        });
        song.set(parsed);
        status.set(None);
    }));

    let editable = is_editable_in_place(&file);

    // One place decides what a change means: apply it, write the file, and say
    // so if the write fails. Every editable field routes through here rather
    // than saving on its own.
    let on_changed = {
        let file = file.clone();
        move |updated: Song| {
            match save_song(&file, &updated) {
                Ok(()) => status.set(None),
                Err(error) => status.set(Some(error)),
            }
            song.set(Ok(updated));
        }
    };

    rsx! {
        if !editable {
            p { class: "detail-hint", {t!("detail.only_yml_editable").to_string()} }
        }
        if let Some(message) = status() {
            p { class: "detail-error", role: "alert", "{message}" }
        }

        match &*song.read() {
            Err(error) if error.is_empty() => rsx! {},
            Err(error) => rsx! { p { class: "detail-error", "{error}" } },
            Ok(current) => match tab {
                DetailTab::Notation => rsx! { SongNotation { song: current.clone() } },
                DetailTab::Text | DetailTab::Preview => rsx! {
                    SongText {
                        song: current.clone(),
                        editable,
                        on_changed,
                    }
                },
            },
        }
    }
}

/// The lyrics, with the parts in the order they are sung and every language of
/// a part side by side.
#[component]
fn SongText(
    song: Song,
    /// Whether the song's file can be written back.
    editable: bool,
    /// Called with the changed song; the caller saves it.
    on_changed: EventHandler<Song>,
) -> Element {
    let default_language = song.default_language.clone();

    let tags: Vec<(String, String)> = song
        .tags()
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();

    rsx! {
        div { class: "detail-song-text",
            h4 {
                InlineEditable {
                    value: song.title.clone(),
                    editable,
                    on_commit: {
                        let song = song.clone();
                        move |title: String| {
                            let mut updated = song.clone();
                            updated.title = title;
                            on_changed.call(updated);
                        }
                    },
                }
            }

            // What the file records about the song — author, copyright, CCLI
            // number and whatever else it carries.
            dl { class: "detail-song-tags",
                for (key , value) in tags {
                    dt { {tag_label(&key)} }
                    dd {
                        InlineEditable {
                            key: "{key}",
                            value: value.clone(),
                            editable,
                            on_commit: {
                                let song = song.clone();
                                let key = key.clone();
                                move |new_value: String| {
                                    let mut updated = song.clone();
                                    updated.set_tag(&key, &new_value);
                                    on_changed.call(updated);
                                }
                            },
                        }
                    }
                }
            }

            if editable {
                AddTagButton {
                    song: song.clone(),
                    on_changed,
                }
            }

            for (index , part) in song.ordered_parts().into_iter().enumerate() {
                div { key: "{index}", class: "detail-song-part",
                    div { class: "detail-song-part-header",
                        div { class: "detail-song-part-label", {crate::logic::detail::part_label(&song, part)} }

                        if editable {
                            PartControls {
                                song: song.clone(),
                                part_id: part.id(),
                                on_changed,
                            }
                        }
                    }

                    div { class: "detail-song-languages",
                        for (language , content) in part.all_lyrics() {
                            div { class: "detail-song-language",
                                span { class: "detail-song-language-code",
                                    {language_label(language, default_language.as_deref())}
                                }
                                InlineEditable {
                                    value: content.content.clone(),
                                    multiline: true,
                                    editable,
                                    on_commit: {
                                        let song = song.clone();
                                        let part_id = part.id();
                                        let language = language.clone();
                                        move |text: String| {
                                            let mut updated = song.clone();
                                            // Addressed by part id, not by
                                            // position: a refrain appears
                                            // several times in the sung order
                                            // but is stored once, so editing
                                            // any occurrence changes that one.
                                            if let Some(part) = updated.part_mut(&part_id) {
                                                let wanted =
                                                    cantara_songlib::song::SongPartContentType::Lyrics {
                                                        language: language.clone(),
                                                    };
                                                for content in part.contents.iter_mut() {
                                                    if content.content_type == wanted {
                                                        content.content = text.clone();
                                                    }
                                                }
                                            }
                                            on_changed.call(updated);
                                        }
                                    },
                                }
                            }
                        }
                    }

                    // A part may carry a melody without any words of its own.
                    if part.all_lyrics().next().is_none() {
                        p { class: "detail-song-empty", {t!("detail.no_lyrics").to_string()} }
                    }
                }
            }

            if editable {
                AddPartButton {
                    song: song.clone(),
                    on_changed,
                }

                OrderList {
                    song: song.clone(),
                    on_changed,
                }
            }
        }
    }
}

/// Moving, removing and translating one part.
#[component]
fn PartControls(
    song: Song,
    part_id: cantara_songlib::song::SongPartId,
    on_changed: EventHandler<Song>,
) -> Element {
    use crate::logic::detail::editing;

    let mut language = use_signal(String::new);
    let index = song.parts().iter().position(|part| part.id() == part_id);
    let last = song.parts().len().saturating_sub(1);

    rsx! {
        div { class: "detail-part-controls",
            button {
                r#type: "button",
                class: "outline secondary",
                disabled: index == Some(0),
                title: t!("detail.move_up").to_string(),
                aria_label: t!("detail.move_up").to_string(),
                onclick: {
                    let song = song.clone();
                    move |_| on_changed.call(editing::move_part(&song, &part_id, false))
                },
                "↑"
            }
            button {
                r#type: "button",
                class: "outline secondary",
                disabled: index == Some(last),
                title: t!("detail.move_down").to_string(),
                aria_label: t!("detail.move_down").to_string(),
                onclick: {
                    let song = song.clone();
                    move |_| on_changed.call(editing::move_part(&song, &part_id, true))
                },
                "↓"
            }

            input {
                r#type: "text",
                class: "detail-language-input",
                placeholder: t!("detail.language_placeholder").to_string(),
                value: "{language}",
                oninput: move |event| language.set(event.value()),
            }
            button {
                r#type: "button",
                class: "outline",
                disabled: language.read().trim().is_empty(),
                title: t!("detail.add_language").to_string(),
                onclick: {
                    let song = song.clone();
                    move |_| {
                        let code = language.read().trim().to_string();
                        language.set(String::new());
                        on_changed.call(editing::add_language(&song, &part_id, &code));
                    }
                },
                Icon { icon: FaPlus }
            }

            button {
                r#type: "button",
                class: "outline secondary detail-remove",
                title: t!("detail.remove_part").to_string(),
                aria_label: t!("detail.remove_part").to_string(),
                onclick: {
                    let song = song.clone();
                    move |_| on_changed.call(editing::remove_part(&song, &part_id))
                },
                "✕"
            }
        }
    }
}

/// The ways this song can be sung.
///
/// The first entry is the song's default and stays; the others are the
/// alternatives — a short version for a service, say.
#[component]
fn OrderList(song: Song, on_changed: EventHandler<Song>) -> Element {
    use crate::logic::detail::{editing, order_label};
    use cantara_songlib::song::PartOrderRule;

    let mut name = use_signal(String::new);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let rules: [(&str, PartOrderRule); 2] = [
        (
            "detail.order_verse_first",
            PartOrderRule::VerseRefrainBridgeRefrain,
        ),
        (
            "detail.order_refrain_first",
            PartOrderRule::RefrainVerseBridgeRefrain,
        ),
    ];

    rsx! {
        div { class: "detail-orders",
            h5 { {t!("detail.orders").to_string()} }

            ul {
                for (index , order) in song.part_orders.iter().enumerate() {
                    li { key: "{index}",
                        span { {order_label(order)} }
                        if index > 0 {
                            button {
                                r#type: "button",
                                class: "outline secondary detail-remove",
                                title: t!("detail.remove_order").to_string(),
                                aria_label: t!("detail.remove_order").to_string(),
                                onclick: {
                                    let song = song.clone();
                                    move |_| on_changed.call(editing::remove_order(&song, index))
                                },
                                "✕"
                            }
                        }
                    }
                }
            }

            if let Some(message) = error() {
                p { class: "detail-error", "{message}" }
            }

            div { class: "detail-add-row",
                input {
                    r#type: "text",
                    placeholder: t!("detail.order_name_placeholder").to_string(),
                    value: "{name}",
                    oninput: move |event| name.set(event.value()),
                }
                for (key , rule) in rules {
                    button {
                        key: "{key}",
                        r#type: "button",
                        class: "outline",
                        disabled: name.read().trim().is_empty(),
                        onclick: {
                            let song = song.clone();
                            let rule = rule.clone();
                            move |_| {
                                let wanted = name.read().clone();
                                match editing::add_order(&song, &wanted, rule.clone()) {
                                    Ok(updated) => {
                                        name.set(String::new());
                                        error.set(None);
                                        on_changed.call(updated);
                                    }
                                    Err(message) => error.set(Some(message)),
                                }
                            }
                        },
                        Icon { icon: FaPlus }
                        { t!(key).to_string() }
                    }
                }
            }
        }
    }
}

/// Adds a tag to the song.
#[component]
fn AddTagButton(song: Song, on_changed: EventHandler<Song>) -> Element {
    let mut key = use_signal(String::new);

    rsx! {
        div { class: "detail-add-row",
            input {
                r#type: "text",
                placeholder: t!("detail.new_tag_placeholder").to_string(),
                value: "{key}",
                oninput: move |event| key.set(event.value()),
            }
            button {
                r#type: "button",
                class: "outline",
                disabled: key.read().trim().is_empty(),
                onclick: {
                    let song = song.clone();
                    move |_| {
                        let name = key.read().trim().to_string();
                        if name.is_empty() {
                            return;
                        }
                        let mut updated = song.clone();
                        updated.set_tag(&name, "");
                        key.set(String::new());
                        on_changed.call(updated);
                    }
                },
                Icon { icon: FaPlus }
                { t!("detail.add_tag").to_string() }
            }
        }
    }
}

/// Adds a part to the song.
///
/// Adds it to the *stored* parts. The sung order is derived from those, so a
/// refrain that recurs after every stanza still appears once here — adding a
/// stanza does not mean inserting it into the sequence by hand.
#[component]
fn AddPartButton(song: Song, on_changed: EventHandler<Song>) -> Element {
    use cantara_songlib::song::SongPartType;

    let choices: [(&str, SongPartType); 4] = [
        ("song_part.verse", SongPartType::Verse),
        ("song_part.chorus", SongPartType::Chorus),
        ("song_part.bridge", SongPartType::Bridge),
        ("song_part.prechorus", SongPartType::PreChorus),
    ];

    rsx! {
        div { class: "detail-add-row",
            span { { t!("detail.add_part").to_string() } }
            for (key , part_type) in choices {
                button {
                    key: "{key}",
                    r#type: "button",
                    class: "outline",
                    onclick: {
                        let song = song.clone();
                        move |_| {
                            let mut updated = song.clone();
                            updated.add_part_of_type(part_type, None);
                            on_changed.call(updated);
                        }
                    },
                    Icon { icon: FaPlus }
                    { t!(key).to_string() }
                }
            }
        }
    }
}

/// How a tag is headed./// How a tag is headed.
///
/// The common ones get a translated name; anything else keeps the key the file
/// used, because a song may carry tags Cantara knows nothing about and dropping
/// them would lose information the author put there on purpose.
fn tag_label(key: &str) -> String {
    let translated = match key.to_lowercase().as_str() {
        "author" => Some("detail.tag_author"),
        "composer" => Some("detail.tag_composer"),
        "copyright" => Some("detail.tag_copyright"),
        "ccli" | "ccli_number" | "ccli-number" => Some("detail.tag_ccli"),
        "publisher" => Some("detail.tag_publisher"),
        "year" => Some("detail.tag_year"),
        "melody" => Some("detail.tag_melody"),
        "translation" | "translator" => Some("detail.tag_translation"),
        _ => None,
    };

    match translated {
        Some(key) => t!(key).to_string(),
        None => key.to_string(),
    }
}

/// How a part's language is labelled in the text view.
fn language_label(
    language: &cantara_songlib::song::LyricLanguage,
    default_language: Option<&str>,
) -> String {
    match language {
        cantara_songlib::song::LyricLanguage::Specific(code) => code.clone(),
        cantara_songlib::song::LyricLanguage::Default => default_language
            .map(|code| code.to_string())
            .unwrap_or_else(|| t!("detail.default_language").to_string()),
    }
}

/// The melody, engraved so it can be played from the screen.
#[component]
fn SongNotation(song: Song) -> Element {
    let engraved = use_memo(use_reactive!(|song| {
        abc_from_song(&song, &AbcSettings::default())
    }));

    rsx! {
        match &*engraved.read() {
            Ok(abc) => rsx! {
                div { class: "detail-song-notation",
                    AbcNotationRenderer {
                        key: "{abc.len()}",
                        abc_notation: abc.clone(),
                        notation_font: crate::logic::settings::FontRepresentation::default(),
                        lyrics_font_size: crate::logic::settings::CssSize::Pt(12.0),
                        staff_line_height: 1.0,
                        // This is a page, not a slide: the staff takes the
                        // page's text colour so it stays legible in both the
                        // light and the dark theme.
                        inherit_color: true,
                    }
                }
            },
            Err(error) => rsx! {
                p { class: "detail-error", {t!("detail.no_notation", reason = error.to_string()).to_string()} }
            },
        }
    }
}

/// Which kind of document a [`TextEditor`] is editing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EditorKind {
    Song,
    Markdown,
}

/// Editing a text-based element, with the result shown as it is typed.
///
/// The source stays the thing being edited — for a song that is the `song.yml`
/// with its voices and lyrics — and everything to the right of it is derived
/// from that source on every keystroke: the song library parses it, reports
/// what is wrong, and engraves what is right. So the notation is never
/// something to maintain by hand; it is what the words and voices *mean*.
#[component]
fn TextEditor(file: SourceFile, kind: EditorKind) -> Element {
    let mut draft = use_signal(|| read_source_file(&file).unwrap_or_default());
    let mut status: Signal<Option<String>> = use_signal(|| None);

    // Re-read when the user opens a different element.
    use_effect(use_reactive!(|file| {
        draft.set(read_source_file(&file).unwrap_or_default());
        status.set(None);
    }));

    rsx! {
        div { class: "detail-editor",
            div { class: "detail-editor-source",
                textarea {
                    class: "detail-editor-textarea",
                    spellcheck: false,
                    wrap: "off",
                    value: "{draft}",
                    oninput: move |event| draft.set(event.value()),
                }

                div { class: "detail-editor-actions",
                    button {
                        onclick: {
                            let file = file.clone();
                            move |_| {
                                match write_text(&file, &draft.read()) {
                                    Ok(()) => status.set(Some(t!("detail.saved").to_string())),
                                    Err(error) => status.set(Some(error)),
                                }
                            }
                        },
                        {t!("detail.save").to_string()}
                    }
                    if let Some(message) = status() {
                        span { class: "detail-editor-status", "{message}" }
                    }
                }
            }

            div { class: "detail-editor-preview scrollable-container",
                match kind {
                    EditorKind::Markdown => {
                        let text = draft();
                        let html = get_markdown_html(&text).map(|html| html.to_string());
                        match html {
                            Some(html) => rsx! { div { class: "detail-markdown", dangerous_inner_html: "{html}" } },
                            None => rsx! { pre { class: "detail-plain", "{text}" } },
                        }
                    }
                    EditorKind::Song => rsx! {
                        SongDraftPreview { file_name: file.file_name().to_string(), source: draft() }
                    },
                }
            }
        }
    }
}

/// What the song source currently says — or what is wrong with it.
#[component]
fn SongDraftPreview(file_name: String, source: String) -> Element {
    let parsed: Memo<Result<Song, String>> = use_memo(use_reactive!(|(file_name, source)| {
        crate::logic::export::song_from_content(&file_name, &source)
            .map_err(|error| format!("{error:?}"))
    }));

    rsx! {
        match &*parsed.read() {
            Err(error) => rsx! {
                div { class: "detail-error", role: "alert",
                    strong { {t!("detail.parse_failed").to_string()} }
                    pre { "{error}" }
                }
            },
            Ok(song) => rsx! {
                SongNotation { song: song.clone() }
                // A preview of what is being typed on the left, so nothing here
                // is editable — the source is.
                SongText {
                    song: song.clone(),
                    editable: false,
                    on_changed: move |_| {},
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cantara_songlib::song::{LyricLanguage, SongPartContentType};

    /// The list shows a stripped display name, but the importer needs the
    /// suffix to tell a `.song` from a `.song.yml`. Passing the display name
    /// made every song report an unsupported format.
    #[test]
    fn test_the_importer_gets_the_real_file_name() {
        let file = SourceFile {
            name: "Alle Jahre wieder".to_string(),
            path: std::path::PathBuf::from("/lieder/Alle Jahre wieder.song"),
            file_type: crate::logic::sourcefiles::SourceFileType::Song,
            md5_hash: None,
            relative_path: None,
        };

        assert_eq!(file.file_name(), "Alle Jahre wieder.song");

        // And the display name really would not do.
        assert!(
            crate::logic::export::song_from_content(&file.name, "#title: X\n\nHallo").is_err(),
            "the display name has no suffix, so it cannot select an importer"
        );
        assert!(
            crate::logic::export::song_from_content(file.file_name(), "#title: X\n\nHallo")
                .is_ok(),
            "the real file name has to reach the importer"
        );
    }

    /// Cantara can only write the YAML format, so a classic `.song` or a CCLI
    /// download must not be silently rewritten into something else.
    #[test]
    fn test_only_yml_songs_are_edited_in_place() {
        let of = |path: &str| SourceFile {
            name: "X".to_string(),
            path: std::path::PathBuf::from(path),
            file_type: crate::logic::sourcefiles::SourceFileType::Song,
            md5_hash: None,
            relative_path: None,
        };

        assert!(is_editable_in_place(&of("/l/X.song.yml")));
        assert!(is_editable_in_place(&of("/l/X.SONG.YAML")));
        assert!(!is_editable_in_place(&of("/l/X.song")));
        assert!(!is_editable_in_place(&of("/l/X.ccli")));
    }

    /// A change has to survive the round trip through the exporter, otherwise
    /// saving would quietly drop what the user just typed.
    #[test]
    fn test_an_edit_survives_the_round_trip() {
        let content =
            std::fs::read_to_string("testfiles/Sei nicht stolz auf das, was du bist.song.yml")
                .unwrap();
        let mut song = crate::logic::export::song_from_content(
            "Sei nicht stolz auf das, was du bist.song.yml",
            &content,
        )
        .unwrap();

        song.title = "Ein anderer Titel".to_string();
        song.set_tag("composer", "J. S. Bach");

        let yml = song_yml_from_song(&song).expect("export");
        let reloaded = crate::logic::export::song_from_content("x.song.yml", &yml).unwrap();

        assert_eq!(reloaded.title, "Ein anderer Titel");
        assert_eq!(
            reloaded.tags().get("composer").map(String::as_str),
            Some("J. S. Bach")
        );
        // And nothing else was lost on the way.
        assert_eq!(reloaded.parts().len(), song.parts().len());
        assert!(
            abc_from_song(&reloaded, &AbcSettings::default()).is_ok(),
            "the melody has to survive a save"
        );
    }

    /// A refrain is stored once but sung several times. Editing it has to
    /// change that one stored part, whichever occurrence was clicked.
    #[test]
    fn test_editing_a_refrain_changes_the_single_stored_part() {
        let content =
            std::fs::read_to_string("testfiles/Sei nicht stolz auf das, was du bist.song.yml")
                .unwrap();
        let mut song = crate::logic::export::song_from_content(
            "Sei nicht stolz auf das, was du bist.song.yml",
            &content,
        )
        .unwrap();

        let refrain_id = song
            .parts()
            .iter()
            .find(|part| part.part_type == cantara_songlib::song::SongPartType::Refrain)
            .map(|part| part.id())
            .expect("the reference song has a refrain");

        let occurrences = song
            .ordered_parts()
            .iter()
            .filter(|part| part.id() == refrain_id)
            .count();
        assert!(occurrences > 1, "the refrain should recur");

        let part = song.part_mut(&refrain_id).unwrap();
        for content in part.contents.iter_mut() {
            content.content = "geändert".to_string();
        }

        let changed = song
            .ordered_parts()
            .iter()
            .filter(|part| part.id() == refrain_id)
            .count();
        assert_eq!(
            changed, occurrences,
            "every occurrence shows the one stored part"
        );
    }

    /// A song's metadata is worth reading, and a tag Cantara has no word for
    /// must keep the key the file used rather than disappear.
    #[test]
    fn test_tag_labels() {
        rust_i18n::set_locale("en");

        assert_eq!(tag_label("author"), "Author");
        assert_eq!(tag_label("CCLI"), "CCLI number");
        // Unknown to Cantara, so the author's own wording stands.
        assert_eq!(tag_label("arrangement_note"), "arrangement_note");
    }

    /// A part tagged with a language shows that code; one left at the song's
    /// default shows the song's own language rather than a blank chip.
    #[test]
    fn test_language_labels() {
        assert_eq!(
            language_label(&LyricLanguage::Specific("de".to_string()), Some("en")),
            "de"
        );
        assert_eq!(language_label(&LyricLanguage::Default, Some("de")), "de");
    }

    /// The editor's preview is built from the source on every keystroke, so a
    /// half-typed document has to produce a message rather than panic.
    #[test]
    fn test_incomplete_song_source_reports_instead_of_panicking() {
        let broken = "version: 0.1\ntitle: Test\nparts:\n  - type: stanza\n    contents:\n      - type: voi";

        let result = crate::logic::export::song_from_content("Test.song.yml", broken);

        assert!(result.is_err(), "a broken draft must not parse as a song");
    }

    /// The example the detail view is built around has to read cleanly: parts
    /// in sung order, several languages, and a voice to engrave.
    #[test]
    fn test_the_reference_song_reads() {
        let path = "testfiles/Sei nicht stolz auf das, was du bist.song.yml";
        let content = std::fs::read_to_string(path).unwrap();

        let song = crate::logic::export::song_from_content(
            "Sei nicht stolz auf das, was du bist.song.yml",
            &content,
        )
        .expect("the reference song must parse");

        let parts = song.ordered_parts();
        assert!(parts.len() > 1, "the sung order should repeat parts");

        let has_voice = song.parts().iter().any(|part| {
            part.contents
                .iter()
                .any(|content| content.content_type == SongPartContentType::LeadVoice)
        });
        assert!(has_voice, "the reference song carries a melody");

        assert!(
            abc_from_song(&song, &AbcSettings::default()).is_ok(),
            "the melody has to engrave"
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod rename_tests {
    use super::*;
    use crate::logic::sourcefiles::SourceFileType;

    fn temp_song(dir: &std::path::Path, display_name: &str) -> SourceFile {
        let path = dir.join(format!("{display_name}.song.yml"));
        std::fs::write(&path, "version: 0.1\ntitle: X\nparts: []\n").unwrap();
        SourceFile {
            name: display_name.to_string(),
            path,
            file_type: SourceFileType::Song,
            md5_hash: None,
            relative_path: None,
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cantara-rename-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The user edits the display name, so the suffix has to be put back —
    /// losing it would make the file unreadable to every importer.
    #[test]
    fn test_renaming_keeps_the_suffix() {
        let dir = scratch("suffix");
        let file = temp_song(&dir, "Alt");

        rename_element(&file, "Neu").expect("rename");

        assert!(dir.join("Neu.song.yml").exists());
        assert!(!dir.join("Alt.song.yml").exists());
    }

    /// A name is a file name. A slash would move the file out of the library.
    #[test]
    fn test_a_path_separator_is_refused() {
        let dir = scratch("separator");
        let file = temp_song(&dir, "Alt");

        assert!(rename_element(&file, "../woanders").is_err());
        assert!(dir.join("Alt.song.yml").exists(), "the file must stay put");
    }

    /// Renaming onto an existing file would destroy it.
    #[test]
    fn test_an_existing_name_is_refused() {
        let dir = scratch("taken");
        let file = temp_song(&dir, "Alt");
        let other = temp_song(&dir, "Belegt");
        std::fs::write(&other.path, "belegt").unwrap();

        assert!(rename_element(&file, "Belegt").is_err());
        assert_eq!(std::fs::read_to_string(&other.path).unwrap(), "belegt");
    }

    #[test]
    fn test_an_empty_name_is_refused() {
        let dir = scratch("empty");
        let file = temp_song(&dir, "Alt");

        assert!(rename_element(&file, "   ").is_err());
        assert!(dir.join("Alt.song.yml").exists());
    }
}
