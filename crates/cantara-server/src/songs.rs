//! What the server reads out of a text file, and the one thing it writes into
//! one.
//!
//! A song's title and tags are kept in the database beside every version, so
//! that a library can be listed and searched without reading every file — and
//! because they are read from the content of each version, a song whose title
//! changed in the text has the new title in the list at once.
//!
//! The one change the server makes to content: a song without a `uuid` tag is
//! given one, written into the file, so that the identity it has on the server
//! travels with it into an export and back (Q6 of
//! `docs/specs/0008-server-version.md`). Where the format cannot carry a tag
//! (CCLI) the file stays as it is and the identity lives on the server only.

use std::collections::BTreeMap;

use cantara_api::ItemKind;
use cantara_songlib::importer::{ccli, classic_song, song_yml};
use cantara_songlib::song::Song;
use uuid::Uuid;

/// What is read out of a file's content.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Metadata {
    pub title: String,
    pub tags: BTreeMap<String, String>,
    /// The `uuid` tag, if the song has a valid one.
    pub uuid: Option<Uuid>,
}

fn parse_song(file_name: &str, content: &str) -> Option<Song> {
    let lower = file_name.to_lowercase();
    let parsed = if lower.ends_with(".song.yml") || lower.ends_with(".song.yaml") {
        song_yml::import_from_yml_string(content)
    } else if lower.ends_with(".ccli") {
        ccli::import_from_ccli_string(content)
    } else if lower.ends_with(".song") {
        classic_song::import_song(content)
    } else {
        return None;
    };
    parsed.ok()
}

/// The name a file is listed under when its content does not name itself:
/// the file name without what makes it a Cantara file.
pub fn display_name(file_name: &str) -> String {
    let lower = file_name.to_lowercase();
    for suffix in [".song.yml", ".song.yaml", ".song", ".ccli", ".md", ".pdf", ".png", ".jpg", ".jpeg", ".mp4", ".m4v", ".webm", ".ogv", ".mov"] {
        if lower.ends_with(suffix) {
            return file_name[..file_name.len() - suffix.len()].to_string();
        }
    }
    file_name.to_string()
}

/// What the content of `file_name` says about itself.
pub fn metadata(file_name: &str, kind: ItemKind, content: &[u8]) -> Metadata {
    let fallback = Metadata {
        title: display_name(file_name),
        ..Metadata::default()
    };
    let Ok(text) = std::str::from_utf8(content) else {
        return fallback;
    };

    match kind {
        ItemKind::Song => {
            let Some(song) = parse_song(file_name, text) else {
                return fallback;
            };
            let tags: BTreeMap<String, String> = song
                .tags()
                .iter()
                .map(|(key, value)| (key.to_lowercase(), value.trim().to_string()))
                .collect();
            let uuid = tags.get("uuid").and_then(|value| Uuid::parse_str(value).ok());
            let title = if song.title.trim().is_empty() {
                fallback.title
            } else {
                song.title.trim().to_string()
            };
            Metadata { title, tags, uuid }
        }
        ItemKind::Markdown => {
            let heading = text
                .lines()
                .map(str::trim)
                .find(|line| line.starts_with("# "))
                .map(|line| line.trim_start_matches('#').trim().to_string());
            Metadata {
                title: heading.unwrap_or(fallback.title),
                ..Metadata::default()
            }
        }
        _ => fallback,
    }
}

/// `content` with a `uuid` tag naming `lineage`, if the format can carry one.
///
/// Returns `None` where it cannot, or where writing it would not survive being
/// read back — in that case the file is stored as it came, which costs
/// nothing but the identity travelling with the file.
pub fn with_uuid_tag(file_name: &str, content: &str, lineage: Uuid) -> Option<String> {
    let lower = file_name.to_lowercase();
    let tagged = if lower.ends_with(".song.yml") || lower.ends_with(".song.yaml") {
        yaml_with_tag(content, lineage)?
    } else if lower.ends_with(".song") {
        classic_with_tag(content, lineage)
    } else {
        return None;
    };

    // Whatever was written has to read back as this very tag, and the song
    // has to read as it did before.
    let read_back = metadata(file_name, ItemKind::Song, tagged.as_bytes());
    let before = metadata(file_name, ItemKind::Song, content.as_bytes());
    (read_back.uuid == Some(lineage) && read_back.title == before.title).then_some(tagged)
}

