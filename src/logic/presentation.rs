//! This module contains functions for creating presentations

use super::{
    settings::PresentationDesign,
    sourcefiles::{SourceFile, SourceFileType},
    states::{RunningPresentation, RunningPresentationPosition, SelectedItemRepresentation, SlideChapter},
    stream_view::{StreamDefaults, map_slides, stream_slide_settings},
};

use crate::logic::tag_mapping::TagMapping;
use cantara_songlib::exporter::slides::slides_from_song;
use cantara_songlib::importer::classic_song::slides_from_classic_song;
use cantara_songlib::slides::{Slide, SlideContent, SimplePictureSlide, SingleLanguageMainContentSlide, SlideSettings};
use dioxus::prelude::*;
use std::{error::Error, path::PathBuf};
// Only the desktop reads a PDF from a path; the web build hands over its bytes.
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use uuid::Uuid;

/// Prefix marker used to identify slides containing rendered Markdown HTML
/// in the `main_text` field of a `SingleLanguageMainContentSlide`.
pub const MARKDOWN_HTML_PREFIX: &str = "<!--md-->";

/// Extracts the picture path from a [SimplePictureSlide] using serde,
/// since the `picture_path` field is private in the external crate.
pub fn get_picture_path(picture_slide: &SimplePictureSlide) -> String {
    match serde_json::to_value(picture_slide) {
        Ok(v) => v
            .get("picture_path")
            .and_then(|p| p.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                log::warn!(
                    "get_picture_path: 'picture_path' field missing or not a string in SimplePictureSlide serialization"
                );
                String::new()
            }),
        Err(err) => {
            log::warn!(
                "get_picture_path: failed to serialize SimplePictureSlide: {}",
                err
            );
            String::new()
        }
    }
}

/// Returns the number of pages in a PDF file using lopdf (desktop only).
#[cfg(not(target_arch = "wasm32"))]
fn get_pdf_page_count(path: &Path) -> Result<usize, Box<dyn Error>> {
    let doc = lopdf::Document::load(path)?;
    Ok(doc.get_pages().len())
}

/// Returns the number of pages in a PDF stored in the web VFS (WASM only).
#[cfg(target_arch = "wasm32")]
fn get_pdf_page_count_from_bytes(bytes: &[u8]) -> Result<usize, Box<dyn Error>> {
    let doc = lopdf::Document::load_mem(bytes)?;
    Ok(doc.get_pages().len())
}

/// This song provides Amazing Grace as a default song which can be used for creating example presentations
const AMAZING_GRACE_SONG: &str = "#title: Amazing Grace
#author: John Newton

Amazing grace
how sweet the sound
that saved a wretch like me.
I once was lost
but now am found,
was blind, but now I see

It was grace that tought
my heart to fear,
and grace my fears relieved:
how precious did that
grace appear the hour
I first believed.

How sweet the name
of Jesus sounds
in a believer's ear.
It soothes his sorrows,
heals the wounds,
and drives away his fear.";

/// Creates slides from markdown content by splitting on `---` separators and
/// rendering each section to HTML using the `markdown` crate.
/// Each slide is stored as a [SingleLanguageMainContentSlide] with the rendered
/// HTML prefixed by [MARKDOWN_HTML_PREFIX] in the `main_text` field.
///
/// The separator is a line containing only `---` (with optional surrounding whitespace),
/// preceded and followed by a newline. Both Unix (`\n`) and Windows (`\r\n`) line endings
/// are supported.
pub fn slides_from_markdown(markdown_content: &str) -> Vec<Slide> {
    // Normalize line endings to \n, then split on lines that are exactly "---"
    let normalized = markdown_content.replace("\r\n", "\n");
    let sections: Vec<&str> = normalized.split("\n---\n").collect();
    let mut slides = Vec::new();

    for section in sections {
        let trimmed = section.trim();
        if trimmed.is_empty() {
            continue;
        }
        let html = markdown::to_html(trimmed);
        let prefixed = format!("{}{}", MARKDOWN_HTML_PREFIX, html);
        // Construct SingleLanguageMainContentSlide via serde since the fields are private
        if let Ok(slide_content) = serde_json::from_value::<SingleLanguageMainContentSlide>(
            serde_json::json!({"main_text": prefixed}),
        ) {
            slides.push(Slide {
                slide_content: SlideContent::SingleLanguageMainContent(slide_content),
                linked_file: None,
            });
        }
    }

    slides
}

/// Checks whether a slide's main text contains rendered Markdown HTML.
/// Returns the HTML content (without the prefix) if it does.
pub fn get_markdown_html(main_text: &str) -> Option<&str> {
    main_text.strip_prefix(MARKDOWN_HTML_PREFIX)
}

/// Renders Markdown source to HTML for reading it as a document.
///
/// Unlike [slides_from_markdown] nothing is split here: a `---` line is a
/// horizontal rule again, because a document is read as a whole and only a
/// presentation needs it cut into slides. Text that already carries the
/// [MARKDOWN_HTML_PREFIX] is passed through, so a slide's stored HTML can be
/// shown without rendering it twice.
pub fn markdown_to_html(source: &str) -> String {
    match get_markdown_html(source) {
        Some(html) => html.to_string(),
        None => markdown::to_html(source),
    }
}

/// Converts HTML to plain text by stripping tags.
/// Block-level elements (p, h1-h6, li, br, div, tr) get newline separators.
pub fn html_to_plain_text(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut tag_name = String::new();
    let mut collecting_tag_name = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            collecting_tag_name = true;
            tag_name.clear();
        } else if ch == '>' {
            in_tag = false;
            collecting_tag_name = false;
            // Insert newline before block-level elements
            let lower = tag_name.to_lowercase();
            let block_tags = [
                "p", "/p", "h1", "h2", "h3", "h4", "h5", "h6", "/h1", "/h2", "/h3", "/h4",
                "/h5", "/h6", "br", "br/", "div", "/div", "li", "/li", "tr", "/tr",
            ];
            if block_tags.iter().any(|t| lower == *t)
                && !result.is_empty()
                && !result.ends_with('\n')
            {
                result.push('\n');
            }
        } else if in_tag {
            if collecting_tag_name {
                if ch.is_whitespace() || ch == '/' && !tag_name.is_empty() {
                    collecting_tag_name = false;
                } else {
                    tag_name.push(ch);
                }
            }
        } else {
            result.push(ch);
        }
    }

    // Decode common HTML entities (&amp; must be last to avoid
    // double-decoding sequences like &amp;lt; → &lt; → <)
    result
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .trim()
        .to_string()
}

/// Creates a presentation from a selected_item_representation and a presentation_design
/// Turn the contents of a song file into slides, choosing the importer from the
/// file name.
///
/// Both builds come through here. The song library can read a file by path and
/// would arrive at the same slides, but it hands back finished slides — and a
/// tag mapping has to reach the song while it is still a song. Reading the
/// bytes here also gives the web build, which has no path on disk, the same
/// converter as the desktop.
///
/// The format dispatch itself lives in [`crate::logic::export::song_from_content`]
/// so that the presentation and the export can never disagree about what a
/// `.ccli` file is.
///
/// Classic `.song` files keep their dedicated converter: that format encodes
/// the presentation order in the file itself, including the `---` spoiler
/// blocks, which cannot be recovered from a parsed `Song`.
/// The `tag_mappings` are the installation's reading rules — see
/// [`crate::logic::tag_mapping`]. They are applied to the parsed song on its
/// way to the slides and never to the file. A classic `.song` has no tags to
/// map, so they do not reach that branch.
pub fn slides_from_song_content(
    content: &str,
    file_name: &str,
    slide_settings: &SlideSettings,
    backup_title: &str,
    tag_mappings: &[TagMapping],
) -> Result<Vec<Slide>, Box<dyn Error>> {
    if file_name.to_lowercase().ends_with(".song") {
        return Ok(slides_from_classic_song(
            content,
            slide_settings,
            backup_title.to_string(),
        ));
    }

    let song = crate::logic::export::song_from_content(file_name, content)
        .map_err(|error| format!("{error:?}"))?;
    let song = crate::logic::tag_mapping::apply(&song, tag_mappings);
    Ok(slides_from_song(&song, slide_settings))
}

