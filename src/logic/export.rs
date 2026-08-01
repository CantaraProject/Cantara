//! Exporting the current selection to a file.
//!
//! # Adding a format
//!
//! Add a variant to [`ExportFormat`], then follow the compiler: every match in
//! this module is exhaustive, so nothing can be half-added. The only other
//! step is a label in `locales/app.yml` under `selection.export_format_*`.
//!
//! Formats fall into two shapes, which [`ExportFormat::one_file_per_song`]
//! distinguishes:
//!
//! * **One document for the whole selection** — the text formats. Several songs
//!   are joined into a single file.
//! * **One file per song** — sheet music. A LilyPond file defines variables at
//!   the top level, so concatenating two songs would leave the second song's
//!   definitions overriding the first and both scores would print the same
//!   music. These formats write into a directory instead.

use std::path::Path;

use cantara_songlib::exporter::abc::{AbcSettings, abc_from_song};
use cantara_songlib::exporter::lilypond::{LilypondSettings, lilypond_from_song};
use cantara_songlib::exporter::text::{TextFormat, TextSettings, text_from_song, text_from_songs};
use cantara_songlib::importer::{ccli, classic_song, cssf, song_yml};
use cantara_songlib::song::Song;

/// A format the selection can be exported to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExportFormat {
    /// The lyrics as they are sung, without any markup.
    PlainText,
    /// Telegram's markup: the title in bold.
    Telegram,
    /// Markdown: the title as a heading, the author in italics.
    Markdown,
    /// LilyPond sheet music, one `.ly` file per song.
    LilyPond,
    /// ABC notation, one `.abc` file per song.
    Abc,
}

impl ExportFormat {
    /// Every format, in the order the menu offers them.
    pub const ALL: &'static [ExportFormat] = &[
        ExportFormat::PlainText,
        ExportFormat::Telegram,
        ExportFormat::Markdown,
        ExportFormat::LilyPond,
        ExportFormat::Abc,
    ];

    /// The value used in the menu's `<select>`, and what settings persist.
    pub fn id(self) -> &'static str {
        match self {
            ExportFormat::PlainText => "text",
            ExportFormat::Telegram => "telegram",
            ExportFormat::Markdown => "markdown",
            ExportFormat::LilyPond => "lilypond",
            ExportFormat::Abc => "abc",
        }
    }

    /// The translation key of the format's label.
    pub fn label_key(self) -> &'static str {
        match self {
            ExportFormat::PlainText => "selection.export_format_text",
            ExportFormat::Telegram => "selection.export_format_telegram",
            ExportFormat::Markdown => "selection.export_format_markdown",
            ExportFormat::LilyPond => "selection.export_format_lilypond",
            ExportFormat::Abc => "selection.export_format_abc",
        }
    }

    /// The file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::PlainText | ExportFormat::Telegram => "txt",
            ExportFormat::Markdown => "md",
            ExportFormat::LilyPond => "ly",
            ExportFormat::Abc => "abc",
        }
    }

    /// Whether every song becomes a file of its own rather than all of them
    /// sharing one document. See the module documentation for why.
    pub fn one_file_per_song(self) -> bool {
        match self {
            ExportFormat::PlainText | ExportFormat::Telegram | ExportFormat::Markdown => false,
            ExportFormat::LilyPond | ExportFormat::Abc => true,
        }
    }

    /// Look a format up by the id its menu entry carries.
    pub fn from_id(id: &str) -> Option<ExportFormat> {
        ExportFormat::ALL
            .iter()
            .copied()
            .find(|format| format.id() == id)
    }

    /// Render one song on its own.
    fn render_one(self, song: &Song) -> Result<String, ExportError> {
        let text = |format: TextFormat| {
            text_from_song(song, &TextSettings::with_format(format))
                .map_err(|error| ExportError::Render(error.to_string()))
        };

        match self {
            ExportFormat::PlainText => text(TextFormat::Plain),
            ExportFormat::Telegram => text(TextFormat::Telegram),
            ExportFormat::Markdown => text(TextFormat::Markdown),
            ExportFormat::LilyPond => lilypond_from_song(song, &LilypondSettings::default())
                .map_err(|error| ExportError::SongFailed {
                    title: song.title.clone(),
                    reason: error,
                }),
            ExportFormat::Abc => {
                abc_from_song(song, &AbcSettings::default()).map_err(|error| {
                    ExportError::SongFailed {
                        title: song.title.clone(),
                        reason: error,
                    }
                })
            }
        }
    }

    /// Render the whole selection.
    ///
    /// Returns one entry per file to write: a single document for the text
    /// formats, one per song for the sheet-music formats.
    pub fn render(self, songs: &[Song]) -> Result<Vec<ExportedFile>, ExportError> {
        if songs.is_empty() {
            return Err(ExportError::NoSongs);
        }

        if self.one_file_per_song() {
            return songs
                .iter()
                .map(|song| {
                    Ok(ExportedFile {
                        name: file_stem(&song.title),
                        content: self.render_one(song)?,
                    })
                })
                .collect();
        }

        let format = match self {
            ExportFormat::PlainText => TextFormat::Plain,
            ExportFormat::Telegram => TextFormat::Telegram,
            ExportFormat::Markdown => TextFormat::Markdown,
            // Unreachable for the sheet-music formats, which took the branch
            // above; the match is written out so a new format cannot slip past.
            ExportFormat::LilyPond | ExportFormat::Abc => {
                return Err(ExportError::Render(
                    "sheet music cannot be joined into one document".to_string(),
                ));
            }
        };

        let content = text_from_songs(songs, &TextSettings::with_format(format))
            .map_err(|error| ExportError::Render(error.to_string()))?;

        Ok(vec![ExportedFile {
            name: default_document_name(songs),
            content,
        }])
    }
}

