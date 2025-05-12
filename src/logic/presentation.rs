//! This module contains functions for creating presentations

use super::{
    settings::PresentationDesign, sourcefiles::SourceFileType, states::SelectedItemRepresentation,
};

use cantara_songlib::slides::Slide;
use std::error::Error;

/// Creates a presentation from a selected_item_representation and a presentation_design
pub fn create_presentation(
    selected_item: &SelectedItemRepresentation,
    presentation_design: &PresentationDesign,
) -> Result<Vec<Slide>, Box<dyn Error>> {
    let mut presentation: Vec<Slide> = vec![];

    if selected_item.source_file.file_type == SourceFileType::Song {
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
        assert!(create_presentation(&select_item, &PresentationDesign::default()).is_ok());
    }
}