fn create_presentation_slides(
    selected_item: &SelectedItemRepresentation,
    default_song_slide_settings: &SlideSettings,
    tag_mappings: &[TagMapping],
) -> Result<Vec<Slide>, Box<dyn Error>> {
    let mut presentation: Vec<Slide> = vec![];

    let slide_settings = selected_item
        .slide_settings_option
        .clone()
        .unwrap_or(default_song_slide_settings.clone());

    if selected_item.source_file.file_type == SourceFileType::Song {
        #[cfg(target_arch = "wasm32")]
        {
            // On web the song is read from the in-memory VFS, so the importer
            // has to be picked from the file name here — there is no path on
            // disk for the library to look at.
            let path_str = selected_item.source_file.path.to_str().unwrap_or("");
            if let Some(content_bytes) = crate::logic::settings::RepositoryType::web_read_file(path_str) {
                let content = String::from_utf8_lossy(&content_bytes);
                let slides = slides_from_song_content(
                    &content,
                    path_str,
                    &slide_settings,
                    &selected_item.source_file.name,
                    tag_mappings,
                )?;
                presentation.extend(slides);
            }
            return Ok(presentation);
        }

        // The file is read here rather than handed to the song library as a
        // path, so that both builds go through one converter. The library's
        // own `slides_from_file` reads the same four formats and would do the
        // same thing — but it hands back finished slides, and the tag mappings
        // have to reach the song while it is still a song.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = &selected_item.source_file.path;
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let content = std::fs::read_to_string(path)?;
            presentation.extend(slides_from_song_content(
                &content,
                file_name,
                &slide_settings,
                &selected_item.source_file.name,
                tag_mappings,
            )?);
        }
    }

    if selected_item.source_file.file_type == SourceFileType::Image {
        let path_str = selected_item
            .source_file
            .path
            .to_str()
            .unwrap_or("")
            .to_string();

        // Use serde to construct SimplePictureSlide since its field is private
        let picture_slide: SimplePictureSlide =
            serde_json::from_value(serde_json::json!({"picture_path": path_str}))?;

        presentation.push(Slide {
            slide_content: SlideContent::SimplePicture(picture_slide),
            linked_file: None,
        });
    }

    // A video is one slide however long it is. The service moves on when the
    // person leading it does, not when the file ends — which is also why a
    // video that is over does not advance anything by itself.
    if selected_item.source_file.file_type == SourceFileType::Video {
        let path_str = selected_item
            .source_file
            .path
            .to_str()
            .unwrap_or("")
            .to_string();

        presentation.push(Slide::new_video_slide(
            path_str,
            selected_item.video_settings.autostart,
            selected_item.video_settings.looping,
        ));
    }

    if selected_item.source_file.file_type == SourceFileType::Pdf {
        let path_str = selected_item
            .source_file
            .path
            .to_str()
            .unwrap_or("")
            .to_string();

        // Which pages the user asked for. A pattern that cannot be read is
        // every page — the field says so while it is being typed, and a
        // half-written one must not make the element disappear from the
        // presentation. See [`crate::logic::pdf_pages`].
        let wanted = crate::logic::pdf_pages::PageSelection::parse(&selected_item.pdf_pages)
            .unwrap_or_default();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let page_count = get_pdf_page_count(&selected_item.source_file.path)?;
            for page in wanted.pages(page_count as u32) {
                let page_path = format!("{}#page={}", path_str, page);
                let picture_slide: SimplePictureSlide =
                    serde_json::from_value(serde_json::json!({"picture_path": page_path}))?;
                presentation.push(Slide {
                    slide_content: SlideContent::SimplePicture(picture_slide),
                    linked_file: None,
                });
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(pdf_bytes) = crate::logic::settings::RepositoryType::web_read_file(&path_str) {
                let page_count = get_pdf_page_count_from_bytes(&pdf_bytes)?;
                for page in wanted.pages(page_count as u32) {
                    let page_path = format!("{}#page={}", path_str, page);
                    let picture_slide: SimplePictureSlide =
                        serde_json::from_value(serde_json::json!({"picture_path": page_path}))?;
                    presentation.push(Slide {
                        slide_content: SlideContent::SimplePicture(picture_slide),
                        linked_file: None,
                    });
                }
            } else {
                log::warn!("Could not read PDF from web VFS: {}", path_str);
            }
        }
    }

    if selected_item.source_file.file_type == SourceFileType::Markdown {
        // Check for inline markdown content first (spontaneous text)
        if let Some(ref inline_content) = selected_item.inline_markdown {
            let slides = slides_from_markdown(inline_content);
            presentation.extend(slides);
            return Ok(presentation);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let path_str = selected_item.source_file.path.to_str().unwrap_or("");
            if let Some(content_bytes) = crate::logic::settings::RepositoryType::web_read_file(path_str) {
                let content = String::from_utf8_lossy(&content_bytes);
                let slides = slides_from_markdown(&content);
                presentation.extend(slides);
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let content = std::fs::read_to_string(&selected_item.source_file.path)?;
            let slides = slides_from_markdown(&content);
            presentation.extend(slides);
        }
    }

    Ok(presentation)
}

/// What the network stream is given for one element of the service: a design
/// of its own where it has one, a second division of the song where it has one,
/// and the map from the projection's slides to that division.
///
/// Nothing at all in the ordinary case. Where the stream is shown the same
/// design and the same division there is no second set to build, no second set
/// to keep in step, and no second reading of the file — the chapter says so by
/// leaving all three empty, and everything downstream falls back to the
/// projection's own slides.
fn stream_view_of(
    selected_item: &SelectedItemRepresentation,
    stream_defaults: &StreamDefaults,
    projection_design: &PresentationDesign,
    projection_slides: &[Slide],
    projection_settings: &SlideSettings,
    tag_mappings: &[TagMapping],
) -> (Option<PresentationDesign>, Option<Vec<Slide>>, Vec<usize>) {
    // A design that happens to be the projection's is not a difference, and
    // recording it as one would put a second preview in the presenter console
    // showing exactly what the first one shows.
    let design = selected_item
        .stream_design_option
        .clone()
        .or_else(|| stream_defaults.design.clone())
        .filter(|design| design != projection_design);

    // Only a song can be divided into slides two ways. A picture is one slide,
    // a document is its pages, a markdown text is its `---` sections — no slide
    // setting moves any of those, and reading the file a second time to be told
    // so again costs a PDF its page count on every rebuild of the running
    // order.
    if selected_item.source_file.file_type != SourceFileType::Song {
        return (design, None, Vec::new());
    }

    let wanted = selected_item
        .stream_slide_settings_option
        .clone()
        .or_else(|| stream_defaults.slide_settings.clone());

    let Some(wanted) = wanted else {
        return (design, None, Vec::new());
    };

    // The line wrap is not the user's alone to choose — see
    // [`crate::logic::stream_view`] for why.
    let settings = stream_slide_settings(projection_settings, &wanted);
    if settings == *projection_settings {
        return (design, None, Vec::new());
    }

    match create_presentation_slides(selected_item, &settings, tag_mappings) {
        Ok(slides) if !slides.is_empty() => {
            let map = map_slides(projection_slides, &slides);
            (design, Some(slides), map)
        }
        // A second reading that fails is not a reason to lose the
        // presentation. The phones fall back to the projection's slides, which
        // is worse than what was asked for and a great deal better than
        // nothing at all.
        _ => (design, None, Vec::new()),
    }
}

