//! The identifier an element of the library carries in the URL.
//!
//! The detail view is addressable: `/detail/<element-id>` opens one particular
//! song, picture or document. For that to be worth anything the identifier has
//! to survive a restart of the app, and it should ideally be the same on the
//! desktop and in the browser, so a link can be passed on.
//!
//! Two kinds of identifier do that:
//!
//! 1. The `uuid` tag of a song, if it carries one. This is the better one — it
//!    belongs to the song itself and stays with it even when the file is
//!    renamed or moved into another repository.
//! 2. Otherwise a hash of the file's path *within its repository*
//!    ([`SourceFile::relative_path`]). The full path is unusable: a downloaded
//!    repository lands in a fresh temporary directory on every start, and the
//!    web build addresses the same file through a VFS scheme.
//!
//! Nothing here is authoritative — an identifier that resolves to nothing is
//! not an error. [`resolve`] returns `None` and the detail view simply opens
//! without an element, which is what a link to a song that has since been
//! removed should do.

use crate::logic::sourcefiles::{read_source_file, SourceFile, SourceFileType};

/// How many hex characters of the fingerprint an identifier normally shows.
///
/// Short enough to stay readable in a URL. Should two files of one library
/// share a prefix of this length, [`of`] falls back to the full fingerprint for
/// them, and [`resolve`] matches by prefix, so both forms lead to the file.
const SHORT_ID_LEN: usize = 8;

/// The full fingerprint of a file: the MD5 of its position in the repository.
fn fingerprint(file: &SourceFile) -> String {
    // A file that was not read from a repository has no position within one;
    // its full path is the best that is available, which still holds for as
    // long as the file is not moved.
    let basis = file
        .relative_path
        .clone()
        .unwrap_or_else(|| file.path.to_string_lossy().into_owned());

    format!("{:x}", md5::compute(basis.as_bytes()))
}

/// The `uuid` tag of a song, if it has one.
///
/// The tag is looked up without regard to case, since the song formats leave
/// the spelling of a tag to whoever wrote the file.
fn uuid_tag(file: &SourceFile) -> Option<String> {
    if file.file_type != SourceFileType::Song {
        return None;
    }

    let content = read_source_file(file).ok()?;
    let file_name = file
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&file.name);
    let song = crate::logic::export::song_from_content(file_name, &content).ok()?;

    song.tags()
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("uuid"))
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The identifier of `file` within `library`.
///
/// Reads the file when it is a song, to see whether it carries a `uuid` tag —
/// which is why this takes one file rather than building an index of the whole
/// library: it is called when an element is opened, once.
pub fn of(file: &SourceFile, library: &[SourceFile]) -> String {
    if let Some(uuid) = uuid_tag(file) {
        return uuid;
    }

    let full = fingerprint(file);
    let short = &full[..SHORT_ID_LEN.min(full.len())];

    // Two files of the same library that agree in the first characters would
    // make the short form ambiguous, so those two (and only those) get the
    // long one.
    let ambiguous = library
        .iter()
        .filter(|other| other.path != file.path)
        .any(|other| fingerprint(other).starts_with(short));

    if ambiguous { full } else { short.to_string() }
}

/// The element `id` refers to, as an index into `library`.
///
/// Returns `None` for anything that does not name a file of this library; the
/// caller is expected to carry on without an element rather than to report an
/// error.
pub fn resolve(library: &[SourceFile], id: &str) -> Option<usize> {
    let id = id.trim();
    if id.is_empty() {
        return None;
    }

    // A fingerprint is hex, so anything else — a UUID with its dashes, above
    // all — cannot be one, and looking for a prefix would be wasted work.
    if id.len() <= 32 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        let wanted = id.to_ascii_lowercase();
        if let Some(index) = library
            .iter()
            .position(|file| fingerprint(file).starts_with(&wanted))
        {
            return Some(index);
        }
    }

    // Only now, and only for songs, is it worth reading files: this parses
    // until the tag matches, which for a link that was copied out of the app
    // stops at the song it came from.
    library.iter().position(|file| {
        uuid_tag(file).is_some_and(|uuid| uuid.eq_ignore_ascii_case(id))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(relative: Option<&str>, path: &str) -> SourceFile {
        SourceFile {
            name: "A Song".to_string(),
            path: PathBuf::from(path),
            file_type: SourceFileType::Song,
            md5_hash: None,
            relative_path: relative.map(str::to_string),
        }
    }

    /// The point of the whole module: the same file in the same repository has
    /// the same identifier, however it was unpacked.
    #[test]
    fn identifier_survives_a_move_of_the_repository() {
        let desktop = file(Some("Lieder/Amazing Grace.song"), "/tmp/.tmpXY/Lieder/Amazing Grace.song");
        let web = file(
            Some("Lieder/Amazing Grace.song"),
            "web-github://reckel-jm/cantara-songrepo/Lieder/Amazing Grace.song",
        );

        assert_eq!(fingerprint(&desktop), fingerprint(&web));
    }

    /// Files without a repository position still get an identifier, just one
    /// that is tied to where they lie.
    #[test]
    fn a_file_outside_a_repository_falls_back_to_its_path() {
        let loose = file(None, "/home/user/Bilder/cover.png");

        assert_eq!(fingerprint(&loose).len(), 32);
    }

    #[test]
    fn an_identifier_leads_back_to_its_file() {
        let library = vec![
            file(Some("a.song"), "/repo/a.song"),
            file(Some("b.song"), "/repo/b.song"),
        ];

        for (index, entry) in library.iter().enumerate() {
            let id = of(entry, &library);
            assert_eq!(resolve(&library, &id), Some(index), "for {id}");
        }
    }

    /// The identifier is normally the short form, and the long one still
    /// resolves — so a link keeps working even if the app later has to lengthen
    /// the identifier because another file was added.
    #[test]
    fn the_long_form_resolves_as_well() {
        let library = vec![file(Some("a.song"), "/repo/a.song")];

        assert_eq!(of(&library[0], &library).len(), SHORT_ID_LEN);
        assert_eq!(resolve(&library, &fingerprint(&library[0])), Some(0));
    }

    #[test]
    fn an_unknown_identifier_resolves_to_nothing() {
        let library = vec![file(Some("a.song"), "/repo/a.song")];

        assert_eq!(resolve(&library, "deadbeef"), None);
        assert_eq!(resolve(&library, "not-an-id"), None);
        assert_eq!(resolve(&library, ""), None);
    }
}
