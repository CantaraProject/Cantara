//! Where the translations live, and what keeps every one of them findable.
//!
//! The texts sit in `locales/`, split by the part of the program they belong
//! to — `selection.yml`, `settings.yml`, `display.yml` and so on.  Nothing has
//! to be registered anywhere: `rust_i18n::i18n!("locales", …)` reads
//! **every** file under that folder and merges them, section by section, at
//! compile time.  A new file is picked up by existing; a section that appears
//! in two files is combined rather than replaced.
//!
//! That leaves three ways to lose a text, all of them silent, and all three are
//! held shut by the tests below:
//!
//! 1. **A key nobody wrote.** `rust_i18n` answers a missing key with the key
//!    itself, so the program shows `settings.song_slide_headline` where a
//!    heading belongs.  Every `t!("…")` in the source is looked up.
//! 2. **A key written twice.** Two files carrying the same key merge, and the
//!    one read later wins — quietly, and the loser may be the one somebody just
//!    edited.  The files are checked against each other.
//! 3. **A file that is not read as it looks.** The version has to be a *whole
//!    number*: `_version: 2.0` parses as a float, `as_u64()` gives nothing, and
//!    rust-i18n falls back to its version-1 format, in which the file means
//!    something else entirely.  That is how `wizard.yml` sat there doing
//!    nothing.  Every file is checked.

/// A moment, written the way the user's language writes one.
///
/// Answers the date and the time separately, because the monitor view's clock
/// widget shows the time always and the date only if it was asked to.
///
/// # Why this is not a format string in the settings
///
/// It could be, and then a German installation would show an American date
/// until somebody went and changed it. The pattern belongs to the language, so
/// it lives with the other texts of the language — `general.date_format` and
/// `general.time_format` — and a translator adding a language brings its date
/// format with it rather than filing a bug about one.
///
/// # Why the patterns are numeric
///
/// `chrono` writes month and day names in English and only in English unless
/// its `unstable-locales` feature is turned on. A German date reading
/// "Sat 05 Sep 2026" would be worse than one reading "05.09.2026", so the
/// patterns say nothing that would have to be translated.
///
/// # Local, not UTC
///
/// The clock on a stage monitor is the clock on the wall of that building. A
/// [`Timestamp`](crate::logic::timer::Timestamp) is a count from the epoch and
/// says nothing about where it was taken, so the conversion to local time
/// happens here, once.
pub fn format_clock(at: crate::logic::timer::Timestamp) -> (String, String) {
    use chrono::{Local, TimeZone};

    let Some(utc) = chrono::DateTime::from_timestamp_millis(at.milliseconds()) else {
        // Only reachable for a clock set hundreds of thousands of years from
        // now. There is nothing sensible to show, and a monitor in front of a
        // congregation is not the place to panic about it.
        return (String::new(), String::new());
    };
    let local = Local.from_utc_datetime(&utc.naive_utc());

    (
        strftime(&local, &rust_i18n::t!("general.date_format"), "%d.%m.%Y"),
        strftime(&local, &rust_i18n::t!("general.time_format"), "%H:%M"),
    )
}

/// Applies a `strftime` pattern, falling back when the pattern is not one.
///
/// The pattern comes out of a translation file, which is a text file a person
/// edits. `chrono` reports an unreadable pattern as a formatting error, and a
/// widget on a platform monitor should show the time in the wrong order rather
/// than nothing at all — so a pattern that will not do is replaced by one that
/// will.
fn strftime(
    at: &chrono::DateTime<chrono::Local>,
    pattern: &str,
    fallback: &str,
) -> String {
    use std::fmt::Write;

    let mut written = String::new();
    match write!(&mut written, "{}", at.format(pattern)) {
        Ok(()) => written,
        Err(_) => at.format(fallback).to_string(),
    }
}