/// Whether a chapter is built with a view for the network stream.
///
/// The only thing that separates the three places a chapter is built. A
/// preview of one element, shown beside the settings that made it, has no
/// stream to serve: what the stream would do with the same element is
/// previewed in the presenter console, next to the slide it would differ from.
enum StreamView<'a> {
    Build(&'a StreamDefaults),
    Skip,
}

/// The design and the division an element is actually shown with: its own
/// where it names one, and the service's general choice where it does not.
///
/// One rule, in one place, because the panels in the presentation options
/// *describe* this rule to the user — see
/// [`crate::components::selection_components`] — and a description that has
/// drifted from what the program does is worse than none.
fn used_design_and_settings(
    selected_item: &SelectedItemRepresentation,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
) -> (PresentationDesign, SlideSettings) {
    (
        selected_item
            .presentation_design_option
            .clone()
            .unwrap_or_else(|| default_presentation_design.clone()),
        selected_item
            .slide_settings_option
            .clone()
            .unwrap_or_else(|| default_slide_settings.clone()),
    )
}

/// The chapter an element and its slides come to, once everything about it has
/// been decided.
///
/// Separate from [`chapter_for`] only so that a preview, which shows an
/// element that cannot be read as an element with no slides rather than
/// dropping it, does not have to write the chapter out a second time.
fn assemble_chapter(
    selected_item: &SelectedItemRepresentation,
    used_presentation_design: PresentationDesign,
    used_slide_settings: SlideSettings,
    slides: Vec<Slide>,
    tag_mappings: &[TagMapping],
    stream: StreamView<'_>,
) -> SlideChapter {
    let (stream_design_option, stream_slides, stream_slide_map) = match stream {
        StreamView::Build(stream_defaults) => stream_view_of(
            selected_item,
            stream_defaults,
            &used_presentation_design,
            &slides,
            &used_slide_settings,
            tag_mappings,
        ),
        StreamView::Skip => (None, None, Vec::new()),
    };

    SlideChapter {
        id: Uuid::new_v4(),
        slides,
        source_file: selected_item.source_file.clone(),
        presentation_design_option: Some(used_presentation_design),
        slide_settings_option: Some(used_slide_settings),
        stream_design_option,
        stream_slides,
        stream_slide_map,
        timer_settings_option: selected_item.timer_settings_option.clone(),
        transition_option: selected_item.transition_effect,
        inline_markdown: selected_item.inline_markdown.clone(),
    }
}

/// One element of the service, as a chapter of the presentation.
///
/// An element whose slides cannot be made is an error here and is left out of
/// the presentation by both callers that run one. A preview says the same
/// thing differently — see [`preview_chapter`].
fn chapter_for(
    selected_item: &SelectedItemRepresentation,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    tag_mappings: &[TagMapping],
    stream: StreamView<'_>,
) -> Result<SlideChapter, Box<dyn Error>> {
    let (used_presentation_design, used_slide_settings) = used_design_and_settings(
        selected_item,
        default_presentation_design,
        default_slide_settings,
    );
    let slides = create_presentation_slides(selected_item, &used_slide_settings, tag_mappings)?;

    Ok(assemble_chapter(
        selected_item,
        used_presentation_design,
        used_slide_settings,
        slides,
        tag_mappings,
        stream,
    ))
}

/// The same chapter, for a preview of a single element.
///
/// An element that cannot be read keeps its place here and shows nothing,
/// rather than disappearing: the preview sits beside the settings for *this*
/// element, and an empty frame beside them is the honest answer to settings
/// that produce no slides.
fn preview_chapter(
    selected_item: &SelectedItemRepresentation,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    tag_mappings: &[TagMapping],
) -> SlideChapter {
    let (used_presentation_design, used_slide_settings) = used_design_and_settings(
        selected_item,
        default_presentation_design,
        default_slide_settings,
    );
    let slides = create_presentation_slides(selected_item, &used_slide_settings, tag_mappings)
        .unwrap_or_default();

    assemble_chapter(
        selected_item,
        used_presentation_design,
        used_slide_settings,
        slides,
        tag_mappings,
        StreamView::Skip,
    )
}

/// Adds a presentation to the global running presentations signal
/// Returns the number (id) of the created presentation
/// Builds a [RunningPresentation] from the selected items without writing to
/// any signal. This is the pure computation step used by [add_presentation]
/// and by the web `start_presentation` (which needs the data before opening
/// the presentation tab, i.e. before any signal writes).
pub fn build_presentation(
    selected_items: &Vec<SelectedItemRepresentation>,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    stream_defaults: &StreamDefaults,
    tag_mappings: &[TagMapping],
) -> Option<RunningPresentation> {
    let mut presentation: Vec<SlideChapter> = vec![];

    for selected_item in selected_items {
        match chapter_for(
            selected_item,
            default_presentation_design,
            default_slide_settings,
            tag_mappings,
            StreamView::Build(stream_defaults),
        ) {
            Ok(chapter) => presentation.push(chapter),
            Err(_) => {
                // TODO: Implement error handling, the user should get a message if an error occurs...
            }
        }
    }

    if !presentation.is_empty() {
        Some(RunningPresentation::new(presentation))
    } else {
        None
    }
}

#[cfg(feature = "desktop")]
pub fn add_presentation(
    selected_items: &Vec<SelectedItemRepresentation>,
    running_presentations: &mut Signal<Vec<RunningPresentation>>,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    stream_defaults: &StreamDefaults,
    tag_mappings: &[TagMapping],
) -> Option<usize> {
    // Right now, we only allow one running presentation at the same time.
    // Later, Cantara is going to support multiple presentations.
    if running_presentations.len() > 0 {
        running_presentations.write().clear();
    }

    if let Some(rp) = build_presentation(
        selected_items,
        default_presentation_design,
        default_slide_settings,
        stream_defaults,
        tag_mappings,
    ) {
        running_presentations
            .write()
            .push(rp);
        return Some(running_presentations.len() - 1);
    }

    None
}

/// Creates a preview presentation from a single selected item with its settings.
/// Falls back to defaults when the item has no custom design or slide settings.
pub fn create_single_item_presentation(
    selected_item: &SelectedItemRepresentation,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    tag_mappings: &[TagMapping],
) -> RunningPresentation {
    let chapter = preview_chapter(
        selected_item,
        default_presentation_design,
        default_slide_settings,
        tag_mappings,
    );

    RunningPresentation::new(vec![chapter])
}

