//! Making a song out of text somebody pasted in.
//!
//! Someone finds a hymn on hymnary.org, selects it, copies it, and wants it in
//! their library. What arrives is text with no structure a program can see —
//! just lines, blank lines between them, and whatever the page happened to put
//! at the front of each verse. This module reads that text the way a person
//! reads it, and produces the verses, refrain and metadata that a
//! `.song.yml` file is made of.
//!
//! # What it goes on
//!
//! Blank lines separate the blocks; everything else is a guess about what a
//! block *is*:
//!
//! - A block opening with `Refrain`, `Chorus`, `Kehrvers`, `Bridge` and the
//!   like is that kind of part. The wording is kept as the part's label, so a
//!   text that said "Kehrvers" still says "Kehrvers" in the editor.
//! - A block opening with a number — `2`, `2.`, `2)`, `Verse 2`, `Strophe 2` —
//!   is that verse. The number is believed: hymn texts skip verses, and a text
//!   that jumps from 2 to 4 means it.
//! - Anything else is the next verse in sequence.
//! - `Author: …`, `Melodie: …`, `Copyright: …` and their siblings, before the
//!   first block of lyrics, are metadata rather than a verse.
//! - The first line, if it is none of the above, is the title.
//!
//! # What it does not do
//!
//! It does not decide anything the user cannot see and undo. The guess is
//! shown before it is saved, and every part of it is editable afterwards in
//! the ordinary editor — this is a head start, not an authority. A block it
//! reads wrongly costs a correction, never data: nothing is dropped, and text
//! it cannot place becomes a verse rather than disappearing.

use cantara_songlib::song::{
    LyricLanguage, Song, SongPart, SongPartContent, SongPartContentType, SongPartId, SongPartType,
};
use std::collections::BTreeMap;

/// One block of the pasted text, read as a part of the song.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GuessedPart {
    /// What kind of part this is.
    pub part_type: SongPartType,

    /// Which one of that kind, 1-based.
    pub number: u32,

    /// The heading the text used, where it had one.
    ///
    /// Kept so that a song which said "Kehrvers" does not come back saying
    /// "refrain.1": the user's own word for the part is information, and the
    /// editor shows it.
    pub label: Option<String>,

    /// The lines of the part, in order, with the heading removed.
    pub lines: Vec<String>,
}

impl GuessedPart {
    /// The lines as one block of text.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

/// What was read out of the pasted text.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Guess {
    /// The title, or empty when the text did not seem to carry one.
    pub title: String,

    /// The metadata lines that were recognised, by tag name.
    pub tags: BTreeMap<String, String>,

    /// The parts, in the order they appeared.
    pub parts: Vec<GuessedPart>,
}

impl Guess {
    /// Whether there is anything worth taking over.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.title.is_empty() && self.tags.is_empty()
    }

    /// The guess as a song the rest of the program can work with.
    ///
    /// A song with no title is named `Untitled` by the caller rather than
    /// here — this stays a faithful reading of what was pasted.
    pub fn to_song(&self) -> Song {
        let mut song = Song::new(&self.title);

        for (name, value) in &self.tags {
            song.set_tag(name, value);
        }

        for guessed in &self.parts {
            let mut part = SongPart::new(SongPartId::new(guessed.part_type, guessed.number));
            part.label = guessed.label.clone();
            part.add_content(SongPartContent::new(
                SongPartContentType::Lyrics {
                    language: LyricLanguage::Default,
                },
                guessed.text(),
            ));
            // The number came from the text, so a duplicate is possible — a
            // page that labels two blocks "2". `add_part_of_type` moves it to
            // the next free number instead of dropping one of them.
            if song.add_part(part.clone()).is_err() {
                let id = song.add_part_of_type(guessed.part_type, None);
                if let Some(placed) = song.part_mut(&id) {
                    placed.label = guessed.label.clone();
                    placed.add_content(SongPartContent::new(
                        SongPartContentType::Lyrics {
                            language: LyricLanguage::Default,
                        },
                        guessed.text(),
                    ));
                }
            }
        }

        song
    }
}