/// Whether the key has a text behind it.
///
/// `rust_i18n` answers a key it does not know with the key, which is what puts
/// `settings.song_slide_headline` on the screen instead of a heading.
///
/// Only the tests ask this. The program itself never checks whether a text is
/// there — it shows what it gets, which is exactly why the checking has to
/// happen before it ever runs.
#[cfg(test)]
pub fn is_translated(key: &str) -> bool {
    rust_i18n::t!(key) != key
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    /// The clock widget gets something to show.
    ///
    /// Deliberately not an assertion about *which* time: the answer depends on
    /// the time zone of whatever machine runs the tests, and pinning that would
    /// be pinning the test machine rather than the behaviour. What has to hold
    /// everywhere is that both halves come back filled in and in the shape the
    /// patterns describe.
    #[test]
    fn a_moment_is_written_as_a_date_and_a_time() {
        let (date, time) = format_clock(crate::logic::timer::Timestamp::now());

        assert!(!date.is_empty(), "no date");
        assert!(!time.is_empty(), "no time");
        assert_eq!(
            time.len(),
            5,
            "the time should read as HH:MM, got {time:?}"
        );
        assert!(
            time.chars().nth(2) == Some(':'),
            "the time should read as HH:MM, got {time:?}"
        );
        assert!(
            date.chars().any(|character| character.is_ascii_digit()),
            "the date has no digits in it: {date:?}"
        );
    }

    /// A pattern out of a translation file is a text a person edits, and a
    /// widget on a platform monitor should show the time in the wrong order
    /// rather than nothing at all.
    #[test]
    fn an_unusable_date_pattern_falls_back_instead_of_failing() {
        let now = chrono::Local::now();

        // `%` with nothing after it is not a specifier.
        let written = strftime(&now, "%", "%d.%m.%Y");

        assert!(
            !written.is_empty(),
            "a pattern that cannot be used produced nothing at all"
        );
    }

    /// The two patterns are what the widget's shape depends on, so a language
    /// that forgets one would be found here rather than on a stage monitor.
    #[test]
    fn both_languages_state_how_they_write_a_date_and_a_time() {
        for language in ["en", "de"] {
            for key in ["general.date_format", "general.time_format"] {
                let pattern = rust_i18n::t!(key, locale = language);
                assert!(
                    pattern.contains('%'),
                    "{language} {key} is not a strftime pattern: {pattern:?}"
                );
            }
        }
    }

    /// The two languages every text is written in.
    const LANGUAGES: [&str; 2] = ["en", "de"];

    fn locales_folder() -> PathBuf {
        PathBuf::from("locales")
    }

    fn locale_files() -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(locales_folder())
            .expect("the locales folder is beside the sources")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("yml")
            })
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no locale files at all");
        files
    }

    /// Every key of one file, with the languages it was written in.
    ///
    /// A reader for the shape these files actually have — a block map two
    /// spaces at a time, values either on the line or in a `|-` block. It is
    /// deliberately strict: anything it cannot read is something the tests
    /// below would be quietly passing over.
    fn keys_of(path: &Path) -> BTreeMap<String, Vec<String>> {
        let text = std::fs::read_to_string(path).expect("a locale file can be read");
        let lines: Vec<&str> = text.lines().collect();
        let mut keys: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut path_so_far: Vec<String> = Vec::new();
        let mut index = 0;

        while index < lines.len() {
            let line = lines[index];
            index += 1;

            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let depth = line.len() - line.trim_start().len();
            assert_eq!(depth % 2, 0, "{}: odd indentation in {line:?}", path.display());
            let level = depth / 2;

            let (name, value) = match trimmed.split_once(':') {
                Some((name, value)) => (name.trim(), value.trim()),
                None => panic!("{}: {line:?} is neither a key nor a value", path.display()),
            };

            if name == "_version" {
                continue;
            }

            path_so_far.truncate(level);
            path_so_far.push(name.to_string());

            if value.is_empty() {
                // A section: its contents follow, indented.
                continue;
            }

            // A block scalar carries its text on the following, deeper lines.
            if value.starts_with('|') || value.starts_with('>') {
                while index < lines.len() {
                    let next = lines[index];
                    if next.trim().is_empty()
                        || next.len() - next.trim_start().len() > depth
                    {
                        index += 1;
                    } else {
                        break;
                    }
                }
            }

            // The leaf is a language; the key is everything above it.
            let language = path_so_far.pop().expect("a leaf has a name");
            let key = path_so_far.join(".");
            keys.entry(key).or_default().push(language);
        }

        keys
    }

    fn every_key() -> BTreeMap<String, Vec<(PathBuf, Vec<String>)>> {
        let mut all: BTreeMap<String, Vec<(PathBuf, Vec<String>)>> = BTreeMap::new();
        for file in locale_files() {
            for (key, languages) in keys_of(&file) {
                all.entry(key).or_default().push((file.clone(), languages));
            }
        }
        all
    }

    /// The version decides which of two quite different file formats rust-i18n
    /// reads. It has to be a whole number: `2.0` is not, and the file is then
    /// read as a version-1 file, in which everything means something else.
    #[test]
    fn every_file_declares_the_version_as_a_whole_number() {
        for file in locale_files() {
            let text = std::fs::read_to_string(&file).expect("readable");
            let version = text
                .lines()
                .find_map(|line| line.strip_prefix("_version:"))
                .unwrap_or_else(|| panic!("{} has no _version", file.display()))
                .trim();

            assert_eq!(
                version,
                "2",
                "{} declares version {version:?}; only a whole 2 is read as \
                 the format these files are written in",
                file.display()
            );
        }
    }

    /// Two files carrying the same key merge into one, and whichever is read
    /// last wins. Nothing says which — so the text somebody just edited may be
    /// the one that disappears.
    #[test]
    fn no_key_is_written_in_two_files() {
        let twice: Vec<String> = every_key()
            .into_iter()
            .filter(|(_, places)| places.len() > 1)
            .map(|(key, places)| {
                let names: Vec<String> = places
                    .iter()
                    .map(|(file, _)| file.display().to_string())
                    .collect();
                format!("{key} in {}", names.join(" and "))
            })
            .collect();

        assert!(twice.is_empty(), "the same key in two files:\n{}", twice.join("\n"));
    }

    /// A key that has only English falls back to English without a word, which
    /// is how a German window ends up with an English sentence in it.
    #[test]
    fn every_key_is_written_in_both_languages() {
        let mut incomplete: Vec<String> = Vec::new();
        for (key, places) in every_key() {
            for (file, languages) in places {
                for language in LANGUAGES {
                    if !languages.iter().any(|written| written == language) {
                        incomplete.push(format!(
                            "{key} has no {language} ({})",
                            file.display()
                        ));
                    }
                }
            }
        }

        assert!(incomplete.is_empty(), "{}", incomplete.join("\n"));
    }

    /// Every key the source asks for by name.
    ///
    /// The point of the whole module: a heading whose key was never written
    /// shows the key itself, and that has reached the screen more than once.
    fn keys_in_source() -> Vec<(PathBuf, String)> {
        // The word boundary matters: without it `format!("…")` and every other
        // macro ending in `t` is read as a translation.
        let pattern = regex::Regex::new(r#"\bt!\(\s*"([^"]+)""#).expect("a valid pattern");
        let mut found = Vec::new();

        fn walk(folder: &Path, files: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(folder).expect("the sources can be read") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    walk(&path, files);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    files.push(path);
                }
            }
        }

        let mut files = Vec::new();
        walk(Path::new("src"), &mut files);

        for file in files {
            let text = std::fs::read_to_string(&file).expect("a source file can be read");
            // A comment explaining a `t!("…")` is not one.
            let code: String = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<&str>>()
                .join("\n");
            for capture in pattern.captures_iter(&code) {
                found.push((file.clone(), capture[1].to_string()));
            }
        }

        assert!(found.len() > 100, "only {} keys found — the scan is broken", found.len());
        found
    }

    #[test]
    fn every_key_the_program_asks_for_exists() {
        let mut missing: Vec<String> = keys_in_source()
            .into_iter()
            .filter(|(_, key)| !is_translated(key))
            .map(|(file, key)| format!("{key} ({})", file.display()))
            .collect();
        missing.sort();
        missing.dedup();

        assert!(
            missing.is_empty(),
            "keys the program asks for and no file answers:\n{}",
            missing.join("\n")
        );
    }

    /// The keys that are *returned* rather than written into a `t!`.
    ///
    /// The scan above cannot see these: the key is the result of a function
    /// and only meets `t!` at the other end. They are the labels of everything
    /// the export and import dialogs offer, so a missing one is a menu entry
    /// with a key in it.
    #[test]
    fn every_key_a_function_hands_out_exists() {
        use crate::logic::export::{ExportCategory, ExportFormat};
        use crate::logic::selection_io::{SelectionFormat, SelectionIoError};
        use crate::logic::tag_mapping::TagMapping;

        let mut keys: Vec<String> = Vec::new();

        // Why a tag mapping rule cannot be used. The settings page shows
        // whatever this returns, so an unwritten message would appear as a key
        // under the row the user is typing in.
        for rule in [
            TagMapping::new("", ""),
            TagMapping::new("author", "author"),
        ] {
            keys.push(
                rule.problem()
                    .expect("these rules are not usable")
                    .to_string(),
            );
        }

        keys.extend(ExportFormat::ALL.iter().map(|format| format.label_key().to_string()));
        for category in ExportCategory::ALL {
            keys.push(category.label_key().to_string());
            keys.push(category.description_key().to_string());
        }
        keys.extend(SelectionFormat::ALL.iter().map(|format| format.label_key().to_string()));

        // What the editor offers to create.
        #[cfg(not(target_arch = "wasm32"))]
        keys.extend(
            crate::logic::repository_files::NewFileKind::ALL
                .iter()
                .map(|kind| kind.label_key().to_string()),
        );

        for error in [
            SelectionIoError::Empty,
            SelectionIoError::Unreadable {
                name: String::new(),
                reason: String::new(),
            },
            SelectionIoError::Malformed(String::new()),
            SelectionIoError::TooNew {
                found: 2,
                supported: 1,
            },
            SelectionIoError::Archive(String::new()),
        ] {
            keys.push(error.message_key().0.to_string());
        }

        let missing: Vec<&String> = keys.iter().filter(|key| !is_translated(key)).collect();
        assert!(missing.is_empty(), "{missing:?}");
    }
}