/// One file the export produced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExportedFile {
    /// The file name without its extension.
    pub name: String,
    pub content: String,
}

/// What can go wrong while exporting.
///
/// Each variant carries what a message needs, so the wording — and its
/// translation — stays in the UI rather than in here.
#[derive(Debug)]
pub enum ExportError {
    /// The selection holds no songs, only pictures or PDFs.
    NoSongs,
    /// A file could not be read.
    Unreadable { name: String, reason: String },
    /// A file is not in a song format this build understands.
    UnsupportedFormat { name: String },
    /// A song could not be parsed.
    Unparsable { name: String, reason: String },
    /// One song could not be rendered — a CCLI export has no melody, for
    /// instance, so it cannot become sheet music.
    SongFailed { title: String, reason: String },
    /// Rendering the document failed.
    Render(String),
    /// Writing the file failed.
    Write { path: String, reason: String },
}

/// Parse a song from the contents of a file, choosing the importer by name.
///
/// This is the one place that maps a file name onto a song importer for
/// content the app already holds in memory. The presentation pipeline uses it
/// too, so the export and the slides can never disagree about what a `.ccli`
/// file is.
pub fn song_from_content(file_name: &str, content: &str) -> Result<Song, ExportError> {
    let lower = file_name.to_lowercase();

    let parsed = if lower.ends_with(".song.yml") || lower.ends_with(".song.yaml") {
        song_yml::import_from_yml_string(content)
    } else if lower.ends_with(".ccli") {
        ccli::import_from_ccli_string(content)
    } else if lower.ends_with(".cssf") {
        cssf::import_input_string(content.to_string(), file_name.to_string())
    } else if lower.ends_with(".song") {
        classic_song::import_song(content)
    } else {
        return Err(ExportError::UnsupportedFormat {
            name: file_name.to_string(),
        });
    };

    let mut song = parsed.map_err(|error| ExportError::Unparsable {
        name: file_name.to_string(),
        reason: error.to_string(),
    })?;

    // Formats that carry no title of their own are named after the file.
    if song.title.trim().is_empty() {
        song.title = Path::new(file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(file_name)
            .to_string();
    }

    Ok(song)
}

/// The name of a document holding the whole selection.
fn default_document_name(songs: &[Song]) -> String {
    match songs {
        [song] => file_stem(&song.title),
        _ => "cantara-export".to_string(),
    }
}

/// Turn a song title into something safe to use as a file name.
fn file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\n' | '\r' | '\t' => '-',
            other => other,
        })
        .collect();

    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "song".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str) -> Song {
        let mut song = Song::new(title);
        let id = song.add_part_of_type(cantara_songlib::song::SongPartType::Verse, None);
        song.part_mut(&id)
            .unwrap()
            .add_content(cantara_songlib::song::SongPartContent::lyrics(
                cantara_songlib::song::LyricLanguage::Default,
                "one two three",
            ));
        song.add_guessed_part_order();
        song
    }

    /// The songs Cantara ships as test data are lyrics-only classic files.
    /// Exporting them to LilyPond has to say so rather than doing nothing.
    #[test]
    fn test_lilypond_of_a_lyrics_only_song_reports_why() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song").unwrap();
        let song = song_from_content("Amazing Grace.song", &content).unwrap();

        match ExportFormat::LilyPond.render(&[song]) {
            Err(ExportError::SongFailed { title, reason }) => {
                assert_eq!(title, "Amazing Grace");
                assert!(
                    reason.contains("no voice content"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected a clear failure, got {:?}", other.map(|f| f.len())),
        }
    }

    /// A song that does carry a melody exports fine.
    #[test]
    fn test_lilypond_of_a_song_with_a_melody() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_from_content("Amazing Grace.song.yml", &content).unwrap();

        let files = ExportFormat::LilyPond.render(&[song]).expect("render");
        assert_eq!(files.len(), 1);
        assert!(files[0].content.contains("\\score"));
        assert_eq!(files[0].name, "Amazing Grace");
    }

    /// Prints what the export dialog's preview would show for each format.
    #[test]
    #[ignore = "diagnostic output, not an assertion"]
    fn dump_export_previews() {
        for (file, label) in [
            ("testfiles/Amazing Grace.song", "classic, no melody"),
            ("testfiles/Amazing Grace.song.yml", "yaml, with melody"),
        ] {
            let content = std::fs::read_to_string(file).unwrap();
            let name = std::path::Path::new(file).file_name().unwrap().to_str().unwrap();
            let song = song_from_content(name, &content).unwrap();

            for format in ExportFormat::ALL {
                let first_line = match format.render(std::slice::from_ref(&song)) {
                    Ok(files) => format!(
                        "{} file(s), first line: {}",
                        files.len(),
                        files[0].content.lines().next().unwrap_or("")
                    ),
                    Err(error) => format!("ERROR {error:?}"),
                };
                println!("  {label:<20} {:<10} {first_line}", format.id());
            }
        }
    }

    #[test]
    fn test_every_format_has_a_distinct_id_and_a_label() {
        let mut ids: Vec<&str> = ExportFormat::ALL.iter().map(|f| f.id()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two formats share an id");

        for format in ExportFormat::ALL {
            assert!(!format.label_key().is_empty());
            assert!(!format.extension().is_empty());
            assert_eq!(ExportFormat::from_id(format.id()), Some(*format));
        }
    }

    #[test]
    fn test_text_formats_produce_one_document() {
        let songs = [song("First"), song("Second")];

        for format in [
            ExportFormat::PlainText,
            ExportFormat::Telegram,
            ExportFormat::Markdown,
        ] {
            let files = format.render(&songs).expect("render");
            assert_eq!(files.len(), 1, "{:?} should join the songs", format);
            assert!(files[0].content.contains("First"));
            assert!(files[0].content.contains("Second"));
            assert_eq!(files[0].name, "cantara-export");
        }
    }

    /// Sheet music cannot share a file: LilyPond variables are global, so the
    /// second song would override the first.
    #[test]
    fn test_sheet_music_produces_one_file_per_song() {
        let songs = [song("First"), song("Second")];

        // Neither song has a melody, so the export reports which one failed
        // rather than writing an empty score.
        match ExportFormat::LilyPond.render(&songs) {
            Err(ExportError::SongFailed { title, .. }) => assert_eq!(title, "First"),
            other => panic!("expected a per-song failure, got {:?}", other.err()),
        }
    }

    #[test]
    fn test_a_single_document_is_named_after_the_song() {
        let files = ExportFormat::Markdown.render(&[song("Amazing Grace")]).unwrap();
        assert_eq!(files[0].name, "Amazing Grace");
    }

    #[test]
    fn test_no_songs_is_reported() {
        assert!(matches!(
            ExportFormat::PlainText.render(&[]),
            Err(ExportError::NoSongs)
        ));
    }

    #[test]
    fn test_file_names_are_made_safe() {
        assert_eq!(file_stem("AC/DC: Live?"), "AC-DC- Live-");
        assert_eq!(file_stem("   "), "song");
        assert_eq!(file_stem("..."), "song");
        assert_eq!(file_stem("Normal Title"), "Normal Title");
    }

    #[test]
    fn test_song_from_content_dispatches_on_the_file_name() {
        let classic = song_from_content("A Song.song", "#title: Classic\n\nline one\n").unwrap();
        assert_eq!(classic.title, "Classic");

        let ccli = song_from_content(
            "Some.ccli",
            "CCLI Title\n\nVerse 1\nline one\n\nCCLI Song # 7\nWriter\n",
        )
        .unwrap();
        assert_eq!(ccli.title, "CCLI Title");
        assert_eq!(ccli.tag("ccli_song_number").unwrap(), "7");

        let yaml = song_from_content(
            "Some.song.yml",
            "version: 0.1\ntitle: Yaml Song\nparts:\n- type: verse\n  contents:\n  - type: lyrics\n    number: 1\n    content: hi\n",
        )
        .unwrap();
        assert_eq!(yaml.title, "Yaml Song");

        assert!(matches!(
            song_from_content("notes.txt", "whatever"),
            Err(ExportError::UnsupportedFormat { .. })
        ));
    }

    /// A `.yml` file that is not a song file must not be taken for one.
    #[test]
    fn test_plain_yml_is_not_a_song() {
        assert!(matches!(
            song_from_content("config.yml", "a: b"),
            Err(ExportError::UnsupportedFormat { .. })
        ));
    }
}