/// The words that introduce a part, and what they mean.
///
/// Lower-case and without punctuation; the text is normalised the same way
/// before it is looked up here. English and German because those are the
/// languages Cantara itself speaks — an unrecognised heading is not lost, it
/// simply makes the block a verse and stays on as its label.
const PART_WORDS: &[(&str, SongPartType)] = &[
    ("verse", SongPartType::Verse),
    ("strophe", SongPartType::Verse),
    ("vers", SongPartType::Verse),
    ("chorus", SongPartType::Chorus),
    ("refrain", SongPartType::Refrain),
    ("kehrvers", SongPartType::Refrain),
    ("kehrreim", SongPartType::Refrain),
    ("bridge", SongPartType::Bridge),
    ("brücke", SongPartType::Bridge),
    ("prechorus", SongPartType::PreChorus),
    ("pre-chorus", SongPartType::PreChorus),
    ("intro", SongPartType::Intro),
    ("outro", SongPartType::Outro),
    ("coda", SongPartType::Outro),
    ("interlude", SongPartType::Interlude),
    ("zwischenspiel", SongPartType::Interlude),
    ("instrumental", SongPartType::Instrumental),
    ("solo", SongPartType::Solo),
];

/// Metadata labels, and the tag name Cantara writes them under.
///
/// Several labels share a tag on purpose: a file written here should use one
/// name for one thing. Reading *other people's* files the other way round is
/// what [`crate::logic::tag_mapping`] is for — this is about what Cantara
/// itself puts in a new file.
const META_WORDS: &[(&str, &str)] = &[
    ("author", "author"),
    ("autor", "author"),
    ("text", "author"),
    ("words", "author"),
    ("worte", "author"),
    ("dichter", "author"),
    ("composer", "composer"),
    ("komponist", "composer"),
    ("music", "composer"),
    ("musik", "composer"),
    ("melody", "composer"),
    ("melodie", "composer"),
    ("tune", "composer"),
    ("weise", "composer"),
    ("translator", "translator"),
    ("translation", "translator"),
    ("übersetzung", "translator"),
    ("übersetzer", "translator"),
    ("copyright", "copyright"),
    ("ccli", "ccli"),
    ("year", "year"),
    ("jahr", "year"),
];

/// The form a heading or label is compared in: lower case, no trailing
/// punctuation, no surrounding space.
fn normalise(word: &str) -> String {
    word.trim()
        .trim_end_matches([':', '.', ')', '-', '–'])
        .trim()
        .to_lowercase()
}

/// A leading part marker, if the line opens with one.
///
/// Returns what kind of part it is, the number if one was given, the original
/// wording, and what is left of the line. That last part matters: hymnary
/// writes the first line of a verse as `1 Amazing grace! How sweet the sound`,
/// where the marker and the lyrics share a line.
fn part_marker(line: &str) -> Option<(SongPartType, Option<u32>, String, String)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    // `2`, `2.`, `2)` — a number on its own, possibly followed by lyrics.
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty() {
        let rest = &trimmed[digits.len()..];
        let separator: String = rest
            .chars()
            .take_while(|c| matches!(c, '.' | ')' | ':' | '-' | ' ' | '\t'))
            .collect();
        // A bare number with nothing after it is a marker; a number followed
        // straight by a letter (`1st verse`) is not.
        if (!separator.is_empty() || rest.is_empty())
            && let Ok(number) = digits.parse::<u32>()
        {
            let label = format!("{digits}{}", separator.trim_end());
            return Some((
                SongPartType::Verse,
                Some(number),
                label.trim().to_string(),
                rest[separator.len()..].trim().to_string(),
            ));
        }
    }

    // `Refrain:`, `Verse 2`, `Strophe 2:` — a word, optionally with a number.
    let (first_word, remainder) = match trimmed.split_once([' ', '\t']) {
        Some((word, rest)) => (word, rest),
        None => (trimmed, ""),
    };

    let part_type = PART_WORDS
        .iter()
        .find(|(word, _)| *word == normalise(first_word))
        .map(|(_, part_type)| *part_type)?;

    // A number may follow the word, and lyrics may follow that.
    let remainder_trimmed = remainder.trim_start();
    let number_digits: String = remainder_trimmed
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();

    let (number, after_number) = match number_digits.parse::<u32>() {
        Ok(number) => (Some(number), &remainder_trimmed[number_digits.len()..]),
        Err(_) => (None, remainder_trimmed),
    };

    let rest = after_number
        .trim_start_matches([':', '.', ')', '-', ' ', '\t'])
        .trim()
        .to_string();

    let label = match number {
        Some(number) => format!("{} {number}", first_word.trim_end_matches(':')),
        None => first_word.trim_end_matches(':').to_string(),
    };

    Some((part_type, number, label, rest))
}

