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
        assert_eq!(subject.source_file().name, "Amazing Grace.song");
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

/// How an order is named on screen.
///
/// The song's own order has no name of its own, so it is called what it is.
pub fn order_label(order: &cantara_songlib::song::PartOrder) -> String {
    match &order.name {
        cantara_songlib::song::PartOrderName::Default => t!("detail.order_default").to_string(),
        cantara_songlib::song::PartOrderName::Custom(name) => name.clone(),
    }
}

/// Structural changes to a song: removing, moving, adding a language, and the
/// alternative orders.
///
/// These live here rather than in the view because they are where a song can
/// quietly lose something. Removing a part, in particular, has to take that
/// part out of every order that names it — an order pointing at a part that no
/// longer exists would drop silently out of the sung sequence.
pub mod editing {
    use cantara_songlib::song::{
        LyricLanguage, PartOrder, PartOrderName, PartOrderRule, Song, SongPartContent, SongPartId,
    };

    /// Removes a part and every reference to it.
    ///
    /// The song library has no `remove_part`, so the song is rebuilt from the
    /// parts that stay. Everything else about it is carried over.
    pub fn remove_part(song: &Song, id: &SongPartId) -> Song {
        let mut rebuilt = Song::new(&song.title);
        rebuilt.default_language = song.default_language.clone();
        rebuilt.score = song.score.clone();

        for (key, value) in song.tags() {
            rebuilt.set_tag(key, value);
        }

        for part in song.parts() {
            if &part.id() != id {
                let _ = rebuilt.add_part(part.clone());
            }
        }

        // An explicit order naming the removed part would otherwise point into
        // nothing.
        rebuilt.part_orders = song
            .part_orders
            .iter()
            .map(|order| match order.rule() {
                PartOrderRule::Custom(ids) => PartOrder::new(
                    order.name.clone(),
                    PartOrderRule::Custom(
                        ids.iter().filter(|other| *other != id).cloned().collect(),
                    ),
                ),
                other => PartOrder::new(order.name.clone(), other.clone()),
            })
            .collect();

        rebuilt
    }

    /// Moves a part one place towards the front or the back.
    ///
    /// Reorders the *stored* parts. A rule-based order derives the sung
    /// sequence from them, so this changes what is sung; an explicit order
    /// names its parts and is left alone.
    pub fn move_part(song: &Song, id: &SongPartId, towards_end: bool) -> Song {
        let mut moved = song.clone();
        let parts = moved.parts_mut();

        let Some(index) = parts.iter().position(|part| &part.id() == id) else {
            return moved;
        };

        let target = if towards_end {
            index + 1
        } else {
            match index.checked_sub(1) {
                Some(target) => target,
                None => return moved,
            }
        };

        if target >= parts.len() {
            return moved;
        }

        parts.swap(index, target);
        moved
    }

    /// Adds an empty lyrics block in `language` to a part.
    ///
    /// Does nothing when the part already has that language: a second block
    /// for the same language would make it ambiguous which one is sung.
    pub fn add_language(song: &Song, id: &SongPartId, language: &str) -> Song {
        let code = language.trim().to_string();
        if code.is_empty() {
            return song.clone();
        }

        let mut updated = song.clone();
        let wanted = LyricLanguage::Specific(code);

        if let Some(part) = updated.part_mut(id) {
            let exists = part
                .all_lyrics()
                .any(|(existing, _)| existing == &wanted);
            if !exists {
                part.add_content(SongPartContent::lyrics(wanted, ""));
            }
        }

        updated
    }

    /// Adds an alternative order under `name`.
    ///
    /// A name is what tells two orders apart, so an empty one, or one already
    /// taken, is refused rather than silently shadowing the existing order.
    pub fn add_order(song: &Song, name: &str, rule: PartOrderRule) -> Result<Song, String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("an order needs a name".to_string());
        }

        let taken = song.part_orders.iter().any(|order| {
            matches!(&order.name, PartOrderName::Custom(existing) if existing == trimmed)
        });
        if taken {
            return Err(format!("there is already an order called {trimmed:?}"));
        }

        let mut updated = song.clone();
        updated.part_orders.push(PartOrder::new(
            PartOrderName::Custom(trimmed.to_string()),
            rule,
        ));
        Ok(updated)
    }

    /// Removes an alternative order.
    ///
    /// The first order is the song's default and stays: a song without any
    /// order has no sung sequence at all.
    pub fn remove_order(song: &Song, index: usize) -> Song {
        let mut updated = song.clone();
        if index > 0 && index < updated.part_orders.len() {
            updated.part_orders.remove(index);
        }
        updated
    }
}

#[cfg(test)]
mod editing_tests {
    use super::editing::*;
    use crate::logic::export::song_from_content;
    use cantara_songlib::song::{PartOrderRule, Song, SongPartType};

    fn song_with_three_parts() -> Song {
        let content = "\
version: 0.1
title: Test
parts:
  - type: stanza
    contents:
      - type: lyrics
        number: 1
        content: eins
  - type: refrain
    contents:
      - type: lyrics
        number: 1
        content: kehrvers
  - type: stanza
    contents:
      - type: lyrics
        number: 2
        content: zwei
";
        song_from_content("Test.song.yml", content).unwrap()
    }

    #[test]
    fn test_removing_a_part_takes_it_out() {
        let song = song_with_three_parts();
        let victim = song.parts()[1].id();

        let reduced = remove_part(&song, &victim);

        assert_eq!(reduced.parts().len(), 2);
        assert!(reduced.parts().iter().all(|part| part.id() != victim));
    }

