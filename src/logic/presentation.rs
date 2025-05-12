//! This module contains functions for creating presentations

use super::{
    settings::PresentationDesign,
    sourcefiles::SourceFileType,
    states::{RunningPresentation, SelectedItemRepresentation, SlideChapter},
};

use cantara_songlib::slides::Slide;
use dioxus::prelude::*;
use std::error::Error;

/// Creates a presentation from a selected_item_representation and a presentation_design
fn create_presentation_slides(
    selected_item: &SelectedItemRepresentation,
    default_presentation_design: &PresentationDesign,
) -> Result<Vec<Slide>, Box<dyn Error>> {
    let mut presentation: Vec<Slide> = vec![];

    if selected_item.source_file.file_type == SourceFileType::Song {
        let presentation_design = selected_item
            .presentation_design_option
            .clone()
            .unwrap_or(default_presentation_design.clone());

        match cantara_songlib::create_presentation_from_file(
            selected_item.source_file.path.clone(),
            presentation_design.slide_settings.clone(),
        ) {
            Ok(slides) => presentation.extend(slides),
            Err(err) => return Err(err),
        }
    }

    Ok(presentation)
}

/// Adds a presentation to the global running presentations signal
/// Returns the number (id) of the created presentation
pub fn add_presentation(selected_items: &Vec<SelectedItemRepresentation>) -> Option<usize> {
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    // Right now, we only allow one running presentation at the same time.
    // Later, Cantara is going to support multiple presentations.
    if running_presentations.len() > 0 {
        running_presentations.clear();
    }

    let mut presentation: Vec<SlideChapter> = vec![];

    for selected_item in selected_items {
        match create_presentation_slides(&selected_item, &PresentationDesign::default()) {
            Ok(slides) => presentation.push(SlideChapter {
                slides,
                source_file: selected_item.source_file.clone(),
                presentation_design: selected_item.presentation_design_option.clone(),
            }),
            Err(_) => {}
        }
    }

    if !presentation.is_empty() {
        running_presentations.push(RunningPresentation::new(presentation));
        return Some(running_presentations.len() - 1);
    }

    None
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use crate::logic::{
        settings::PresentationDesign,
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
        };
        assert!(create_presentation_slides(&select_item, &PresentationDesign::default()).is_ok());
    }
}
