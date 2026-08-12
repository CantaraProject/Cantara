//! Putting a file into a repository, and moving it between them.
//!
//! A repository is a folder the user pointed Cantara at. Creating a song means
//! writing a file into one of them; changing which repository a song belongs
//! to means moving that file; keeping it in both means copying it. There is no
//! index and no database — the folder *is* the library — so all three are file
//! operations, and the rules they follow are the ones a careful person would
//! follow by hand:
//!
//! - **Nothing is ever overwritten.** A name already taken by different
//!   content gets a free one beside it — `Amazing Grace (2).song.yml`. A
//!   library is somebody's own work; an operation here may add to it and may
//!   move something the user pointed at, never replace something they did not.
//! - **Only local folders.** A remote repository is a download, unpacked fresh
//!   on every start. Writing into it would be writing into a temporary
//!   directory, which is worse than refusing. Whether a repository can be
//!   written to is [`Settings::writable_repositories`] to answer; this module
//!   takes the folder it is given.
//! - **A move that cannot finish leaves the original alone.** The copy is made
//!   first and the original removed only once it has arrived, so an
//!   interruption costs a duplicate rather than the file.
//!
//! [`Settings::writable_repositories`]: crate::logic::settings::Settings::writable_repositories

use std::path::{Path, PathBuf};

/// What kind of file the editor can create.
///
/// Only the two formats Cantara can *write*. It reads several song formats —
/// classic `.song`, CCLI, CSSF — but writing them back would lose what their
/// importers cannot reconstruct, so a new song is always a `.song.yml`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NewFileKind {
    /// A song, as YAML.
    Song,
    /// A Markdown document.
    Markdown,
}

impl NewFileKind {
    /// Every kind, in the order the dialog offers them.
    pub const ALL: &'static [NewFileKind] = &[NewFileKind::Song, NewFileKind::Markdown];

    /// The file name suffix, including the dot.
    pub fn suffix(self) -> &'static str {
        match self {
            NewFileKind::Song => ".song.yml",
            NewFileKind::Markdown => ".md",
        }
    }

    /// What the dialog calls it.
    pub fn label_key(self) -> &'static str {
        match self {
            NewFileKind::Song => "detail.new_song",
            NewFileKind::Markdown => "detail.new_markdown",
        }
    }

    /// What a file of this kind holds when it has just been created.
    ///
    /// A song needs the little that makes it a valid song file; a Markdown
    /// document starts as its own title and nothing else, which is what a
    /// person would type first anyway.
    pub fn initial_content(self, title: &str) -> String {
        match self {
            NewFileKind::Song => format!(
                "version: 0.1\ntitle: {}\nparts: []\n",
                yaml_string(title)
            ),
            NewFileKind::Markdown => format!("# {title}\n"),
        }
    }
}

/// A title as a YAML scalar that means exactly the title.
///
/// Written plainly, `title: Psalm 23: The Lord is my shepherd` is not valid
/// YAML at all, and a song called `#1` or one holding a quote or a line break
/// is read as something else. The file would be written and then refuse to
/// open — the one moment a user has no reason to suspect their own title.
///
/// The escaping is `serde_json`'s. A YAML 1.2 double-quoted scalar uses the
/// same escapes as a JSON string, so a JSON string *is* one; borrowing the
/// encoder is safer than writing a fourth one by hand.
fn yaml_string(title: &str) -> String {
    serde_json::to_string(title).unwrap_or_else(|_| "\"\"".to_string())
}

/// Why a file could not be created, moved or copied.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FileTaskError {
    /// The file system refused.
    Io { name: String, reason: String },

    /// A thousand names were taken. Something is wrong that retrying will not
    /// fix, and the alternative — overwriting — is not on offer.
    NoFreeName(String),

    /// The thing to move is not a file that can be moved.
    NotAFile(String),

    /// The file is already in that repository.
    AlreadyThere,
}

impl FileTaskError {
    /// The message to show, as a translation key with its parameters.
    ///
    /// Kept beside the error so that a new one cannot be added without a
    /// message: the match is exhaustive.
    pub fn message_key(&self) -> (&'static str, Vec<(&'static str, String)>) {
        match self {
            FileTaskError::Io { name, reason } => (
                "detail.file_error_io",
                vec![("name", name.clone()), ("reason", reason.clone())],
            ),
            FileTaskError::NoFreeName(name) => (
                "detail.file_error_no_free_name",
                vec![("name", name.clone())],
            ),
            FileTaskError::NotAFile(name) => {
                ("detail.file_error_not_a_file", vec![("name", name.clone())])
            }
            FileTaskError::AlreadyThere => ("detail.file_error_already_there", vec![]),
        }
    }
}

fn io_error(path: &Path, error: std::io::Error) -> FileTaskError {
    FileTaskError::Io {
        name: path.display().to_string(),
        reason: error.to_string(),
    }
}