    /// Everything the song is apart from that part has to survive.
    #[test]
    fn test_removing_a_part_keeps_the_rest_of_the_song() {
        let mut song = song_with_three_parts();
        song.set_tag("author", "Jemand");
        song.default_language = Some("de".to_string());
        let victim = song.parts()[1].id();

        let reduced = remove_part(&song, &victim);

        assert_eq!(reduced.title, song.title);
        assert_eq!(reduced.default_language, song.default_language);
        assert_eq!(
            reduced.tags().get("author").map(String::as_str),
            Some("Jemand")
        );
    }

    /// An explicit order naming the removed part would point into nothing, and
    /// the song library would drop it from the sequence without a word.
    #[test]
    fn test_removing_a_part_prunes_it_from_the_orders() {
        use cantara_songlib::song::{PartOrder, PartOrderName};

        let mut song = song_with_three_parts();
        let ids: Vec<_> = song.parts().iter().map(|part| part.id()).collect();
        song.part_orders.push(PartOrder::new(
            PartOrderName::Custom("kurz".to_string()),
            PartOrderRule::Custom(ids.clone()),
        ));

        let reduced = remove_part(&song, &ids[1]);

        for order in &reduced.part_orders {
            if let PartOrderRule::Custom(remaining) = order.rule() {
                assert!(
                    !remaining.contains(&ids[1]),
                    "the order still names the removed part"
                );
            }
        }
    }

    #[test]
    fn test_moving_a_part_swaps_it_with_its_neighbour() {
        let song = song_with_three_parts();
        let first = song.parts()[0].id();
        let second = song.parts()[1].id();

        let moved = move_part(&song, &first, true);

        assert_eq!(moved.parts()[0].id(), second);
        assert_eq!(moved.parts()[1].id(), first);
    }

    /// Moving past either end must leave the song exactly as it was.
    #[test]
    fn test_moving_past_the_ends_changes_nothing() {
        let song = song_with_three_parts();
        let first = song.parts()[0].id();
        let last = song.parts()[song.parts().len() - 1].id();

        let up = move_part(&song, &first, false);
        let down = move_part(&song, &last, true);

        let ids = |s: &Song| s.parts().iter().map(|p| p.id()).collect::<Vec<_>>();
        assert_eq!(ids(&up), ids(&song));
        assert_eq!(ids(&down), ids(&song));
    }

    #[test]
    fn test_adding_a_language_gives_the_part_an_empty_block() {
        let song = song_with_three_parts();
        let id = song.parts()[0].id();

        let extended = add_language(&song, &id, "en");

        let part = extended.part(&id).unwrap();
        assert!(part.lyrics_for(Some("en"), None).is_some());
    }

    /// Two blocks for one language would make it ambiguous which is sung.
    #[test]
    fn test_a_language_is_not_added_twice() {
        let song = song_with_three_parts();
        let id = song.parts()[0].id();

        let once = add_language(&song, &id, "en");
        let twice = add_language(&once, &id, "en");

        let count = |s: &Song| s.part(&id).unwrap().all_lyrics().count();
        assert_eq!(count(&once), count(&twice));
    }

    #[test]
    fn test_an_empty_language_code_is_ignored() {
        let song = song_with_three_parts();
        let id = song.parts()[0].id();

        let unchanged = add_language(&song, &id, "   ");

        assert_eq!(
            unchanged.part(&id).unwrap().all_lyrics().count(),
            song.part(&id).unwrap().all_lyrics().count()
        );
    }

    #[test]
    fn test_adding_an_alternative_order() {
        let song = song_with_three_parts();
        let before = song.part_orders.len();

        let extended = add_order(&song, "kurz", PartOrderRule::VerseRefrainBridgeRefrain).unwrap();

        assert_eq!(extended.part_orders.len(), before + 1);
    }

    /// A name is what tells two orders apart.
    #[test]
    fn test_an_order_name_must_be_free_and_not_empty() {
        let song = song_with_three_parts();
        let once = add_order(&song, "kurz", PartOrderRule::VerseRefrainBridgeRefrain).unwrap();

        assert!(add_order(&once, "kurz", PartOrderRule::VerseRefrainBridgeRefrain).is_err());
        assert!(add_order(&once, "  ", PartOrderRule::VerseRefrainBridgeRefrain).is_err());
    }

    /// The first order is the song's default; without it there is no sung
    /// sequence at all.
    #[test]
    fn test_the_default_order_cannot_be_removed() {
        let song = song_with_three_parts();
        let with_alternative =
            add_order(&song, "kurz", PartOrderRule::VerseRefrainBridgeRefrain).unwrap();
        let before = with_alternative.part_orders.len();

        assert_eq!(remove_order(&with_alternative, 0).part_orders.len(), before);
        assert_eq!(
            remove_order(&with_alternative, before - 1).part_orders.len(),
            before - 1
        );
    }

    /// Whatever is changed, the song still has to be writable — that is what
    /// saving does immediately afterwards.
    #[test]
    fn test_the_result_still_exports() {
        use cantara_songlib::exporter::song_yml::song_yml_from_song;

        let song = song_with_three_parts();
        let id = song.parts()[1].id();

        for candidate in [
            remove_part(&song, &id),
            move_part(&song, &id, true),
            add_language(&song, &id, "en"),
            add_order(&song, "kurz", PartOrderRule::RefrainVerseBridgeRefrain).unwrap(),
        ] {
            let yml = song_yml_from_song(&candidate).expect("export");
            let reloaded = song_from_content("x.song.yml", &yml).expect("reimport");
            assert_eq!(reloaded.title, candidate.title);
            let _ = SongPartType::Verse;
        }
    }
}