/// Creates an example presentation with the song Amazing Grace and a given presentation design
pub fn create_amazing_grace_presentation(
    presentation_design: &PresentationDesign,
    slide_settings: &SlideSettings,
) -> RunningPresentation {
    let slides = slides_from_classic_song(
        AMAZING_GRACE_SONG,
        slide_settings,
        "Amazing Grace".to_string(),
    );
    let source_file = SourceFile {
        name: "Amazing Grace (Example)".to_string(),
        path: PathBuf::new(),
        file_type: SourceFileType::Song,
        md5_hash: None,
        relative_path: None,
    };
    let slide_chapter = SlideChapter::new(
        slides,
        source_file,
        Some(presentation_design.clone()),
        Some(slide_settings.clone()),
    );

    RunningPresentation::new(vec![slide_chapter])
}

/// Updates a running presentation in-place by regenerating slide chapters
/// from the current selection, while preserving the viewing position.
///
/// Chapters are always fully regenerated from the selected items (so changes
/// to settings like style or max lines per slide take effect). The current
/// viewing position is restored by matching the old chapter's UUID to the
/// new chapter set. If the current chapter was removed, the position falls
/// back to the first chapter or `None` if no chapters remain.
/// Pure computation for [`update_presentation`]: regenerates chapters from
/// `selected_items` and computes the new position, preserving all other fields
/// from `old_rp` (black screen state, resolution, scroll position).
///
/// Separated from the signal-mutating wrapper so it can be unit-tested without
/// a Dioxus runtime.
fn apply_presentation_update(
    old_rp: RunningPresentation,
    selected_items: &[SelectedItemRepresentation],
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    stream_defaults: &StreamDefaults,
    tag_mappings: &[TagMapping],
) -> RunningPresentation {
    // Remember current position for restoration
    let old_position = old_rp.position.clone();
    let old_chapter_id = old_position.as_ref().and_then(|pos| {
        old_rp.presentation.get(pos.chapter()).map(|ch| ch.id)
    });
    let old_chapter_slide = old_position
        .as_ref()
        .map(|pos| pos.chapter_slide())
        .unwrap_or(0);

    // Generate new chapters from current selection.
    // Each chapter gets a fresh UUID — we do NOT reuse old UUIDs because the
    // user may have changed settings (style, max lines, etc.) and the slides
    // are fully regenerated. The old UUID is only used to find which new
    // chapter corresponds to the one the user was viewing.
    let mut new_chapters: Vec<SlideChapter> = vec![];

    // Build a fingerprint → queue mapping for position tracking.
    //
    // The fingerprint is (source_file.path, source_file.md5_hash, inline_markdown).
    // Using all three fields correctly distinguishes items that share the same path
    // but have different content (e.g. two inline-text items, or a file whose hash
    // changed). For truly identical items (same fingerprint) we use FIFO order, which
    // is the best possible behaviour when the items are indistinguishable.
    //
    // This replaces the old Vec<(PathBuf, Uuid)> + linear-scan approach, which
    // matched only by path and could carry the wrong UUID to the wrong chapter
    // when the same path appeared more than once and the selection was reordered.
    type ChapterKey = (std::path::PathBuf, Option<String>, Option<String>);
    let mut old_key_ids: std::collections::HashMap<ChapterKey, std::collections::VecDeque<Uuid>> =
        std::collections::HashMap::new();
    for ch in &old_rp.presentation {
        let key: ChapterKey = (
            ch.source_file.path.clone(),
            ch.source_file.md5_hash.clone(),
            ch.inline_markdown.clone(),
        );
        old_key_ids.entry(key).or_default().push_back(ch.id);
    }

    for selected_item in selected_items {
        match chapter_for(
            selected_item,
            default_presentation_design,
            default_slide_settings,
            tag_mappings,
            StreamView::Build(stream_defaults),
        ) {
            Ok(mut chapter) => {
                // Carry the old UUID for this content fingerprint (FIFO within
                // identical fingerprints) so we can restore the viewing
                // position. Temporary: a fresh UUID is assigned once the
                // position-restore step below has found the chapter again.
                let key: ChapterKey = (
                    selected_item.source_file.path.clone(),
                    selected_item.source_file.md5_hash.clone(),
                    selected_item.inline_markdown.clone(),
                );
                if let Some(carried_id) = old_key_ids.get_mut(&key).and_then(|q| q.pop_front()) {
                    chapter.id = carried_id;
                }

                new_chapters.push(chapter);
            }
            Err(_) => { /* skip failed items */ }
        }
    }

    // Determine new position
    let new_position = if new_chapters.is_empty() {
        None
    } else if let Some(target_id) = old_chapter_id {
        // Try to find the old chapter in the new set by its carried UUID
        if let Some(new_ch_idx) = new_chapters.iter().position(|ch| ch.id == target_id) {
            let slide_count = new_chapters[new_ch_idx].slides.len();
            if slide_count == 0 {
                // Chapter exists but has no slides — fall back to first chapter
                RunningPresentationPosition::new(&new_chapters)
            } else {
                let clamped_slide = old_chapter_slide.min(slide_count - 1);
                // The running number the slide now has, worked out where every
                // other running number is — see [`crate::logic::states::slides_before`].
                let total = crate::logic::states::slides_before(
                    &new_chapters,
                    new_ch_idx,
                    crate::logic::states::Division::Projection,
                ) + clamped_slide;
                Some(RunningPresentationPosition::from_raw(
                    new_ch_idx,
                    clamped_slide,
                    total,
                ))
            }
        } else {
            // Current chapter was deleted; fall back to first chapter
            RunningPresentationPosition::new(&new_chapters)
        }
    } else {
        RunningPresentationPosition::new(&new_chapters)
    };

    // Now assign fresh UUIDs to all chapters so they don't carry stale old IDs
    for ch in &mut new_chapters {
        ch.id = Uuid::new_v4();
    }

    RunningPresentation {
        presentation: new_chapters,
        position: new_position,
        // Preserve fields that are unrelated to content regeneration
        is_black_screen: old_rp.is_black_screen,
        presentation_resolution: old_rp.presentation_resolution,
        markdown_scroll_position: old_rp.markdown_scroll_position,
        presentation_layout: old_rp.presentation_layout,
        // Rebuilding the slides is not a reason to stop a video that is
        // playing: the running order is updated while the service runs, and
        // the element that is up may well be untouched by the change.
        video: old_rp.video.clone(),
    }
}