/// A file name built from what the user typed.
///
/// The characters a file name cannot hold become dashes rather than being
/// dropped, so two songs whose titles differ only there stay two names. An
/// empty title would make a file called nothing but its suffix, so it becomes
/// a word instead.
pub fn file_name_for(title: &str, kind: NewFileKind) -> String {
    let cleaned: String = title
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect();

    let stem = match cleaned.trim() {
        "" => "Untitled",
        trimmed => trimmed,
    };

    format!("{stem}{}", kind.suffix())
}

/// A path in `folder` that nothing occupies yet.
///
/// `Amazing Grace.song.yml` becomes `Amazing Grace (2).song.yml` and so on.
/// The suffix is split at the *first* dot so that `.song.yml` survives whole —
/// `Path::file_stem` would leave `Amazing Grace.song (2).yml`, which is a file
/// Cantara no longer recognises as a song.
fn free_path(folder: &Path, file_name: &str) -> Result<PathBuf, FileTaskError> {
    let (stem, suffix) = match file_name.split_once('.') {
        Some((stem, suffix)) => (stem.to_string(), format!(".{suffix}")),
        None => (file_name.to_string(), String::new()),
    };

    for attempt in 0..1000 {
        let candidate = match attempt {
            0 => folder.join(file_name),
            _ => folder.join(format!("{stem} ({}){suffix}", attempt + 1)),
        };
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(FileTaskError::NoFreeName(file_name.to_string()))
}

/// Writes a new file into `folder`, without touching anything already there.
///
/// Returns where it landed, which is not necessarily the name that was asked
/// for — see [`free_path`].
pub fn create(folder: &Path, file_name: &str, content: &str) -> Result<PathBuf, FileTaskError> {
    std::fs::create_dir_all(folder).map_err(|error| io_error(folder, error))?;
    let path = free_path(folder, file_name)?;
    std::fs::write(&path, content).map_err(|error| io_error(&path, error))?;
    Ok(path)
}

/// The file name to give `file` in another folder.
fn name_of(file: &Path) -> Result<String, FileTaskError> {
    file.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| FileTaskError::NotAFile(file.display().to_string()))
}

/// Copies `file` into `folder`, keeping the original where it is.
pub fn copy_into(file: &Path, folder: &Path) -> Result<PathBuf, FileTaskError> {
    if !file.is_file() {
        return Err(FileTaskError::NotAFile(file.display().to_string()));
    }
    if file.parent() == Some(folder) {
        return Err(FileTaskError::AlreadyThere);
    }

    std::fs::create_dir_all(folder).map_err(|error| io_error(folder, error))?;
    let target = free_path(folder, &name_of(file)?)?;
    std::fs::copy(file, &target).map_err(|error| io_error(&target, error))?;
    Ok(target)
}