/// A classic song keeps its tags in a block of `#tag: value` lines. If the
/// file opens with such a block the tag joins it; otherwise it gets one.
fn classic_with_tag(content: &str, lineage: Uuid) -> String {
    let opens_with_tags = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.trim_start().starts_with('#'));
    let leading = content.len() - content.trim_start().len();
    let rest = &content[leading..];
    if opens_with_tags {
        format!("#uuid: {lineage}\n{rest}")
    } else {
        format!("#uuid: {lineage}\n\n{rest}")
    }
}

/// A YAML song keeps its tags in a `tags:` mapping. The tag is put on the line
/// after `tags:` with the indentation of the line that follows; a file without
/// the mapping gets one at its end.
fn yaml_with_tag(content: &str, lineage: Uuid) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let Some(position) = lines.iter().position(|line| line.trim_end() == "tags:") else {
        if lines.iter().any(|line| line.starts_with("tags:")) {
            // `tags: {…}` written inline. Rewriting flow style by hand is not
            // worth the risk.
            return None;
        }
        let separator = if content.ends_with('\n') { "" } else { "\n" };
        return Some(format!("{content}{separator}tags:\n  uuid: {lineage}\n"));
    };

    let indent = lines
        .get(position + 1)
        .map(|line| &line[..line.len() - line.trim_start().len()])
        .filter(|indent| !indent.is_empty())
        .unwrap_or("  ");

    let mut out: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    out.insert(position + 1, format!("{indent}uuid: {lineage}"));
    let mut text = out.join("\n");
    if content.ends_with('\n') {
        text.push('\n');
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lineage() -> Uuid {
        Uuid::parse_str("0192f1e2-7c1a-7b3e-9d2a-3f4e5a6b7c8d").unwrap()
    }

    #[test]
    fn a_classic_song_gains_its_tag_in_its_own_tag_block() {
        let song = "#title: Amazing Grace\n#author: John Newton\n\nAmazing grace, how sweet the sound\n";
        let tagged = with_uuid_tag("Amazing Grace.song", song, lineage()).unwrap();
        assert!(tagged.starts_with("#uuid: 0192f1e2"));
        let meta = metadata("Amazing Grace.song", ItemKind::Song, tagged.as_bytes());
        assert_eq!(meta.uuid, Some(lineage()));
        assert_eq!(meta.title, "Amazing Grace");
        assert_eq!(meta.tags.get("author").map(String::as_str), Some("John Newton"));
    }

    #[test]
    fn a_classic_song_without_tags_gets_a_block_of_its_own() {
        let song = "Alas! and did my Savior bleed,\nand did my Sovereign die!\n";
        let tagged = with_uuid_tag("Alas.song", song, lineage()).unwrap();
        assert!(tagged.starts_with(&format!("#uuid: {}\n\nAlas!", lineage())));
        assert_eq!(metadata("Alas.song", ItemKind::Song, tagged.as_bytes()).uuid, Some(lineage()));
    }

    #[test]
    fn a_yaml_song_gains_the_tag_in_its_mapping() {
        let song = "version: 0.1\ntitle: Amazing Grace\ndefault_language: en\ntags:\n  author: John Newton\nparts: []\n";
        let tagged = with_uuid_tag("Amazing Grace.song.yml", song, lineage()).unwrap();
        assert!(tagged.contains("tags:\n  uuid: 0192f1e2"));
        let meta = metadata("Amazing Grace.song.yml", ItemKind::Song, tagged.as_bytes());
        assert_eq!(meta.uuid, Some(lineage()));
        assert_eq!(meta.tags.get("author").map(String::as_str), Some("John Newton"));
    }

    #[test]
    fn a_ccli_song_is_left_as_it_is() {
        assert_eq!(with_uuid_tag("Lied.ccli", "Lied\n\nVers 1\nText\n", lineage()), None);
    }

    #[test]
    fn markdown_is_titled_by_its_first_heading() {
        let meta = metadata("notes.md", ItemKind::Markdown, b"Intro\n# Abkuendigungen\ntext");
        assert_eq!(meta.title, "Abkuendigungen");
        assert_eq!(metadata("Bild.png", ItemKind::Image, b"\x89PNG").title, "Bild");
    }

    #[test]
    fn the_real_test_files_take_the_tag() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testfiles");
        for name in ["Amazing Grace.song", "Amazing Grace.song.yml", "Alas, and Did My Savior Bleed.song"] {
            let content = std::fs::read_to_string(root.join(name)).unwrap();
            let tagged = with_uuid_tag(name, &content, lineage());
            assert!(tagged.is_some(), "{name} did not take a uuid tag");
        }
    }
}
