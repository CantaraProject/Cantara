//! What the detail view can show, and how, for each kind of element.
//!
//! The selection view answers "which elements go into the presentation"; the
//! detail view answers "what is *in* this element" — and, where it makes sense,
//! lets the user change it.
//!
//! # Adding a kind of element
//!
//! Every element Cantara can open is a [`DetailSubject`] variant. Adding one is
//! deliberately a compiler-guided exercise: the variant makes every `match` in
//! this module and in `detail_components.rs` incomplete, so the tabs it offers,
//! whether it can be edited, and how it is drawn all have to be decided rather
//! than silently defaulting. That is the same reason the export formats are an
//! enum — a wildcard arm once swallowed a whole slide type in the presenter
//! console and showed "…" instead of the lyrics.

use crate::logic::sourcefiles::{SourceFile, SourceFileType};

/// One element opened in the detail view.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DetailSubject {
    /// A song, in any of the formats the song library reads.
    Song(SourceFile),
    /// A picture.
    Image(SourceFile),
    /// A PDF, shown page by page.
    Pdf(SourceFile),
    /// A markdown document.
    Markdown(SourceFile),
}

/// Whether the detail view is showing an element or editing it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DetailMode {
    #[default]
    View,
    Edit,
}

/// One way of looking at an element.
///
/// A song is worth reading two ways — as words and as music — so the detail
/// view is organised in tabs rather than one long page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetailTab {
    /// The lyrics, with every language and the parts in the order they are sung.
    Text,
    /// The melody, engraved so it can be played from the screen.
    Notation,
    /// The element as the audience would see it.
    Preview,
}

impl DetailTab {
    /// The translation key for this tab's label.
    pub fn label_key(self) -> &'static str {
        match self {
            DetailTab::Text => "detail.tab_text",
            DetailTab::Notation => "detail.tab_notation",
            DetailTab::Preview => "detail.tab_preview",
        }
    }

    /// A stable identifier, for the tab buttons.
    pub fn id(self) -> &'static str {
        match self {
            DetailTab::Text => "text",
            DetailTab::Notation => "notation",
            DetailTab::Preview => "preview",
        }
    }
}

impl DetailSubject {
    /// The subject for a source file, if the detail view can show it.
    pub fn of(file: &SourceFile) -> Option<DetailSubject> {
        match file.file_type {
            SourceFileType::Song => Some(DetailSubject::Song(file.clone())),
            SourceFileType::Image => Some(DetailSubject::Image(file.clone())),
            SourceFileType::Pdf => Some(DetailSubject::Pdf(file.clone())),
            SourceFileType::Markdown => Some(DetailSubject::Markdown(file.clone())),
            // Cantara knows these turn up in a repository but has no viewer for
            // them; the selection view does not offer them either.
            SourceFileType::Presentation | SourceFileType::Video => None,
        }
    }

    /// The file this subject was opened from.
    pub fn source_file(&self) -> &SourceFile {
        match self {
            DetailSubject::Song(file)
            | DetailSubject::Image(file)
            | DetailSubject::Pdf(file)
            | DetailSubject::Markdown(file) => file,
        }
    }

    /// The tabs this element is worth looking at through.
    ///
    /// One tab means the view shows it plainly, without a tab bar.
    pub fn tabs(&self) -> &'static [DetailTab] {
        match self {
            DetailSubject::Song(_) => &[DetailTab::Text, DetailTab::Notation],
            DetailSubject::Image(_) | DetailSubject::Pdf(_) => &[DetailTab::Preview],
            DetailSubject::Markdown(_) => &[DetailTab::Preview],
        }
    }

    /// Whether this element can be changed from the detail view.
    ///
    /// A picture and a PDF are opaque to Cantara — it can show them but has no
    /// business rewriting them.
    pub fn is_editable(&self) -> bool {
        match self {
            DetailSubject::Song(_) | DetailSubject::Markdown(_) => true,
            DetailSubject::Image(_) | DetailSubject::Pdf(_) => false,
        }
    }

    /// The name shown above the element.
    pub fn title(&self) -> String {
        self.source_file().name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(name: &str, file_type: SourceFileType) -> SourceFile {
        SourceFile {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{name}")),
            file_type,
            md5_hash: None,
        }
    }

    /// Every kind the selection view offers has to be openable, otherwise a
    /// user can pick something the detail view then refuses to show. The two
    /// types Cantara only recognises by name are deliberately not among them.
    #[test]
    fn test_every_source_type_can_be_opened() {
        for file_type in [
            SourceFileType::Song,
            SourceFileType::Image,
            SourceFileType::Pdf,
            SourceFileType::Markdown,
        ] {
            let subject = DetailSubject::of(&file("x", file_type));
            assert!(subject.is_some(), "{file_type:?} cannot be opened");
        }

        for file_type in [SourceFileType::Presentation, SourceFileType::Video] {
            assert!(
                DetailSubject::of(&file("x", file_type)).is_none(),
                "{file_type:?} has no viewer and must not claim to have one"
            );
        }
    }

    /// A song is worth reading as words and as music.
    #[test]
    fn test_a_song_offers_text_and_notation() {
        let subject = DetailSubject::of(&file("a.song", SourceFileType::Song)).unwrap();

        assert_eq!(subject.tabs(), &[DetailTab::Text, DetailTab::Notation]);
    }

    /// Cantara can show a picture or a PDF but has no business rewriting them.
    #[test]
    fn test_only_the_text_formats_are_editable() {
        let editable = |file_type| {
            DetailSubject::of(&file("x", file_type))
                .unwrap()
                .is_editable()
        };

        assert!(editable(SourceFileType::Song));
        assert!(editable(SourceFileType::Markdown));
        assert!(!editable(SourceFileType::Image));
        assert!(!editable(SourceFileType::Pdf));
    }

    /// Every subject has to keep the file it came from — the viewers read it.
    #[test]
    fn test_the_source_file_survives() {
        let original = file("Amazing Grace.song", SourceFileType::Song);
        let subject = DetailSubject::of(&original).unwrap();

        assert_eq!(subject.source_file(), &original);
        assert_eq!(subject.title(), "Amazing Grace.song");
    }

    /// Tab identifiers end up in the DOM and in comparisons, so they must be
    /// distinct.
    #[test]
    fn test_tab_ids_are_distinct() {
        let ids = [DetailTab::Text, DetailTab::Notation, DetailTab::Preview]
            .map(|tab| tab.id());
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        assert_eq!(sorted.len(), ids.len());
    }
}
