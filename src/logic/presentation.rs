//! This module contains functions for creating presentations

use super::{
    settings::PresentationDesign,
    sourcefiles::{SourceFile, SourceFileType},
    states::{RunningPresentation, SelectedItemRepresentation, SlideChapter},
};

use cantara_songlib::importer::classic_song::slides_from_classic_song;
use cantara_songlib::slides::{Slide, SlideSettings};
use dioxus::prelude::*;
use std::{error::Error, path::PathBuf};
use crate::logic::settings::PresentationDesignSettings;

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

/// Creates a presentation from a selected_item_representation and a presentation_design
fn create_presentation_slides(
    selected_item: &SelectedItemRepresentation,
    default_song_slide_settings: &SlideSettings,
) -> Result<Vec<Slide>, Box<dyn Error>> {
    let mut presentation: Vec<Slide> = vec![];

    if selected_item.source_file.file_type == SourceFileType::Song {

        let slide_settings = selected_item
            .slide_settings_option
            .clone()
            .unwrap_or(default_song_slide_settings.clone());

        match cantara_songlib::create_presentation_from_file(
            selected_item.source_file.path.clone(),
            slide_settings,
        ) {
            Ok(slides) => presentation.extend(slides),
            Err(err) => return Err(err),
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
    slide_settings: &SlideSettings
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
        Some(slide_settings.clone())
    );

    RunningPresentation::new(vec![slide_chapter])
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
            slide_settings_option: None
        };
        assert!(create_presentation_slides(&select_item, &SlideSettings::default()).is_ok());
    }
}