/// A metadata line — `Author: John Newton` — as a tag name and value.
fn meta_line(line: &str) -> Option<(String, String)> {
    let (label, value) = line.split_once(':')?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    // A label is one or two words. Anything longer is a line of lyrics that
    // happens to contain a colon, and there are plenty of those.
    if label.split_whitespace().count() > 2 {
        return None;
    }

    let tag = META_WORDS
        .iter()
        .find(|(word, _)| *word == normalise(label))
        .map(|(_, tag)| (*tag).to_string())?;

    Some((tag, value.to_string()))
}

/// Read pasted text as a song.
///
/// See the [module documentation](self) for what the guess is based on. The
/// result is always usable: text that fits no pattern at all comes back as a
/// single verse.
pub fn guess(text: &str) -> Guess {
    // A line holding nothing but spaces or a tab is a blank line to the eye,
    // and copied text is full of them — a selection dragged across a web page
    // brings the indentation of the markup with it. Emptying them before the
    // split is what makes "blank lines separate the blocks" true of the text a
    // user actually pastes; without it two verses either side of such a line
    // merge into one.
    let normalised: String = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| match line.trim().is_empty() {
            true => "",
            false => line,
        })
        .collect::<Vec<&str>>()
        .join("\n");

    let mut result = Guess::default();

    // Blocks separated by one or more blank lines.
    let blocks: Vec<Vec<&str>> = normalised
        .split("\n\n")
        .map(|block| {
            block
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<&str>>()
        })
        .filter(|block: &Vec<&str>| !block.is_empty())
        .collect();

    let mut next_verse = 1;
    let mut seen_lyrics = false;

    // Whether a single line standing on its own at the top is a title.
    //
    // On its own the shape says nothing — a title is one line, and so is a
    // one-line verse. What settles it is what comes after: a title is followed
    // by something that looks like a song, meaning a block of several lines or
    // one that announces itself as a verse. Three one-line blocks in a row are
    // three short verses, and taking the first as a title would silently drop
    // it.
    let looks_like_a_song = |rest: &[Vec<&str>]| {
        rest.iter()
            .any(|block| block.len() > 1 || part_marker(block[0]).is_some())
    };

    for (index, block) in blocks.iter().enumerate() {
        // Metadata and the title only count before the singing starts. After
        // that, a short line with a colon is a line of the song.
        if !seen_lyrics {
            let meta: Vec<(String, String)> =
                block.iter().filter_map(|line| meta_line(line)).collect();

            if meta.len() == block.len() {
                for (tag, value) in meta {
                    result.tags.entry(tag).or_insert(value);
                }
                continue;
            }

            // A single line that is neither metadata nor a part marker, before
            // anything has been sung: the title — provided a song follows it.
            if result.title.is_empty()
                && block.len() == 1
                && part_marker(block[0]).is_none()
                && meta.is_empty()
                && looks_like_a_song(&blocks[index + 1..])
            {
                result.title = block[0].trim().to_string();
                continue;
            }
        }

        let (part_type, number, label, first_line) = match part_marker(block[0]) {
            Some(marker) => marker,
            None => (SongPartType::Verse, None, String::new(), String::new()),
        };

        let mut lines: Vec<String> = Vec::new();
        if label.is_empty() {
            // No marker at all: every line is content.
            lines.extend(block.iter().map(|line| line.trim().to_string()));
        } else {
            // The marker shared its line with lyrics often enough to matter.
            if !first_line.is_empty() {
                lines.push(first_line);
            }
            lines.extend(block[1..].iter().map(|line| line.trim().to_string()));
        }

        if lines.is_empty() {
            continue;
        }

        let number = match part_type {
            // A verse number in the text is believed — hymn texts skip verses.
            SongPartType::Verse => {
                let number = number.unwrap_or(next_verse);
                next_verse = number + 1;
                number
            }
            // Everything else is counted as it comes: a song with two bridges
            // gets bridge 1 and bridge 2.
            _ => {
                let taken = result
                    .parts
                    .iter()
                    .filter(|part| part.part_type == part_type)
                    .count() as u32;
                number.unwrap_or(taken + 1)
            }
        };

        seen_lyrics = true;
        result.parts.push(GuessedPart {
            part_type,
            number,
            label: match label.is_empty() {
                true => None,
                false => Some(label),
            },
            lines,
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a copy from hymnary.org actually looks like: a title, then verses
    /// whose number opens the first line of lyrics.
    #[test]
    fn reads_a_hymnary_style_paste() {
        let guess = guess(
            "Amazing Grace\n\
             \n\
             1 Amazing grace! How sweet the sound\n\
             that saved a wretch like me!\n\
             \n\
             2 'Twas grace that taught my heart to fear,\n\
             and grace my fears relieved;\n",
        );

        assert_eq!(guess.title, "Amazing Grace");
        assert_eq!(guess.parts.len(), 2);

        assert_eq!(guess.parts[0].part_type, SongPartType::Verse);
        assert_eq!(guess.parts[0].number, 1);
        assert_eq!(
            guess.parts[0].text(),
            "Amazing grace! How sweet the sound\nthat saved a wretch like me!"
        );

        assert_eq!(guess.parts[1].number, 2);
        assert!(guess.parts[1].text().starts_with("'Twas grace"));
    }

    /// The number in the text wins: hymn texts leave verses out, and renumbering
    /// them would quietly change which verse is which.
    #[test]
    fn a_skipped_verse_number_is_kept() {
        let guess = guess("1 First line\n\n4 Fourth line\n\n5 Fifth line");

        let numbers: Vec<u32> = guess.parts.iter().map(|part| part.number).collect();
        assert_eq!(numbers, vec![1, 4, 5]);
    }

    #[test]
    fn a_refrain_heading_on_its_own_line_is_recognised() {
        let guess = guess(
            "1 A verse line\n\
             \n\
             Refrain:\n\
             Praise God from whom all blessings flow\n",
        );

        assert_eq!(guess.parts.len(), 2);
        assert_eq!(guess.parts[1].part_type, SongPartType::Refrain);
        assert_eq!(guess.parts[1].number, 1);
        assert_eq!(
            guess.parts[1].text(),
            "Praise God from whom all blessings flow"
        );
    }

    /// The source's own wording survives, because it is what the user will look
    /// for in the editor.
    #[test]
    fn the_heading_is_kept_as_the_label() {
        let guess = guess("Kehrvers:\nLobe den Herren");

        assert_eq!(guess.parts[0].part_type, SongPartType::Refrain);
        assert_eq!(guess.parts[0].label.as_deref(), Some("Kehrvers"));
    }

    #[test]
    fn german_verse_headings_are_recognised() {
        let guess = guess("Strophe 1:\nLobe den Herren\n\nStrophe 2:\nLobe den Herren, der alles");

        assert_eq!(guess.parts.len(), 2);
        assert!(
            guess
                .parts
                .iter()
                .all(|part| part.part_type == SongPartType::Verse)
        );
        assert_eq!(guess.parts[1].number, 2);
        assert_eq!(guess.parts[1].label.as_deref(), Some("Strophe 2"));
    }

    /// Text with no markers at all still has to come out as something.
    #[test]
    fn unmarked_blocks_become_verses_in_order() {
        let guess = guess("First block line\n\nSecond block line\n\nThird block line");

        assert_eq!(guess.parts.len(), 3);
        let numbers: Vec<u32> = guess.parts.iter().map(|part| part.number).collect();
        assert_eq!(numbers, vec![1, 2, 3]);
        assert_eq!(guess.parts[0].text(), "First block line");
    }

    #[test]
    fn metadata_lines_become_tags() {
        let guess = guess(
            "Amazing Grace\n\
             \n\
             Author: John Newton\n\
             Melodie: Traditional\n\
             \n\
             1 Amazing grace\n",
        );

        assert_eq!(guess.tags.get("author").map(String::as_str), Some("John Newton"));
        assert_eq!(
            guess.tags.get("composer").map(String::as_str),
            Some("Traditional")
        );
        assert_eq!(guess.parts.len(), 1, "the metadata became a verse");
    }

    /// A line of lyrics with a colon in it is a line of lyrics.
    #[test]
    fn a_colon_inside_the_song_is_not_metadata() {
        let guess = guess(
            "1 And this he said to me: rejoice\n\
             for I have overcome the world\n",
        );

        assert!(guess.tags.is_empty(), "{:?}", guess.tags);
        assert_eq!(guess.parts.len(), 1);
        assert!(guess.parts[0].text().starts_with("And this he said"));
    }

    /// Text copied out of a web page carries the markup's indentation, so the
    /// line between two verses is very often spaces rather than nothing. To
    /// the eye it is blank, and it has to separate the blocks like any other.
    #[test]
    fn a_separator_line_of_spaces_still_separates() {
        let guess = guess("1 First verse\n   \n2 Second verse\n\t\n3 Third verse");

        assert_eq!(guess.parts.len(), 3);
        assert_eq!(guess.parts[0].text(), "First verse");
        assert_eq!(guess.parts[1].text(), "Second verse");
        assert_eq!(guess.parts[2].text(), "Third verse");
    }

    /// The same, where it decides whether a heading is metadata or a verse.
    #[test]
    fn whitespace_separators_do_not_merge_metadata_into_a_verse() {
        let spaced = guess("Amazing Grace\n \nAuthor: John Newton\n \n1 Amazing grace");
        let empty = guess("Amazing Grace\n\nAuthor: John Newton\n\n1 Amazing grace");

        assert_eq!(spaced, empty);
        assert_eq!(spaced.title, "Amazing Grace");
        assert_eq!(
            spaced.tags.get("author").map(String::as_str),
            Some("John Newton")
        );
        assert_eq!(spaced.parts.len(), 1);
    }

    #[test]
    fn windows_line_endings_are_read_the_same() {
        let unix = guess("Title\n\n1 A line\n\nRefrain:\nAnother line");
        let windows = guess("Title\r\n\r\n1 A line\r\n\r\nRefrain:\r\nAnother line");

        assert_eq!(unix, windows);
    }

    #[test]
    fn empty_text_gives_an_empty_guess() {
        assert!(guess("").is_empty());
        assert!(guess("   \n\n  \n").is_empty());
    }

    /// Two blocks labelled the same is a real thing on lyrics sites. Neither
    /// may be lost.
    #[test]
    fn a_repeated_number_keeps_both_blocks() {
        let guess = guess("2 First text\n\n2 Second text");
        let song = guess.to_song();

        assert_eq!(song.parts().len(), 2);
        let texts: Vec<&str> = song
            .parts()
            .iter()
            .flat_map(|part| part.contents.iter())
            .map(|content| content.content.as_str())
            .collect();
        assert!(texts.contains(&"First text"), "{texts:?}");
        assert!(texts.contains(&"Second text"), "{texts:?}");
    }

    #[test]
    fn the_guess_becomes_a_song() {
        let guess = guess(
            "Amazing Grace\n\
             \n\
             Author: John Newton\n\
             \n\
             1 Amazing grace\n\
             \n\
             Refrain:\n\
             Praise God\n",
        );
        let song = guess.to_song();

        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.tag("author").map(String::as_str), Some("John Newton"));
        assert_eq!(song.parts().len(), 2);

        let refrain = song
            .parts()
            .iter()
            .find(|part| part.part_type == SongPartType::Refrain)
            .expect("the refrain became a part");
        assert_eq!(refrain.display_label(), "Refrain");
        assert_eq!(refrain.contents[0].content, "Praise God");
    }

    /// A number that opens a word rather than a verse must not eat the line.
    #[test]
    fn a_number_glued_to_a_word_is_not_a_marker() {
        assert!(part_marker("1st of May").is_none());
        assert!(part_marker("Amazing grace").is_none());
    }

    #[test]
    fn a_bare_number_line_is_a_marker() {
        let (part_type, number, label, rest) = part_marker("2.").expect("a marker");
        assert_eq!(part_type, SongPartType::Verse);
        assert_eq!(number, Some(2));
        assert_eq!(label, "2.");
        assert!(rest.is_empty());
    }
}
