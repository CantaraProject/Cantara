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
use cantara_songlib::song::{Song, SongPart, SongPartType};
use rust_i18n::t;

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

/// The translation key for a kind of song part.
fn part_type_key(part_type: SongPartType) -> &'static str {
    match part_type {
        SongPartType::Verse => "song_part.verse",
        SongPartType::Chorus => "song_part.chorus",
        SongPartType::Bridge => "song_part.bridge",
        SongPartType::Intro => "song_part.intro",
        SongPartType::Outro => "song_part.outro",
        SongPartType::Interlude => "song_part.interlude",
        SongPartType::Instrumental => "song_part.instrumental",
        SongPartType::Solo => "song_part.solo",
        SongPartType::PreChorus => "song_part.prechorus",
        SongPartType::PostChorus => "song_part.postchorus",
        SongPartType::Refrain => "song_part.refrain",
        SongPartType::Other => "song_part.other",
    }
}

/// How a part is headed on screen: `"Strophe 1"` rather than `"verse.1"`.
///
/// The number is only shown when the song has more than one part of that kind —
/// a song with a single chorus reads better as "Refrain" than "Refrain 1".
///
/// A part Cantara has no word for keeps the heading its file gave it. That is
/// what the song library preserves the original wording for: songs downloaded
/// in a language the importer has no vocabulary for would otherwise lose their
/// structure entirely.
pub fn part_label(song: &Song, part: &SongPart) -> String {
    if part.part_type == SongPartType::Other {
        return part
            .label
            .clone()
            .filter(|label| !label.trim().is_empty())
            .unwrap_or_else(|| part.id().to_string());
    }

    let name = t!(part_type_key(part.part_type)).to_string();

    let siblings = song
        .parts()
        .iter()
        .filter(|other| other.part_type == part.part_type)
        .count();

    if siblings > 1 {
        format!("{name} {}", part.number)
    } else {
        name
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

#[cfg(test)]
mod part_label_tests {
    use super::*;
    use crate::logic::export::song_from_content;

    fn reference_song() -> Song {
        let content =
            std::fs::read_to_string("testfiles/Sei nicht stolz auf das, was du bist.song.yml")
                .unwrap();
        song_from_content("Sei nicht stolz auf das, was du bist.song.yml", &content).unwrap()
    }

    /// The point of the exercise: a part must never be headed with its
    /// identifier. "verse.1" is a key, not something to show a musician.
    #[test]
    fn test_no_identifier_reaches_the_screen() {
        let song = reference_song();

        for part in song.parts() {
            let label = part_label(&song, part);
            assert!(
                !label.contains('.'),
                "the identifier leaked through as {label:?}"
            );
            assert!(!label.is_empty());
        }
    }

    /// A song with one chorus reads better as "Refrain" than "Refrain 1"; a
    /// song with several stanzas needs the number to tell them apart.
    ///
    /// Deliberately says nothing about the wording: `set_locale` is global and
    /// the test threads share it, so an assertion on German text here could be
    /// broken by whichever test flips the locale next.
    #[test]
    fn test_the_number_appears_only_when_it_distinguishes() {
        let content = "\
version: 0.1
title: Test
parts:
  - type: stanza
    contents:
      - type: lyrics
        number: 1
        content: eins
  - type: stanza
    contents:
      - type: lyrics
        number: 1
        content: zwei
  - type: refrain
    contents:
      - type: lyrics
        number: 1
        content: kehrvers
";
        let song = song_from_content("Test.song.yml", content).unwrap();

        let label_of = |part_type, number| {
            let part = song
                .parts()
                .iter()
                .find(|part| part.part_type == part_type && part.number == number)
                .expect("part");
            part_label(&song, part)
        };

        let first = label_of(SongPartType::Verse, 1);
        let second = label_of(SongPartType::Verse, 2);
        let chorus = label_of(SongPartType::Refrain, 1);

        assert_ne!(first, second, "two stanzas must be told apart");
        assert!(first.ends_with('1'), "got {first:?}");
        assert!(second.ends_with('2'), "got {second:?}");
        assert!(
            !chorus.ends_with(char::is_numeric),
            "the only chorus should carry no number: {chorus:?}"
        );
    }

    /// The label follows the interface language.
    #[test]
    fn test_the_label_is_localised() {
        let song = reference_song();
        let part = song
            .parts()
            .iter()
            .find(|part| part.part_type == SongPartType::Verse)
            .expect("the reference song has stanzas");

        rust_i18n::set_locale("de");
        let german = part_label(&song, part);
        rust_i18n::set_locale("en");
        let english = part_label(&song, part);

        assert!(german.starts_with("Strophe"), "got {german:?}");
        assert!(english.starts_with("Verse"), "got {english:?}");
    }

    /// A part Cantara has no word for keeps the heading its file gave it —
    /// otherwise a song in an unfamiliar language loses its structure.
    #[test]
    fn test_an_unknown_part_keeps_its_own_heading() {
        let mut song = Song::new("Test");
        let mut part = SongPart::new(cantara_songlib::song::SongPartId::new(
            SongPartType::Other,
            1,
        ));
        part.label = Some("주 후렴".to_string());
        song.add_part(part).expect("the part must be accepted");

        let part = &song.parts()[0];
        assert_eq!(part_label(&song, part), "주 후렴");
    }
}
