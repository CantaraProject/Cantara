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
use crate::logic::sourcefiles::SourceFile;
use crate::logic::states::SelectedItemRepresentation;
use cantara_songlib::exporter::abc::{AbcSettings, abc_from_song};
use cantara_songlib::song::Song;
use dioxus::prelude::*;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// The file name on disk, which is what decides a song's format.
///
/// [`SourceFile::name`] is the *display* name — the suffix has been stripped
/// for the list — so passing it to the importer makes every song look like an
/// unknown format.
fn file_name_of(file: &SourceFile) -> String {
    file.path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&file.name)
        .to_string()
}

/// Reads an element's text, wherever it lives.
///
/// The desktop reads the file system; the web build has a virtual file system
/// filled from the bundled repositories.
fn read_text(file: &SourceFile) -> Result<String, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read_to_string(&file.path).map_err(|error| error.to_string())
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::logic::settings::RepositoryType::web_read_file(
            file.path.to_str().unwrap_or_default(),
        )
        .ok_or_else(|| "file not available".to_string())
        .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))
    }
}

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

/// The detail view: the same element list as the selection view, with one
/// element opened beside it.
#[component]
pub fn Detail() -> Element {
    let source_files: Signal<Vec<SourceFile>> = use_context();
    let selected_items: Signal<Vec<SelectedItemRepresentation>> = use_context();
    let active_detailed_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_selection_filter: Signal<SelectionSidebarType> =
        use_signal(|| SelectionSidebarType::Songs);

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
                }
            }

            if search_visible() {
                SearchResults {
                    search_results,
                    query: filter_string,
                    selected_items,
                    search_visible,
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
                    onclick: move |_| {
                        nav.push(crate::Route::SettingsPage {});
                    },
                    {t!("settings.settings_button").to_string()}
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
    let in_detail = matches!(route, crate::Route::Detail {});

    rsx! {
        button {
            class: "outline secondary smaller-buttons",
            onclick: move |_| {
                if in_detail {
                    nav.push(crate::Route::Selection {});
                } else {
                    nav.push(crate::Route::Detail {});
                }
            },
            if in_detail {
                {t!("detail.to_selection").to_string()}
            } else {
                {t!("detail.to_detail").to_string()}
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

    rsx! {
        div { class: "detail-header",
            h3 { {subject.title()} }

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
    let path = file.path.to_str().unwrap_or_default().to_string();

    rsx! {
        div { class: "detail-image",
            img { src: "{path}", alt: "{file.name}" }
        }
    }
}

/// A PDF, one page at a time.
///
/// Reuses the presentation's page renderer, so the document is parsed once per
/// window and paging costs only a short script.
#[component]
fn PdfViewer(file: SourceFile) -> Element {
    let mut page = use_signal(|| 1_u32);
    let path = file.path.to_str().unwrap_or_default().to_string();
    let pages = crate::logic::search::pdf_page_count(&file.path).unwrap_or(1).max(1);

    rsx! {
        div { class: "detail-pdf",
            div { class: "detail-pdf-page",
                crate::components::presentation_components::PdfPageCanvas {
                    key: "{path}-{page()}",
                    pdf_path: path.clone(),
                    page_num: page(),
                }
            }
            div { class: "detail-pdf-controls",
                button {
                    class: "outline",
                    disabled: page() <= 1,
                    onclick: move |_| page.set(page().saturating_sub(1).max(1)),
                    "‹"
                }
                span { "{page()} / {pages}" }
                button {
                    class: "outline",
                    disabled: page() >= pages,
                    onclick: move |_| page.set((page() + 1).min(pages)),
                    "›"
                }
            }
        }
    }
}

#[component]
fn MarkdownViewer(file: SourceFile) -> Element {
    let content = use_memo(use_reactive!(|file| read_text(&file)));

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
    let parsed: Memo<Result<Song, String>> = use_memo(use_reactive!(|file| {
        read_text(&file).and_then(|content| {
            crate::logic::export::song_from_content(&file_name_of(&file), &content)
                .map_err(|error| format!("{error:?}"))
        })
    }));

    rsx! {
        match &*parsed.read() {
            Err(error) => rsx! { p { class: "detail-error", "{error}" } },
            Ok(song) => match tab {
                DetailTab::Notation => rsx! { SongNotation { song: song.clone() } },
                DetailTab::Text | DetailTab::Preview => rsx! { SongText { song: song.clone() } },
            },
        }
    }
}

/// The lyrics, with the parts in the order they are sung and every language of
/// a part side by side.
#[component]
fn SongText(song: Song) -> Element {
    let default_language = song.default_language.clone();

    let tags: Vec<(String, String)> = song
        .tags()
        .iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();

    rsx! {
        div { class: "detail-song-text",
            h4 { "{song.title}" }

            // What the file records about the song — author, copyright, CCLI
            // number and whatever else it carries. Shown once, above the words.
            if !tags.is_empty() {
                dl { class: "detail-song-tags",
                    for (key , value) in tags {
                        dt { {tag_label(&key)} }
                        dd { "{value}" }
                    }
                }
            }

            for (index , part) in song.ordered_parts().into_iter().enumerate() {
                div { key: "{index}", class: "detail-song-part",
                    div { class: "detail-song-part-label",
                        {crate::logic::detail::part_label(&song, part)}
                    }

                    div { class: "detail-song-languages",
                        for (language , content) in part.all_lyrics() {
                            div { class: "detail-song-language",
                                span { class: "detail-song-language-code",
                                    {language_label(language, default_language.as_deref())}
                                }
                                pre { "{content.content}" }
                            }
                        }
                    }

                    // A part may carry a melody without any words of its own.
                    if part.all_lyrics().next().is_none() {
                        p { class: "detail-song-empty", {t!("detail.no_lyrics").to_string()} }
                    }
                }
            }
        }
    }
}

/// How a tag is headed.
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
    let mut draft = use_signal(|| read_text(&file).unwrap_or_default());
    let mut status: Signal<Option<String>> = use_signal(|| None);

    // Re-read when the user opens a different element.
    use_effect(use_reactive!(|file| {
        draft.set(read_text(&file).unwrap_or_default());
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
                        SongDraftPreview { file_name: file_name_of(&file), source: draft() }
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
                SongText { song: song.clone() }
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
        };

        assert_eq!(file_name_of(&file), "Alle Jahre wieder.song");

        // And the display name really would not do.
        assert!(
            crate::logic::export::song_from_content(&file.name, "#title: X\n\nHallo").is_err(),
            "the display name has no suffix, so it cannot select an importer"
        );
        assert!(
            crate::logic::export::song_from_content(&file_name_of(&file), "#title: X\n\nHallo")
                .is_ok(),
            "the real file name has to reach the importer"
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
