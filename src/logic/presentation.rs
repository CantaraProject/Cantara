//! This module contains functions for creating presentations

use super::{
    settings::PresentationDesign,
    sourcefiles::{SourceFile, SourceFileType},
    states::{RunningPresentation, SelectedItemRepresentation, SlideChapter},
};

use cantara_songlib::importer::classic_song::slides_from_classic_song;
use cantara_songlib::slides::{Slide, SlideContent, SimplePictureSlide, SingleLanguageMainContentSlide, SlideSettings};
use dioxus::prelude::*;
use std::{error::Error, path::{Path, PathBuf}};

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
                slides,
                source_file: selected_item.source_file.clone(),
                presentation_design_option: Some(used_presentation_design),
                slide_settings_option: Some(used_slide_settings),
            }),
            Err(_) => {
                // TODO: Implement error handling, the user should get a message if an error occurs...
            }
        }
    }

    if !presentation.is_empty() {
        running_presentations
            .write()
            .push(RunningPresentation::new(presentation));
        return Some(running_presentations.len() - 1);
    }

    None
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
    };
    let slide_chapter = SlideChapter::new(
        slides,
        source_file,
        Some(presentation_design.clone()),
        Some(slide_settings.clone()),
    );

    RunningPresentation::new(vec![slide_chapter])
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
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
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
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
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
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
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
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
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
            },
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
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
}
