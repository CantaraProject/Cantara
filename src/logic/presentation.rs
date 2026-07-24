//! This module contains functions for creating presentations

use super::{
    settings::PresentationDesign,
    sourcefiles::{SourceFile, SourceFileType},
    states::{RunningPresentation, RunningPresentationPosition, SelectedItemRepresentation, SlideChapter},
};

use cantara_songlib::importer::classic_song::slides_from_classic_song;
use cantara_songlib::slides::{Slide, SlideContent, SimplePictureSlide, SingleLanguageMainContentSlide, SlideSettings};
use dioxus::prelude::*;
use std::{error::Error, path::{Path, PathBuf}};
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
            if block_tags.iter().any(|t| lower == *t) {
                if !result.is_empty() && !result.ends_with('\n') {
                    result.push('\n');
                }
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
fn create_presentation_slides(
    selected_item: &SelectedItemRepresentation,
    default_song_slide_settings: &SlideSettings,
) -> Result<Vec<Slide>, Box<dyn Error>> {
    let mut presentation: Vec<Slide> = vec![];

    let slide_settings = selected_item
        .slide_settings_option
        .clone()
        .unwrap_or(default_song_slide_settings.clone());

    if selected_item.source_file.file_type == SourceFileType::Song {
        #[cfg(target_arch = "wasm32")]
        {
            // On web, read song content from the in-memory VFS
            let path_str = selected_item.source_file.path.to_str().unwrap_or("");
            if let Some(content_bytes) = crate::logic::settings::RepositoryType::web_read_file(path_str) {
                let content = String::from_utf8_lossy(&content_bytes);
                let slides = slides_from_classic_song(
                    &content,
                    &slide_settings,
                    selected_item.source_file.name.clone(),
                );
                presentation.extend(slides);
            }
            return Ok(presentation);
        }

        #[cfg(not(target_arch = "wasm32"))]
        match cantara_songlib::create_presentation_from_file(
            selected_item.source_file.path.clone(),
            slide_settings,
        ) {
            Ok(slides) => presentation.extend(slides),
            Err(err) => return Err(err),
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

    if selected_item.source_file.file_type == SourceFileType::Pdf {
        let path_str = selected_item
            .source_file
            .path
            .to_str()
            .unwrap_or("")
            .to_string();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let page_count = get_pdf_page_count(&selected_item.source_file.path)?;
            for page in 1..=page_count {
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
                for page in 1..=page_count {
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
) -> Option<RunningPresentation> {
    let mut presentation: Vec<SlideChapter> = vec![];

    for selected_item in selected_items {
        let used_presentation_design = selected_item
            .presentation_design_option
            .clone()
            .unwrap_or(default_presentation_design.clone());

        let used_slide_settings = selected_item
            .slide_settings_option
            .clone()
            .unwrap_or(default_slide_settings.clone());

        match create_presentation_slides(selected_item, &used_slide_settings) {
            Ok(slides) => presentation.push(SlideChapter {
                id: Uuid::new_v4(),
                slides,
                source_file: selected_item.source_file.clone(),
                presentation_design_option: Some(used_presentation_design),
                slide_settings_option: Some(used_slide_settings),
                timer_settings_option: selected_item.timer_settings_option.clone(),
                transition_option: selected_item.transition_effect,
                inline_markdown: selected_item.inline_markdown.clone(),
            }),
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

pub fn add_presentation(
    selected_items: &Vec<SelectedItemRepresentation>,
    running_presentations: &mut Signal<Vec<RunningPresentation>>,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
) -> Option<usize> {
    // Right now, we only allow one running presentation at the same time.
    // Later, Cantara is going to support multiple presentations.
    if running_presentations.len() > 0 {
        running_presentations.write().clear();
    }

    if let Some(rp) = build_presentation(selected_items, default_presentation_design, default_slide_settings) {
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
) -> RunningPresentation {
    let used_presentation_design = selected_item
        .presentation_design_option
        .clone()
        .unwrap_or(default_presentation_design.clone());

    let used_slide_settings = selected_item
        .slide_settings_option
        .clone()
        .unwrap_or(default_slide_settings.clone());

    let slides = create_presentation_slides(selected_item, &used_slide_settings)
        .unwrap_or_default();

    let chapter = SlideChapter {
        id: Uuid::new_v4(),
        slides,
        source_file: selected_item.source_file.clone(),
        presentation_design_option: Some(used_presentation_design),
        slide_settings_option: Some(used_slide_settings),
        timer_settings_option: selected_item.timer_settings_option.clone(),
        transition_option: selected_item.transition_effect,
        inline_markdown: selected_item.inline_markdown.clone(),
    };

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
        let used_presentation_design = selected_item
            .presentation_design_option
            .clone()
            .unwrap_or(default_presentation_design.clone());

        let used_slide_settings = selected_item
            .slide_settings_option
            .clone()
            .unwrap_or(default_slide_settings.clone());

        match create_presentation_slides(selected_item, &used_slide_settings) {
            Ok(slides) => {
                // Carry the old UUID for this content fingerprint (FIFO within
                // identical fingerprints) so we can restore the viewing position.
                let key: ChapterKey = (
                    selected_item.source_file.path.clone(),
                    selected_item.source_file.md5_hash.clone(),
                    selected_item.inline_markdown.clone(),
                );
                let carried_id = old_key_ids.get_mut(&key).and_then(|q| q.pop_front());

                new_chapters.push(SlideChapter {
                    // Temporarily use the carried old UUID so we can find this
                    // chapter in the position-restore step below. A fresh UUID is
                    // assigned after that step completes.
                    id: carried_id.unwrap_or_else(Uuid::new_v4),
                    slides,
                    source_file: selected_item.source_file.clone(),
                    presentation_design_option: Some(used_presentation_design),
                    slide_settings_option: Some(used_slide_settings),
                    timer_settings_option: selected_item.timer_settings_option.clone(),
                    transition_option: selected_item.transition_effect,
                    inline_markdown: selected_item.inline_markdown.clone(),
                });
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
                // Recompute slide_total
                let mut total: usize = 0;
                for i in 0..new_ch_idx {
                    total += new_chapters[i].slides.len();
                }
                total += clamped_slide;
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
    }
}

pub fn update_presentation(
    selected_items: &[SelectedItemRepresentation],
    running_presentations: &mut Signal<Vec<RunningPresentation>>,
    default_presentation_design: &PresentationDesign,
    default_slide_settings: &SlideSettings,
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
        use std::collections::HashMap;

        if let Some(rp) = running_presentations.peek().first() {
            if let Ok(json) = serde_json::to_string(rp) {
                // Collect VFS files (e.g. PDFs) so the synced tab can render them
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

                let _ = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .map(|s| {
                        let _ = s.set_item(SYNC_KEY_POSITION_FROM_CONSOLE, &json);
                        let _ = s.remove_item(SYNC_KEY_POSITION);
                        // Sync VFS files if there are any PDFs
                        if !files.is_empty() {
                            if let Ok(files_json) = serde_json::to_string(&files) {
                                let _ = s.set_item(SYNC_KEY_FILES, &files_json);
                            }
                        }
                    });
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

    #[test]
    fn test_presentation_creation_from_amazing_grace() {
        let select_item = SelectedItemRepresentation {
            source_file: SourceFile {
                name: "Amazing Grace".to_string(),
                path: PathBuf::from_str("testfiles/Amazing Grace.song").unwrap(),
                file_type: SourceFileType::Song,
                md5_hash: None,
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
            timer_settings_option: None,
            transition_effect: Default::default(),
        };
        assert!(create_presentation_slides(&select_item, &SlideSettings::default()).is_ok());
    }

    #[test]
    fn test_presentation_creation_from_pdf() {
        let select_item = SelectedItemRepresentation {
            source_file: SourceFile {
                name: "Example".to_string(),
                path: PathBuf::from_str("testfiles/Example.pdf").unwrap(),
                file_type: SourceFileType::Pdf,
                md5_hash: None,
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
            timer_settings_option: None,
            transition_effect: Default::default(),
        };
        let result = create_presentation_slides(&select_item, &SlideSettings::default());
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
        let select_item = SelectedItemRepresentation {
            source_file: SourceFile {
                name: "MultiPage".to_string(),
                path: PathBuf::from_str("testfiles/MultiPage.pdf").unwrap(),
                file_type: SourceFileType::Pdf,
                md5_hash: None,
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
            timer_settings_option: None,
            transition_effect: Default::default(),
        };
        let result = create_presentation_slides(&select_item, &SlideSettings::default());
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

    #[test]
    fn test_presentation_creation_from_image() {
        let select_item = SelectedItemRepresentation {
            source_file: SourceFile {
                name: "test_image".to_string(),
                path: PathBuf::from_str("testfiles/test.png").unwrap(),
                file_type: SourceFileType::Image,
                md5_hash: None,
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
            timer_settings_option: None,
            transition_effect: Default::default(),
        };
        let result = create_presentation_slides(&select_item, &SlideSettings::default());
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
        let select_item = SelectedItemRepresentation {
            source_file: SourceFile {
                name: "example".to_string(),
                path: PathBuf::from_str("testfiles/example.md").unwrap(),
                file_type: SourceFileType::Markdown,
                md5_hash: None,
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
            timer_settings_option: None,
            transition_effect: Default::default(),
        };
        let result = create_presentation_slides(&select_item, &SlideSettings::default());
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
        SelectedItemRepresentation {
            source_file: SourceFile {
                name: path.to_string(),
                path: PathBuf::from_str(path).unwrap(),
                file_type: crate::logic::sourcefiles::SourceFileType::Markdown,
                md5_hash: None,
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: Some(markdown.to_string()),
            timer_settings_option: None,
            transition_effect: Default::default(),
        }
    }

    /// Build a `RunningPresentation` from inline-markdown items (no disk I/O).
    fn build_rp(items: &[SelectedItemRepresentation]) -> RunningPresentation {
        let design = PresentationDesign::default();
        let settings = SlideSettings::default();
        let rp = build_presentation(&items.to_vec(), &design, &settings);
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
        );

        let pos = updated.position.expect("position should fall back, not be None");
        assert_eq!(pos.chapter(), 0, "should fall back to first chapter");
        assert_eq!(pos.chapter_slide(), 0);
    }
}