pub fn update_presentation(
    selected_items: &[SelectedItemRepresentation],
    running_presentations: &mut Signal<Vec<RunningPresentation>>,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
    stream_defaults: &StreamDefaults,
    tag_mappings: &[TagMapping],
) {
    // Must have a running presentation to update
    let Some(old_rp) = running_presentations.peek().first().cloned() else {
        return;
    };

    let updated = apply_presentation_update(
        old_rp,
        selected_items,
        default_presentation_design,
        default_slide_settings,
        stream_defaults,
        tag_mappings,
    );

    // Update the running presentation in-place (preserves window state)
    if let Some(first) = running_presentations.write().first_mut() {
        first.presentation = updated.presentation;
        first.position = updated.position;
        // Keep: is_black_screen, presentation_resolution, markdown_scroll_position
    }

    // On web, immediately sync the updated state to localStorage so the synced
    // presentation tab picks it up. Without this, the presenter console's
    // use_effect might not be mounted yet (e.g. user is on the selection page),
    // and the presentation tab would keep reading stale data from
    // SYNC_KEY_POSITION_FROM_CONSOLE. We also clear SYNC_KEY_POSITION to
    // prevent the presentation tab's old state from being read back by the
    // presenter console and reverting the update.
    #[cfg(target_arch = "wasm32")]
    {
        use super::settings::RepositoryType;
        use super::sync::{SYNC_KEY_FILES, SYNC_KEY_POSITION, SYNC_KEY_POSITION_FROM_CONSOLE};
        use super::web_storage;
        use std::collections::HashMap;

        if let Some(rp) = running_presentations.peek().first()
            && let Ok(json) = serde_json::to_string(rp) {
                // Collect VFS files (e.g. PDFs) so the synced tab can render them
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

                web_storage::write_text(SYNC_KEY_POSITION_FROM_CONSOLE, &json);
                web_storage::remove(SYNC_KEY_POSITION);
                // Sync VFS files if there are any PDFs
                if !files.is_empty() {
                    web_storage::write(SYNC_KEY_FILES, &files);
                }
            }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use crate::logic::{
        sourcefiles::{SourceFile, SourceFileType},
        states::SelectedItemRepresentation,
    };

    use super::*;
    use cantara_songlib::slides::LanguageConfiguration;

    /// Prints one complex slide, so the exact ABC handed to abcjs can be seen
    /// with `cargo test dump_complex_slide -- --nocapture --ignored`.
    #[test]
    #[ignore = "diagnostic output, not an assertion"]
    fn dump_complex_slide() {
        use cantara_songlib::slides::{
            LanguageConfiguration, ShowMetaInformation, SlideElement, SlideRowKind,
        };

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let settings = SlideSettings {
            title_slide: false,
            empty_last_slide: false,
            show_spoiler: true,
            max_lines: Some(2),
            meta_syntax: String::new(),
            show_meta_information: ShowMetaInformation::none(),
            language: LanguageConfiguration::Complex(vec![
                SlideElement::Notation,
                SlideElement::Lyrics("en".to_string()),
            ]),
        };

        let slides =
            slides_from_song_content(&content, "Amazing Grace.song.yml", &settings, "x", &[]).unwrap();

        for (index, slide) in slides.iter().enumerate() {
            if let SlideContent::Complex(complex) = &slide.slide_content {
                println!("--- slide {index} ({} lines)", complex.line_count);
                for row in &complex.rows {
                    match &row.kind {
                        SlideRowKind::Notation { syllables } => {
                            println!("  [notation, {syllables} syllables]");
                            for line in row.content.lines() {
                                println!("    {line}");
                            }
                        }
                        SlideRowKind::Lyrics { language } => println!(
                            "  [{}{}] {}",
                            language.clone().unwrap_or("-".into()),
                            if row.redundant { ", repeat" } else { "" },
                            row.content.replace('\n', " / ")
                        ),
                    }
                }
            }
        }
    }

    /// The point of the whole tag mapping: a template asking for a name this
    /// collection does not use still fills.
    ///
    /// `Amazing Grace.song.yml` records `author` and has no `composer`, so
    /// `{{composer}}` renders empty — until a mapping says the one may be read
    /// as the other.
    #[test]
    fn a_tag_mapping_fills_a_template_the_song_cannot() {
        use crate::logic::tag_mapping::TagMapping;
        use cantara_songlib::slides::ShowMetaInformation;

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let settings = SlideSettings {
            meta_syntax: "{{composer}}".to_string(),
            show_meta_information: ShowMetaInformation::title_first_slide_last_slide(),
            ..SlideSettings::default()
        };

        let without =
            slides_from_song_content(&content, "Amazing Grace.song.yml", &settings, "x", &[])
                .unwrap();
        assert!(
            !serde_json::to_string(&without).unwrap().contains("John Newton"),
            "the song has no composer, so nothing should have been rendered"
        );

        let with = slides_from_song_content(
            &content,
            "Amazing Grace.song.yml",
            &settings,
            "x",
            &[TagMapping::new("author", "composer")],
        )
        .unwrap();
        assert!(
            serde_json::to_string(&with).unwrap().contains("John Newton"),
            "the mapping did not reach the slides"
        );

        // And the file it was read from says what it always said.
        assert_eq!(
            content,
            std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap()
        );
    }

    /// The meta information template configured in the slide settings has to
    /// reach the finished slides — it was never rendered before, and the
    /// settings were never persisted either.
    #[test]
    fn test_meta_information_reaches_the_slides() {
        use cantara_songlib::slides::ShowMetaInformation;

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let settings = SlideSettings {
            title_slide: true,
            empty_last_slide: false,
            show_spoiler: false,
            max_lines: None,
            meta_syntax: "{{title}} ({{author}})".to_string(),
            show_meta_information: ShowMetaInformation::title_first_slide_last_slide(),
            language: LanguageConfiguration::SingleLanguage(None),
        };

        let slides =
            slides_from_song_content(&content, "Amazing Grace.song.yml", &settings, "x", &[]).unwrap();

        let carrying = slides.iter().filter(|slide| slide.has_meta_text()).count();
        assert!(carrying > 0, "no slide carries the meta information");

        let rendered = serde_json::to_string(&slides).unwrap();
        assert!(
            rendered.contains("Amazing Grace (John Newton)"),
            "the template was not rendered into the slides"
        );

        // And with the metadata switched off, no slide carries one.
        let settings = SlideSettings {
            show_meta_information: ShowMetaInformation::none(),
            ..settings
        };
        let slides =
            slides_from_song_content(&content, "Amazing Grace.song.yml", &settings, "x", &[]).unwrap();
        assert_eq!(slides.iter().filter(|slide| slide.has_meta_text()).count(), 0);
    }

    /// "On every slide" reaches the slides in between, not only the ones at
    /// either end — the middle verse of a three-verse hymn is exactly where the
    /// older settings could not put a copyright line.
    #[test]
    fn test_meta_information_on_every_slide() {
        use cantara_songlib::slides::ShowMetaInformation;

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let settings = SlideSettings {
            title_slide: false,
            empty_last_slide: false,
            show_spoiler: false,
            max_lines: None,
            meta_syntax: "{{title}} ({{author}})".to_string(),
            show_meta_information: ShowMetaInformation::all_slides(),
            language: LanguageConfiguration::SingleLanguage(None),
        };

        let slides =
            slides_from_song_content(&content, "Amazing Grace.song.yml", &settings, "x", &[])
                .unwrap();

        assert!(slides.len() > 2, "a hymn with a middle to speak of");
        assert!(
            slides.iter().all(|slide| slide.has_meta_text()),
            "a slide was left without the meta information"
        );
    }

    /// Switching the layout in the settings has to change the kind of slide
    /// that comes out.
    #[test]
    fn test_the_layout_setting_changes_the_slides() {
        use cantara_songlib::slides::{ShowMetaInformation, SlideElement};

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let base = SlideSettings {
            title_slide: false,
            empty_last_slide: false,
            show_spoiler: false,
            max_lines: None,
            meta_syntax: String::new(),
            show_meta_information: ShowMetaInformation::none(),
            language: LanguageConfiguration::SingleLanguage(None),
        };

        let simple =
            slides_from_song_content(&content, "Amazing Grace.song.yml", &base, "x", &[]).unwrap();
        assert!(simple.iter().all(|slide| !matches!(
            slide.slide_content,
            SlideContent::Complex(_)
        )));

        let complex_settings = SlideSettings {
            language: LanguageConfiguration::Complex(vec![
                SlideElement::Notation,
                SlideElement::Lyrics("en".to_string()),
            ]),
            ..base
        };
        let complex =
            slides_from_song_content(&content, "Amazing Grace.song.yml", &complex_settings, "x", &[])
                .unwrap();
        assert!(
            complex
                .iter()
                .any(|slide| matches!(slide.slide_content, SlideContent::Complex(_))),
            "the complex layout produced no complex slides"
        );
    }

    #[test]
    fn test_presentation_creation_from_amazing_grace() {
        let select_item = SelectedItemRepresentation::for_test(
            "Amazing Grace",
            "testfiles/Amazing Grace.song",
            SourceFileType::Song,
        );
        assert!(create_presentation_slides(&select_item, &SlideSettings::default(), &[]).is_ok());
    }

    /// A song the projection divides two lines at a time and the stream four.
    /// Both sets are generated, and every projection slide is mapped to the
    /// stream slide that holds it — that mapping is what keeps a phone in step
    /// with a wall the two do not change together with.
    #[test]
    fn a_stream_division_is_built_and_mapped() {
        let item = amazing_grace();
        let projection = SlideSettings {
            max_lines: Some(2),
            ..SlideSettings::default()
        };
        let stream = SlideSettings {
            max_lines: Some(4),
            ..SlideSettings::default()
        };

        let rp = build_presentation(
            &vec![item],
            &PresentationDesign::default(),
            &projection,
            &StreamDefaults {
                design: None,
                slide_settings: Some(stream),
            },
                    &[],
)
        .expect("a presentation");
        let chapter = &rp.presentation[0];

        let stream_slides = chapter
            .stream_slides
            .as_ref()
            .expect("the stream was given a division of its own");
        assert!(
            stream_slides.len() < chapter.slides.len(),
            "four lines at a time is fewer slides than two: {} against {}",
            stream_slides.len(),
            chapter.slides.len()
        );
        assert_eq!(
            chapter.stream_slide_map.len(),
            chapter.slides.len(),
            "every slide of the projection is mapped"
        );
        assert!(
            chapter
                .stream_slide_map
                .iter()
                .all(|&index| index < stream_slides.len()),
            "and mapped to a slide that exists: {:?}",
            chapter.stream_slide_map
        );
        assert!(
            chapter
                .stream_slide_map
                .windows(2)
                .all(|pair| pair[0] <= pair[1]),
            "and never backwards: {:?}",
            chapter.stream_slide_map
        );
        assert!(chapter.stream_differs());
    }

    /// A service that asks for the same division gets no second set of slides
    /// — and, more to the point, the song is not read and converted twice to
    /// arrive at the same answer.
    #[test]
    fn the_same_division_builds_no_second_set() {
        let settings = SlideSettings {
            max_lines: Some(2),
            ..SlideSettings::default()
        };

        let rp = build_presentation(
            &vec![amazing_grace()],
            &PresentationDesign::default(),
            &settings,
            &StreamDefaults {
                design: None,
                slide_settings: Some(settings.clone()),
            },
                    &[],
)
        .expect("a presentation");

        assert!(rp.presentation[0].stream_slides.is_none());
        assert!(!rp.presentation[0].stream_differs());
    }

    /// An element may single itself out, and what it says wins over the
    /// service's general choice.
    #[test]
    fn an_elements_own_choice_beats_the_general_one() {
        let mut item = amazing_grace();
        item.stream_slide_settings_option = Some(SlideSettings {
            max_lines: None,
            ..SlideSettings::default()
        });

        let rp = build_presentation(
            &vec![item],
            &PresentationDesign::default(),
            &SlideSettings {
                max_lines: Some(2),
                ..SlideSettings::default()
            },
            &StreamDefaults {
                design: None,
                slide_settings: Some(SlideSettings {
                    max_lines: Some(4),
                    ..SlideSettings::default()
                }),
            },
                    &[],
)
        .expect("a presentation");

        let chapter = &rp.presentation[0];
        let stream_slides = chapter.stream_slides.as_ref().expect("a division of its own");
        assert!(
            stream_slides.len() < chapter.slides.len(),
            "whole verses, not four lines at a time"
        );
        assert_eq!(chapter.stream_slide_map.len(), chapter.slides.len());
    }

    /// The wrap the service asked for is reconciled against the projection's on
    /// the way in, so a stream slide can never hold part of a projected one.
    #[test]
    fn a_wrap_that_does_not_divide_is_reconciled_before_it_is_used() {
        let three_lines_at_a_time = SlideSettings {
            max_lines: Some(3),
            ..SlideSettings::default()
        };

        let rp = build_presentation(
            &vec![amazing_grace()],
            &PresentationDesign::default(),
            &SlideSettings {
                max_lines: Some(2),
                ..SlideSettings::default()
            },
            &StreamDefaults {
                design: None,
                slide_settings: Some(three_lines_at_a_time),
            },
                    &[],
)
        .expect("a presentation");
        let chapter = &rp.presentation[0];

        // Three is not a multiple of two, so four is what was used — and the
        // proof is that every projection slide still lands inside exactly one
        // stream slide, which is what a straddle would break.
        let stream_slides = chapter.stream_slides.as_ref().expect("a division of its own");
        for (slide, &mapped) in chapter.slides.iter().zip(&chapter.stream_slide_map) {
            let shown = slide_text(slide);
            if shown.is_empty() {
                continue;
            }
            let holding = slide_text(&stream_slides[mapped]);
            assert!(
                shown.iter().all(|line| holding.contains(line)),
                "the projection showed {shown:?}, the phones {holding:?}"
            );
        }
    }

    fn amazing_grace() -> SelectedItemRepresentation {
        SelectedItemRepresentation::for_test(
            "Amazing Grace",
            "testfiles/Amazing Grace.song",
            SourceFileType::Song,
        )
    }

    /// The words a slide shows, for comparing one against another.
    fn slide_text(slide: &Slide) -> Vec<String> {
        let text = match &slide.slide_content {
            SlideContent::SingleLanguageMainContent(main) => main.clone().main_text(),
            SlideContent::Title(title) => title.title_text.clone(),
            _ => String::new(),
        };
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// A video is one slide however long it is: the service moves on when the
    /// person leading it does, not when the file ends. And the way it is to be
    /// played travels with the slide, because the projection, the console and
    /// every phone have to agree on it.
    #[test]
    fn test_a_video_becomes_one_slide_carrying_its_settings() {
        use crate::logic::states::VideoSettings;

        let mut select_item = SelectedItemRepresentation::new_with_sourcefile(SourceFile {
            name: "Intro".to_string(),
            path: PathBuf::from_str("testfiles/Intro.mp4").unwrap(),
            file_type: SourceFileType::Video,
            md5_hash: None,
            relative_path: None,
        });
        select_item.video_settings = VideoSettings {
            autostart: false,
            looping: true,
        };

        let slides = create_presentation_slides(&select_item, &SlideSettings::default(), &[])
            .expect("a video makes slides");

        assert_eq!(slides.len(), 1, "a video is one slide, whatever its length");
        match &slides[0].slide_content {
            SlideContent::Video(video) => {
                assert!(video.video_path.ends_with("Intro.mp4"));
                assert!(!video.autostart);
                assert!(video.looping);
            }
            other => panic!("expected a video slide, got {other:?}"),
        }
    }

    /// The ordinary case, and the one an operator gets without touching
    /// anything: it starts by itself and does not repeat.
    #[test]
    fn test_a_video_starts_by_itself_and_does_not_repeat_by_default() {
        let select_item = SelectedItemRepresentation::new_with_sourcefile(SourceFile {
            name: "Intro".to_string(),
            path: PathBuf::from_str("testfiles/Intro.mp4").unwrap(),
            file_type: SourceFileType::Video,
            md5_hash: None,
            relative_path: None,
        });

        let slides = create_presentation_slides(&select_item, &SlideSettings::default(), &[])
            .expect("a video makes slides");

        match &slides[0].slide_content {
            SlideContent::Video(video) => {
                assert!(video.autostart, "a video on the wall waiting to be started is a pause");
                assert!(!video.looping);
            }
            other => panic!("expected a video slide, got {other:?}"),
        }
    }

    #[test]
    fn test_presentation_creation_from_pdf() {
        let select_item = SelectedItemRepresentation::for_test(
            "Example",
            "testfiles/Example.pdf",
            SourceFileType::Pdf,
        );
        let result = create_presentation_slides(&select_item, &SlideSettings::default(), &[]);
        assert!(result.is_ok());
        let slides = result.unwrap();
        // Example.pdf has 1 page, so 1 slide
        assert_eq!(slides.len(), 1);
        assert!(matches!(
            slides[0].slide_content,
            SlideContent::SimplePicture(_)
        ));
        // Verify the page fragment is encoded in the path
        if let SlideContent::SimplePicture(ref ps) = slides[0].slide_content {
            let path = get_picture_path(ps);
            assert!(path.ends_with("#page=1"));
        }
    }

    #[test]
    fn test_presentation_creation_from_multipage_pdf() {
        let select_item = SelectedItemRepresentation::for_test(
            "MultiPage",
            "testfiles/MultiPage.pdf",
            SourceFileType::Pdf,
        );
        let result = create_presentation_slides(&select_item, &SlideSettings::default(), &[]);
        assert!(result.is_ok());
        let slides = result.unwrap();
        // MultiPage.pdf has 3 pages, so 3 slides
        assert_eq!(slides.len(), 3);
        for (i, slide) in slides.iter().enumerate() {
            assert!(matches!(slide.slide_content, SlideContent::SimplePicture(_)));
            if let SlideContent::SimplePicture(ref ps) = slide.slide_content {
                let path = get_picture_path(ps);
                assert!(path.ends_with(&format!("#page={}", i + 1)));
            }
        }
    }

    /// The pattern has to reach the slides — the parser being right about
    /// `1+3` is worth nothing if the presentation still shows all three pages.
    #[test]
    fn a_page_pattern_leaves_the_other_pages_out() {
        let multipage = |pattern: &str| SelectedItemRepresentation {
            source_file: SourceFile {
                name: "MultiPage".to_string(),
                path: PathBuf::from_str("testfiles/MultiPage.pdf").unwrap(),
                file_type: SourceFileType::Pdf,
                md5_hash: None,
                relative_path: None,
            },
            presentation_design_option: None,
            slide_settings_option: None,
            stream_design_option: None,
            stream_slide_settings_option: None,
            inline_markdown: None,
            timer_settings_option: None,
            transition_effect: Default::default(),
            pdf_pages: pattern.to_string(),
            video_settings: Default::default(),
        };

        let pages_of = |pattern: &str| -> Vec<String> {
            create_presentation_slides(&multipage(pattern), &SlideSettings::default(), &[])
                .expect("the PDF is read")
                .iter()
                .filter_map(|slide| match slide.slide_content {
                    SlideContent::SimplePicture(ref picture) => {
                        let path = get_picture_path(picture);
                        path.rsplit_once("#page=")
                            .map(|(_, page)| page.to_string())
                    }
                    _ => None,
                })
                .collect()
        };

        assert_eq!(pages_of("2"), vec!["2"]);
        assert_eq!(pages_of("1+3"), vec!["1", "3"]);
        assert_eq!(pages_of("2-3"), vec!["2", "3"]);

        // Empty is the whole document, and so is a pattern that cannot be
        // read: a half-written one must not make the element vanish.
        assert_eq!(pages_of(""), vec!["1", "2", "3"]);
        assert_eq!(pages_of("2-"), vec!["1", "2", "3"]);
    }

    #[test]
    fn test_presentation_creation_from_image() {
        let select_item = SelectedItemRepresentation::for_test(
            "test_image",
            "testfiles/test.png",
            SourceFileType::Image,
        );
        let result = create_presentation_slides(&select_item, &SlideSettings::default(), &[]);
        assert!(result.is_ok());
        let slides = result.unwrap();
        assert_eq!(slides.len(), 1);
        assert!(matches!(
            slides[0].slide_content,
            SlideContent::SimplePicture(_)
        ));
    }

    #[test]
    fn test_presentation_creation_from_markdown() {
        let select_item = SelectedItemRepresentation::for_test(
            "example",
            "testfiles/example.md",
            SourceFileType::Markdown,
        );
        let result = create_presentation_slides(&select_item, &SlideSettings::default(), &[]);
        assert!(result.is_ok());
        let slides = result.unwrap();
        // example.md has 3 sections separated by ---
        assert_eq!(slides.len(), 3);
        for slide in &slides {
            assert!(matches!(
                slide.slide_content,
                SlideContent::SingleLanguageMainContent(_)
            ));
        }
    }

    #[test]
    fn test_slides_from_markdown() {
        let md = "# Hello\n\nWorld\n\n---\n\n## Slide 2\n\n- a\n- b";
        let slides = slides_from_markdown(md);
        assert_eq!(slides.len(), 2);

        // Check that slides contain the markdown prefix
        if let SlideContent::SingleLanguageMainContent(ref s) = slides[0].slide_content {
            let text = s.clone().main_text();
            assert!(text.starts_with(MARKDOWN_HTML_PREFIX));
            let html = get_markdown_html(&text).unwrap();
            assert!(html.contains("<h1>"));
            assert!(html.contains("Hello"));
        } else {
            panic!("Expected SingleLanguageMainContent");
        }

        if let SlideContent::SingleLanguageMainContent(ref s) = slides[1].slide_content {
            let text = s.clone().main_text();
            let html = get_markdown_html(&text).unwrap();
            assert!(html.contains("<h2>"));
            assert!(html.contains("<li>"));
        } else {
            panic!("Expected SingleLanguageMainContent");
        }
    }

    #[test]
    fn test_slides_from_markdown_empty_sections() {
        let md = "# Only slide\n\n---\n\n---\n\n";
        let slides = slides_from_markdown(md);
        // Empty sections should be skipped
        assert_eq!(slides.len(), 1);
    }

    #[test]
    fn test_get_markdown_html() {
        let with_prefix = format!("{}<h1>Hello</h1>", MARKDOWN_HTML_PREFIX);
        assert_eq!(get_markdown_html(&with_prefix), Some("<h1>Hello</h1>"));

        let without_prefix = "Just plain text";
        assert_eq!(get_markdown_html(without_prefix), None);
    }

    /// A document is read whole: `---` stays a rule instead of cutting the
    /// text in two, and already rendered HTML is not run through the renderer
    /// a second time.
    #[test]
    fn test_markdown_to_html_renders_a_document() {
        let html = markdown_to_html("# Hello\n\n> quoted\n\n---\n\n## World");
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("<hr />"));
        assert!(html.contains("<h2>World</h2>"));

        let already_rendered = format!("{}<h1>Hello</h1>", MARKDOWN_HTML_PREFIX);
        assert_eq!(markdown_to_html(&already_rendered), "<h1>Hello</h1>");
    }

    #[test]
    fn test_slides_from_markdown_windows_line_endings() {
        let md = "# Hello\r\n\r\n---\r\n\r\n## World";
        let slides = slides_from_markdown(md);
        assert_eq!(slides.len(), 2);
    }

    #[test]
    fn test_html_to_plain_text() {
        assert_eq!(
            html_to_plain_text("<h1>Title</h1><p>Hello world</p>"),
            "Title\nHello world"
        );
        assert_eq!(
            html_to_plain_text("<ul><li>one</li><li>two</li></ul>"),
            "one\ntwo"
        );
        assert_eq!(
            html_to_plain_text("<p>a &amp; b &lt; c</p>"),
            "a & b < c"
        );
        // &amp;lt; should decode to &lt; (not <)
        assert_eq!(html_to_plain_text("&amp;lt;"), "&lt;");
        assert_eq!(html_to_plain_text("plain text"), "plain text");
    }

    // -------------------------------------------------------------------------
    // Helpers for update_presentation tests
    // -------------------------------------------------------------------------

    fn inline_md_item(path: &str, markdown: &str) -> SelectedItemRepresentation {
        {
            let mut item = SelectedItemRepresentation::for_test(
                path,
                path,
                SourceFileType::Markdown,
            );
            item.inline_markdown = Some(markdown.to_string());
            item
        }
    }

    /// Build a `RunningPresentation` from inline-markdown items (no disk I/O).
    fn build_rp(items: &[SelectedItemRepresentation]) -> RunningPresentation {
        let design = PresentationDesign::default();
        let settings = SlideSettings::default();
        let rp = build_presentation(
            &items.to_vec(),
            &design,
            &settings,
            &StreamDefaults::default(),
                    &[],
);
        rp.expect("build_presentation should succeed for inline markdown items")
    }

    // -------------------------------------------------------------------------
    // update_presentation tests
    // -------------------------------------------------------------------------

    /// (1) When the same chapters are regenerated the position (chapter index
    /// and slide-within-chapter) is preserved exactly.
    #[test]
    fn test_update_preserves_position_on_regeneration() {
        // Chapter 0: 2 slides, Chapter 1: 3 slides
        let item_a = inline_md_item("a.md", "# S1\n\n---\n\n# S2");
        let item_b = inline_md_item("b.md", "# S1\n\n---\n\n# S2\n\n---\n\n# S3");
        let items = [item_a.clone(), item_b.clone()];

        let mut rp = build_rp(&items);
        // Navigate to chapter 1, slide 1  (slide_total = 2 + 1 = 3)
        rp.jump_to(1, 1);
        assert_eq!(rp.position.as_ref().unwrap().chapter(), 1);
        assert_eq!(rp.position.as_ref().unwrap().chapter_slide(), 1);

        let updated = apply_presentation_update(
            rp,
            &items,
            &PresentationDesign::default(),
            &SlideSettings::default(),
            &StreamDefaults::default(),
                    &[],
);

        let pos = updated.position.expect("position should survive regeneration");
        assert_eq!(pos.chapter(), 1, "chapter index should be preserved");
        assert_eq!(pos.chapter_slide(), 1, "slide-within-chapter should be preserved");
        // slide_total = chapter-0 slides (2) + chapter_slide (1)
        assert_eq!(pos.slide_total(), 3, "slide_total should be recomputed correctly");
    }

    /// (2) When the chapter is regenerated with fewer slides than the current
    /// slide index, the position is clamped to the last available slide.
    #[test]
    fn test_update_clamps_slide_index_when_fewer_slides() {
        // Chapter 0: starts with 3 slides; user is on slide 2
        let item_3slides = inline_md_item("a.md", "# S1\n\n---\n\n# S2\n\n---\n\n# S3");
        let items_initial = [item_3slides];

        let mut rp = build_rp(&items_initial);
        rp.jump_to(0, 2); // last slide
        assert_eq!(rp.position.as_ref().unwrap().chapter_slide(), 2);

        // Regenerate with only 1 slide for the same chapter
        let item_1slide = inline_md_item("a.md", "# Only");
        let items_updated = [item_1slide];

        let updated = apply_presentation_update(
            rp,
            &items_updated,
            &PresentationDesign::default(),
            &SlideSettings::default(),
            &StreamDefaults::default(),
                    &[],
);

        let pos = updated.position.expect("position should still exist");
        assert_eq!(pos.chapter(), 0, "still in chapter 0");
        assert_eq!(pos.chapter_slide(), 0, "clamped to slide 0 (only slide)");
        assert_eq!(pos.slide_total(), 0);
    }

    /// (2b) Two items share the same path but have different inline content.
    /// After the selection is reordered the position must follow the correct
    /// item (the one the user was actually viewing), not slide to the other.
    #[test]
    fn test_update_preserves_position_when_duplicate_paths_reordered() {
        // Both items share path "shared.md" but have different content (1 vs 2 slides).
        let item_one = inline_md_item("shared.md", "# Solo");
        let item_two = inline_md_item("shared.md", "# First\n\n---\n\n# Second");

        let items_initial = [item_one.clone(), item_two.clone()];
        let mut rp = build_rp(&items_initial);
        // Navigate to chapter 1 (item_two), slide 1
        rp.jump_to(1, 1);
        assert_eq!(rp.position.as_ref().unwrap().chapter(), 1);
        assert_eq!(rp.position.as_ref().unwrap().chapter_slide(), 1);

        // Regenerate with the order swapped: [item_two, item_one]
        let items_swapped = [item_two, item_one];
        let updated = apply_presentation_update(
            rp,
            &items_swapped,
            &PresentationDesign::default(),
            &SlideSettings::default(),
            &StreamDefaults::default(),
                    &[],
);

        let pos = updated.position.expect("position should survive reorder");
        // item_two is now chapter 0; user should still be on its slide 1
        assert_eq!(pos.chapter(), 0, "position should follow item_two to its new index");
        assert_eq!(pos.chapter_slide(), 1, "slide within item_two should be preserved");
        assert_eq!(pos.slide_total(), 1, "slide_total = 0 slides before + slide 1");
    }

    /// (3) When the currently active chapter is removed from the selection
    /// the position falls back to the first chapter.
    #[test]
    fn test_update_falls_back_to_first_chapter_when_current_removed() {
        // Two chapters; user is on chapter 1
        let item_a = inline_md_item("a.md", "# SlideA");
        let item_b = inline_md_item("b.md", "# SlideB");
        let items_initial = [item_a.clone(), item_b.clone()];

        let mut rp = build_rp(&items_initial);
        rp.jump_to(1, 0);
        assert_eq!(rp.position.as_ref().unwrap().chapter(), 1);

        // Regenerate with only chapter A (chapter B is gone)
        let items_updated = [item_a];

        let updated = apply_presentation_update(
            rp,
            &items_updated,
            &PresentationDesign::default(),
            &SlideSettings::default(),
            &StreamDefaults::default(),
                    &[],
);

        let pos = updated.position.expect("position should fall back, not be None");
        assert_eq!(pos.chapter(), 0, "should fall back to first chapter");
        assert_eq!(pos.chapter_slide(), 0);
    }
}