/// Moves `file` into `folder`.
///
/// The copy is made first and the original removed only once it has arrived.
/// `rename` would do both at once and is tried first, but it fails across
/// drives — and a library spread over `C:` and an external disk is exactly the
/// case this has to survive.
pub fn move_into(file: &Path, folder: &Path) -> Result<PathBuf, FileTaskError> {
    let target = copy_into(file, folder)?;

    if let Err(error) = std::fs::remove_file(file) {
        // The copy is there and the original could not go. Leaving both is the
        // only honest outcome; removing the copy instead would throw away the
        // one operation that did work.
        return Err(FileTaskError::Io {
            name: file.display().to_string(),
            reason: error.to_string(),
        });
    }

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn folder() -> TempDir {
        TempDir::new().expect("a temporary directory")
    }

    #[test]
    fn a_new_song_gets_the_yaml_suffix() {
        assert_eq!(
            file_name_for("Amazing Grace", NewFileKind::Song),
            "Amazing Grace.song.yml"
        );
        assert_eq!(
            file_name_for("Notes", NewFileKind::Markdown),
            "Notes.md"
        );
    }

    #[test]
    fn characters_a_file_name_cannot_hold_become_dashes() {
        assert_eq!(
            file_name_for("Who/What: Why?", NewFileKind::Song),
            "Who-What- Why-.song.yml"
        );
    }

    #[test]
    fn a_title_of_nothing_still_makes_a_file() {
        assert_eq!(file_name_for("   ", NewFileKind::Song), "Untitled.song.yml");
    }

    #[test]
    fn a_new_song_file_is_a_song_file() {
        let content = NewFileKind::Song.initial_content("Amazing Grace");
        let song = crate::logic::export::song_from_content("X.song.yml", &content)
            .expect("a new song file has to be readable");
        assert_eq!(song.title, "Amazing Grace");
    }

    /// A title is whatever the user typed, and YAML gives several ordinary
    /// characters a meaning. Written plainly, `Psalm 23: The Lord` is not
    /// valid YAML — the file would be created and then refuse to open.
    #[test]
    fn a_title_yaml_would_misread_survives() {
        for title in [
            "Psalm 23: The Lord is my shepherd",
            "#1 in the book",
            "He said \"peace\"",
            "Ends with a backslash \\",
            "A title\nwith a line break",
            "  leading and trailing  ",
            "- not a list item",
            "{braces} and [brackets]",
            "",
        ] {
            let content = NewFileKind::Song.initial_content(title);
            let song = crate::logic::export::song_from_content("X.song.yml", &content)
                .unwrap_or_else(|error| {
                    panic!("{title:?} produced a file that cannot be read: {error:?}\n{content}")
                });

            // An empty title is filled in from the file name by the reader,
            // which is the behaviour every other format gets too.
            if !title.trim().is_empty() {
                assert_eq!(song.title, title, "the title came back changed");
            }
        }
    }

    #[test]
    fn creating_writes_the_file() {
        let folder = folder();
        let path = create(folder.path(), "Song.song.yml", "content").expect("created");

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "content");
        assert_eq!(path.file_name().unwrap(), "Song.song.yml");
    }

    /// The rule the whole module is built on.
    #[test]
    fn an_existing_file_is_never_overwritten() {
        let folder = folder();
        create(folder.path(), "Song.song.yml", "the original").expect("created");
        let second = create(folder.path(), "Song.song.yml", "something else").expect("created");

        assert_eq!(second.file_name().unwrap(), "Song (2).song.yml");
        assert_eq!(
            std::fs::read_to_string(folder.path().join("Song.song.yml")).unwrap(),
            "the original"
        );
    }

    /// `.song.yml` is two suffixes, and only the whole pair makes it a song.
    #[test]
    fn the_double_suffix_survives_renaming() {
        let folder = folder();
        create(folder.path(), "Song.song.yml", "one").expect("created");
        let second = create(folder.path(), "Song.song.yml", "two").expect("created");

        let name = second.file_name().unwrap().to_str().unwrap();
        assert!(name.ends_with(".song.yml"), "{name}");
        assert_eq!(
            crate::logic::sourcefiles::SourceFileType::of(name),
            Some(crate::logic::sourcefiles::SourceFileType::Song)
        );
    }

    #[test]
    fn copying_leaves_the_original() {
        let source = folder();
        let target = folder();
        let file = create(source.path(), "Song.song.yml", "content").expect("created");

        let copy = copy_into(&file, target.path()).expect("copied");

        assert!(file.exists(), "the original was removed");
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "content");
    }

    #[test]
    fn moving_takes_the_original_with_it() {
        let source = folder();
        let target = folder();
        let file = create(source.path(), "Song.song.yml", "content").expect("created");

        let moved = move_into(&file, target.path()).expect("moved");

        assert!(!file.exists(), "the original stayed behind");
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), "content");
    }

    /// Moving onto a name that is taken must not eat the file that is there.
    #[test]
    fn moving_onto_a_taken_name_keeps_both() {
        let source = folder();
        let target = folder();
        let file = create(source.path(), "Song.song.yml", "the one being moved").expect("created");
        create(target.path(), "Song.song.yml", "the one already there").expect("created");

        let moved = move_into(&file, target.path()).expect("moved");

        assert_eq!(moved.file_name().unwrap(), "Song (2).song.yml");
        assert_eq!(
            std::fs::read_to_string(target.path().join("Song.song.yml")).unwrap(),
            "the one already there"
        );
        assert_eq!(
            std::fs::read_to_string(&moved).unwrap(),
            "the one being moved"
        );
    }

    #[test]
    fn a_file_cannot_be_moved_into_its_own_folder() {
        let source = folder();
        let file = create(source.path(), "Song.song.yml", "content").expect("created");

        assert_eq!(
            copy_into(&file, source.path()),
            Err(FileTaskError::AlreadyThere)
        );
        assert!(file.exists());
    }

    #[test]
    fn a_missing_file_is_reported_rather_than_moved() {
        let source = folder();
        let target = folder();
        let missing = source.path().join("nothing.song.yml");

        assert!(matches!(
            move_into(&missing, target.path()),
            Err(FileTaskError::NotAFile(_))
        ));
    }

    /// The target folder is made when it is not there — a repository the user
    /// configured but never filled.
    #[test]
    fn a_missing_target_folder_is_created() {
        let source = folder();
        let target = folder();
        let nested = target.path().join("Lieder");
        let file = create(source.path(), "Song.song.yml", "content").expect("created");

        let moved = move_into(&file, &nested).expect("moved");

        assert!(moved.starts_with(&nested));
    }

    #[test]
    fn every_error_has_a_message() {
        for error in [
            FileTaskError::Io {
                name: String::new(),
                reason: String::new(),
            },
            FileTaskError::NoFreeName(String::new()),
            FileTaskError::NotAFile(String::new()),
            FileTaskError::AlreadyThere,
        ] {
            let (key, _) = error.message_key();
            assert!(
                crate::logic::localisation::is_translated(key),
                "{key} has no message"
            );
        }
    }
}
