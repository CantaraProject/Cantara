//! This module provides functionality for handling available source files (for creating output) in Cantara.


// Reading files from disk, and the paths to do it with, are a desktop matter:
// the web build keeps its repositories in an in-memory VFS.
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
/// The maximal depth for recursive file searching. Implemented as a constant to prevent loops.
const MAX_DEPTH: usize = 6;

#[cfg(not(target_arch = "wasm32"))]
/// Recursively collects every file below `dir` whose name `accept` says yes to,
/// up to a recursion depth of the constant [MAX_DEPTH].
///
/// # Arguments
/// * `dir` - The starting directory path.
/// * `accept` - Decides, from the file name alone, whether a file is wanted.
/// * `depth` - The current recursion depth (starts at 0).
///
/// # Returns
/// A vector of `PathBuf`s containing the full paths of matching files.
///
/// # Note
/// The kind of an entry is taken from the directory listing rather than asked
/// for per entry. Both `Path::is_dir` and `Path::is_file` open the file to
/// answer, which on Windows costs more than the whole rest of the scan on a
/// library of a few thousand files. A symbolic link is the one case the
/// listing cannot answer, and only there is the target looked at — which also
/// keeps linked directories being followed, as they always were.
fn find_files_recursive(dir: &Path, accept: &dyn Fn(&str) -> bool, depth: usize) -> Vec<PathBuf> {
    let mut result = Vec::new();

    // Stop recursion beyond depth 6
    if depth > MAX_DEPTH {
        return result;
    }

    // A directory that cannot be read is skipped: a library may well contain
    // one the user has no access to, and that is no reason to abandon the scan.
    let Ok(entries) = fs::read_dir(dir) else {
        return result;
    };

    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();

        let (is_dir, is_file) = if kind.is_symlink() {
            (path.is_dir(), path.is_file())
        } else {
            (kind.is_dir(), kind.is_file())
        };

        if is_dir {
            result.extend(find_files_recursive(&path, accept, depth + 1));
            continue;
        }

        if !is_file {
            continue;
        }

        let wanted = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(accept);

        if wanted {
            result.push(path);
        }
    }

    result
}

#[cfg(not(target_arch = "wasm32"))]
/// Finds all files in a directory and its subdirectories (up to 6 levels deep)
/// whose filenames end with the given suffix.
///
/// # Arguments
/// * `dir` - The starting directory path.
/// * `endings` - A vector with the suffixes to match (e.g., `vec![".txt"]`).
///
/// # Returns
/// A vector of `PathBuf`s containing the full paths of matching files.
///
/// # Notes
/// - Returns an empty vector if the directory does not exist or is not a directory.
/// - The `ending` should include the dot if matching extensions (e.g., ".txt").
/// - Matching is case-sensitive.
/// - Symlinks are followed (default behavior of `is_file` and `is_dir`).
#[cfg(test)]
fn find_files_with_ending(dir: &Path, endings: Vec<&'static str>) -> Vec<PathBuf> {
    find_matching_files(dir, &|name| {
        endings.iter().any(|ending| name.ends_with(ending))
    })
}

#[cfg(not(target_arch = "wasm32"))]
/// Every file below `dir` whose name `accept` says yes to.
///
/// Returns an empty vector if the directory does not exist or is not a
/// directory.
fn find_matching_files(dir: &Path, accept: &dyn Fn(&str) -> bool) -> Vec<PathBuf> {
    // Check if the directory exists and is a directory
    if !dir.is_dir() {
        return Vec::new();
    }

    // Start recursive traversal at depth 0.
    find_files_recursive(dir, accept, 0)
}

#[cfg(not(target_arch = "wasm32"))]
/// Every file below `dir` that Cantara can read, each listed once and in a
/// stable order.
///
/// Matching goes through [`SourceFileType::of`], so this cannot fall behind the
/// list of supported formats — and it inherits that function's indifference to
/// case, which a plain suffix comparison did not: a `LIED.CCLI` from a Windows
/// share was recognised everywhere else in the program but never found by the
/// scan.
fn find_supported_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = find_matching_files(dir, &|name| SourceFileType::of(name).is_some());
    // The order a directory is listed in is the file system's business, not
    // something the song list should inherit.
    files.sort();
    files
}

/// This enum declares the generic types which a source file can by.
/// One type can be represented by several different file formats.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceFileType {
    /// A song to sing
    Song,

    /// A presentation which can be displayed, but its content structure is not familiar to Cantara
    Presentation,

    /// An image/picture which Cantara can display
    Image,

    /// A video which Cantara can display
    Video,

    /// A PDF document which Cantara can display
    Pdf,

    /// A Markdown document which Cantara can render and display
    Markdown,
}

impl SourceFileType {
    /// Every file name suffix Cantara reads, together with the kind of content
    /// it holds.
    ///
    /// This is the single place that decides what a file is; the directory
    /// scan, drag & drop and the web VFS all go through it, so they cannot
    /// drift apart. Longer suffixes come first, so `.song.yml` is recognised
    /// as a YAML song rather than as a classic `.song` file.
    const SUFFIXES: &'static [(&'static str, SourceFileType)] = &[
        (".song.yml", SourceFileType::Song),
        (".song.yaml", SourceFileType::Song),
        (".song", SourceFileType::Song),
        (".ccli", SourceFileType::Song),
        (".png", SourceFileType::Image),
        (".jpg", SourceFileType::Image),
        (".jpeg", SourceFileType::Image),
        (".pdf", SourceFileType::Pdf),
        (".md", SourceFileType::Markdown),
        // Only the containers a web view can play by itself. Cantara hands a
        // video to the engine the rest of the program is already drawn by
        // rather than decoding one of its own, so what is offered here is what
        // that engine can open: MP4 and WebM everywhere, Ogg on Gecko and
        // Blink, QuickTime on WebKit.
        //
        // Deliberately absent: `.mkv`, `.avi`, `.wmv` and `.flv`. A browser
        // engine will not play them, and a file that is listed but shows a
        // black rectangle when the service reaches it is worse than one that
        // was never offered.
        (".mp4", SourceFileType::Video),
        (".m4v", SourceFileType::Video),
        (".webm", SourceFileType::Video),
        (".ogv", SourceFileType::Video),
        (".mov", SourceFileType::Video),
    ];

    /// What kind of content a file name promises, or `None` when Cantara does
    /// not read that format.
    ///
    /// Matching ignores case, so `LIED.CCLI` is found just like `lied.ccli`.
    ///
    /// ```
    /// # use cantara::logic::sourcefiles::SourceFileType;
    /// assert_eq!(SourceFileType::of("Amazing Grace.song"), Some(SourceFileType::Song));
    /// assert_eq!(SourceFileType::of("Amazing Grace.song.yml"), Some(SourceFileType::Song));
    /// assert_eq!(SourceFileType::of("Lied.CCLI"), Some(SourceFileType::Song));
    /// assert_eq!(SourceFileType::of("config.yml"), None);
    /// ```
    pub fn of(file_name: &str) -> Option<SourceFileType> {
        let lower = file_name.to_lowercase();
        SourceFileType::SUFFIXES
            .iter()
            .find(|(suffix, _)| lower.ends_with(suffix))
            .map(|(_, file_type)| *file_type)
    }

    /// The name to show for a file: its name without the suffix Cantara
    /// matched.
    ///
    /// `Path::file_stem` would leave `Amazing Grace.song` for a
    /// `Amazing Grace.song.yml` file, which is not what a user expects to read
    /// in the song list.
    pub fn display_name(file_name: &str) -> String {
        let lower = file_name.to_lowercase();
        for (suffix, _) in SourceFileType::SUFFIXES {
            if lower.ends_with(suffix) {
                return file_name[..file_name.len() - suffix.len()].to_string();
            }
        }
        file_name.to_string()
    }
}

/// A source file which contains content which Cantara can use to generate content from
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceFile {
    /// The name of the source file. This is most likely the latest part of its file path without
    /// the file ending, but it does not have to be the case. The name will be used to display the file
    /// in the selection context.
    pub name: String,

    /// The file path where Cantara can access the file, this does not necessarily be its origin.
    /// For example, remote repositories might be accessible at any http/https URL, but in this case the file path
    /// would be the temporary folder where Cantara has downloaded the repository content.
    pub path: PathBuf,

    /// The file type of the file, indicating its content.
    pub file_type: SourceFileType,

    /// The MD5 hash of the file contents, computed when the file is read from the repository.
    /// Used for file identity checks. `None` if the hash has not been computed yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5_hash: Option<String>,

    /// Where the file sits *inside* its repository, with `/` as separator and
    /// without a leading one — `Lieder/Amazing Grace.song`.
    ///
    /// [`path`](Self::path) cannot answer this: a downloaded repository is
    /// unpacked into a new temporary directory on every start, and the web
    /// build addresses its files through a VFS scheme instead of a file system
    /// at all. The position within the repository is the one part that stays
    /// the same, which is why the identifier the detail view puts into the URL
    /// is derived from it — see [`crate::logic::element_id`].
    ///
    /// `None` for files that were not read from a repository (a picture the
    /// user picked by hand, or an older settings file that predates this
    /// field); those fall back to the full path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
/// This function will get all source files in a given directory which can be imported and used by Cantara
///
/// # Parameters
/// - `start_dir`: The borrowed [Path] reference where the recursive search for source files starts
///
/// # Returns
/// A vector of [SourceFile]s which contains all results, each file listed once.
/// If no file was found, an empty vector is returned.
///
/// # Hint
/// To prevent infinitive recursion (e.g. if there are symbolic links causing a loop) the maximum depth for recursive search is determined by [MAX_DEPTH].
pub fn get_source_files(start_dir: &Path) -> Vec<SourceFile> {
    let paths = find_supported_files(start_dir);

    // Fingerprinting is what makes a scan expensive — every file is read from
    // beginning to end — and each file's fingerprint is independent of the
    // others, so they are computed a handful at a time. Everything else here
    // is string work on names that are already in memory.
    let hashes = crate::logic::parallel::map_parallel(&paths, |path| fingerprint(path));

    paths
        .into_iter()
        .zip(hashes)
        .filter_map(|(file, md5_hash)| {
            let file_name = file.file_name()?.to_str()?;
            let file_type = SourceFileType::of(file_name)?;

            Some(SourceFile {
                name: SourceFileType::display_name(file_name),
                path: file.clone(),
                file_type,
                md5_hash,
                relative_path: relative_path(start_dir, &file),
            })
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
/// How many files below `start_dir` Cantara can read.
///
/// Nothing but the names is looked at, which is what the settings page needs
/// to say how large a repository is. Going through [`get_source_files`] for
/// that read and hashed every file in the library — including videos and
/// scanned PDFs — every time the page was drawn.
pub fn count_source_files(start_dir: &Path) -> usize {
    find_supported_files(start_dir).len()
}

#[cfg(not(target_arch = "wasm32"))]
/// The MD5 hash of a file's contents, which is how the program tells one
/// version of a file from another — see [`SourceFile::md5_hash`].
///
/// Read in blocks rather than in one piece: a library may hold a video or a
/// scanned score of a few hundred megabytes, and there is no reason for any of
/// it to be in memory at once when all that comes out is a hash.
fn fingerprint(path: &Path) -> Option<String> {
    use std::io::Read;

    /// Big enough that the read syscalls disappear next to the disk itself,
    /// small enough to stay in cache while it is being hashed.
    const BLOCK: usize = 64 * 1024;

    let mut file = fs::File::open(path).ok()?;
    let mut context = md5::Context::new();
    let mut block = vec![0u8; BLOCK];
    loop {
        match file.read(&mut block) {
            Ok(0) => break,
            Ok(read) => context.consume(&block[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
    Some(format!("{:x}", context.finalize()))
}

#[cfg(not(target_arch = "wasm32"))]
/// The position of `file` below `root`, in the form the URL identifier is built
/// from: `/` as separator, whatever the platform's own separator is.
fn relative_path(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let segments: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(segments.join("/"))
}

/// This is a wrapper around [SourceFile] which ensures that the [SourceFile] is an image
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageSourceFile(SourceFile);

impl ImageSourceFile {
    // Constructor that enforces the FileType::Image constraint
    pub fn new(source_file: SourceFile) -> Option<Self> {
        if matches!(source_file.file_type, SourceFileType::Image) {
            Some(ImageSourceFile(source_file))
        } else {
            None
        }
    }

    // Accessor to get the inner SourceFile
    pub fn into_inner(self) -> SourceFile {
        self.0
    }

    // Optional: Reference accessor for convenience
    pub fn as_source(&self) -> &SourceFile {
        &self.0
    }
}

/// This is a wrapper around [SourceFile] which ensures that the [SourceFile] is a video
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VideoSourceFile(SourceFile);

impl VideoSourceFile {
    /// Constructor that enforces the [`SourceFileType::Video`] constraint.
    pub fn new(source_file: SourceFile) -> Option<Self> {
        if matches!(source_file.file_type, SourceFileType::Video) {
            Some(VideoSourceFile(source_file))
        } else {
            None
        }
    }

    /// Accessor to get the inner [SourceFile].
    pub fn into_inner(self) -> SourceFile {
        self.0
    }

    /// Reference accessor for convenience.
    pub fn as_source(&self) -> &SourceFile {
        &self.0
    }

    /// What to tell a web view this file holds, for the `type` of a `<source>`.
    ///
    /// A guess from the suffix, which is all there is without opening the file.
    /// An engine that disagrees falls back to sniffing the content, so being
    /// wrong here costs nothing; saying nothing at all is what makes some
    /// engines refuse to try.
    pub fn mime_type(&self) -> &'static str {
        mime_type_of_video(self.0.file_name())
    }
}

/// The MIME type a video file name promises.
///
/// Free-standing as well as on [`VideoSourceFile`], because the thing that
/// serves the file to a browser has a path and not a source file.
pub fn mime_type_of_video(file_name: &str) -> &'static str {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".webm") {
        "video/webm"
    } else if lower.ends_with(".ogv") {
        "video/ogg"
    } else if lower.ends_with(".mov") {
        "video/quicktime"
    } else {
        // `.mp4` and `.m4v`, and the safest guess for anything else that got
        // this far.
        "video/mp4"
    }
}

impl SourceFile {
    /// The name of the file itself — the thing that decides its format.
    ///
    /// [`name`](Self::name) is the *display* name, with the suffix stripped
    /// for the list, so handing that to an importer makes every song look like
    /// an unknown format. Falls back to the display name for an element that
    /// was never a file, such as a piece of Markdown typed into the app.
    pub fn file_name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.name)
    }

    /// Creates a [SourceFile] from a web VFS path (e.g., `web-zip://url/path/to/file.song`).
    /// Only available on WASM targets.
    ///
    /// `repository_prefix` is the part of the path that addresses the
    /// repository itself (`web-github://owner/repo`); what follows it is the
    /// file's [`relative_path`](Self::relative_path). The prefix has to be
    /// passed in because it cannot be recovered from the path alone — a
    /// `web-zip://` prefix ends with a URL, which contains slashes of its own.
    #[cfg(target_arch = "wasm32")]
    pub fn from_web_path(vfs_path: &str, repository_prefix: &str) -> Option<Self> {
        // The file name is the last component of the path
        let file_name = vfs_path.split('/').next_back()?;
        let file_type = SourceFileType::of(file_name)?;

        Some(SourceFile {
            name: SourceFileType::display_name(file_name),
            path: PathBuf::from(vfs_path),
            file_type,
            md5_hash: None,
            relative_path: vfs_path
                .strip_prefix(repository_prefix)
                .map(|relative| relative.trim_start_matches('/').to_string()),
        })
    }
}

/// Reads a source file as text, wherever the build keeps it.
///
/// The desktop reads from the file system; the web build has none and reads
/// from the in-memory VFS its repositories were unpacked into.
/// The bytes of a source file, wherever the build keeps them.
///
/// What [`read_source_file`] is for text, this is for everything: a picture or
/// a PDF put into a selection file is not text and must not be read as if it
/// were.
pub fn read_source_file_bytes(file: &SourceFile) -> Result<Vec<u8>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::logic::settings::RepositoryType;

        let path = file.path.to_string_lossy().to_string();
        RepositoryType::web_read_file(&path).ok_or_else(|| "not found in the web storage".to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        fs::read(&file.path).map_err(|error| error.to_string())
    }
}

pub fn read_source_file(file: &SourceFile) -> Result<String, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::logic::settings::RepositoryType;

        let path = file.path.to_string_lossy().to_string();
        let bytes = RepositoryType::web_read_file(&path)
            .ok_or_else(|| "not found in the web storage".to_string())?;
        String::from_utf8(bytes).map_err(|error| error.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        fs::read_to_string(&file.path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::path::Path;

    /// Every format Cantara advertises has to be recognised, and nothing else.
    #[test]
    fn test_file_type_of_name() {
        let cases = [
            ("Amazing Grace.song", Some(SourceFileType::Song)),
            ("Amazing Grace.song.yml", Some(SourceFileType::Song)),
            ("Amazing Grace.song.yaml", Some(SourceFileType::Song)),
            ("Weiß ich den Weg auch nicht.ccli", Some(SourceFileType::Song)),
            ("cover.png", Some(SourceFileType::Image)),
            ("cover.JPG", Some(SourceFileType::Image)),
            ("handout.pdf", Some(SourceFileType::Pdf)),
            ("notes.md", Some(SourceFileType::Markdown)),
            // Case is ignored, so a file from a Windows share is still found.
            ("LIED.CCLI", Some(SourceFileType::Song)),
            ("Amazing Grace.SONG", Some(SourceFileType::Song)),
            // A plain YAML file is configuration, not a song.
            ("config.yml", None),
            ("song.yml", None),
            ("readme.txt", None),
            ("no-extension", None),
        ];

        for (name, expected) in cases {
            assert_eq!(SourceFileType::of(name), expected, "for {name}");
        }
    }

    /// The name in the song list must not keep half of a double extension.
    #[test]
    fn test_display_name_strips_the_whole_suffix() {
        assert_eq!(
            SourceFileType::display_name("Amazing Grace.song.yml"),
            "Amazing Grace"
        );
        assert_eq!(
            SourceFileType::display_name("Amazing Grace.song"),
            "Amazing Grace"
        );
        assert_eq!(SourceFileType::display_name("Lied.CCLI"), "Lied");
        assert_eq!(SourceFileType::display_name("no-extension"), "no-extension");
    }

    /// A repository holding every format must list each file exactly once.
    #[test]
    fn test_scan_finds_every_format_once() {
        let files = get_source_files(Path::new("testfiles"));

        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        for expected in [
            "Amazing Grace",
            "Weiß ich den Weg auch nicht",
            "Sei nicht stolz auf das, was du bist",
        ] {
            assert!(
                names.contains(&expected),
                "'{expected}' missing from {names:?}"
            );
        }

        // "Amazing Grace" exists as .song and as .song.yml — two files, two
        // entries, but neither of them listed twice.
        let paths: Vec<&std::path::Path> = files.iter().map(|file| file.path.as_path()).collect();
        let mut unique = paths.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(paths.len(), unique.len(), "a file was listed twice");

        let songs = files
            .iter()
            .filter(|file| file.file_type == SourceFileType::Song)
            .count();
        assert!(songs >= 4, "expected the song files, found {songs}");
    }

    /// [`SourceFileType::of`] ignores case, and the scan has to agree with it:
    /// a `LIED.CCLI` off a Windows share was recognised as a song everywhere
    /// in the program except by the one thing that had to find it first.
    #[test]
    fn the_scan_finds_a_file_whose_suffix_is_written_in_capitals() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        fs::write(dir.path().join("LIED.CCLI"), "").expect("the file can be written");
        fs::write(dir.path().join("Handout.PDF"), "").expect("the file can be written");

        let names: Vec<String> = get_source_files(dir.path())
            .into_iter()
            .map(|file| file.name)
            .collect();

        assert!(names.contains(&"LIED".to_string()), "found {names:?}");
        assert!(names.contains(&"Handout".to_string()), "found {names:?}");
    }

    /// The count the settings page shows is the number of files the scan
    /// would return — it just does not read any of them.
    #[test]
    fn the_count_agrees_with_the_scan() {
        let dir = Path::new("testfiles");

        assert_eq!(count_source_files(dir), get_source_files(dir).len());
    }

    #[test]
    fn traverse_test_dir() {
        let dir = Path::new("testfiles");
        assert_eq!(find_files_with_ending(dir, vec!["song"]).len(), 2);
        assert_eq!(
            find_files_with_ending(dir, vec!["non_existing_ending"]).len(),
            0
        );
    }

    /// The scan has to record where a file sits inside its repository, since
    /// that is what the URL identifier of the detail view is derived from.
    #[test]
    fn get_source_files_records_the_position_in_the_repository() {
        let files = get_source_files(Path::new("testfiles"));

        let song = files
            .iter()
            .find(|file| file.name == "Amazing Grace")
            .expect("the test library holds Amazing Grace");

        assert_eq!(
            song.relative_path.as_deref(),
            Some("Amazing Grace.song"),
            "the position is relative to the scanned directory, not the whole path"
        );
    }

    #[test]
    fn traverse_test_dir_pdf() {
        let dir = Path::new("testfiles");
        assert_eq!(find_files_with_ending(dir, vec!["pdf"]).len(), 2);
    }

    #[test]
    fn get_source_files_includes_pdf() {
        let dir = Path::new("testfiles");
        let source_files = get_source_files(dir);
        let pdf_files: Vec<&SourceFile> = source_files
            .iter()
            .filter(|sf| sf.file_type == SourceFileType::Pdf)
            .collect();
        assert_eq!(pdf_files.len(), 2);
    }

    /// Length of an MD5 hash in hexadecimal representation (16 bytes × 2 hex chars per byte).
    const MD5_HEX_LENGTH: usize = 32;

    #[test]
    fn get_source_files_computes_md5_hash() {
        let dir = Path::new("testfiles");
        let source_files = get_source_files(dir);
        // All source files should have an MD5 hash since they are read from disk
        for sf in &source_files {
            assert!(
                sf.md5_hash.is_some(),
                "Expected md5_hash to be Some for file: {}",
                sf.name
            );
            // MD5 hash should be a valid 32-character hex string
            let hash = sf.md5_hash.as_ref().unwrap();
            assert_eq!(hash.len(), MD5_HEX_LENGTH, "MD5 hash should be 32 hex chars for: {}", sf.name);
            assert!(
                hash.chars().all(|c| c.is_ascii_hexdigit()),
                "MD5 hash should be valid hex for: {}",
                sf.name
            );
        }
    }

    #[test]
    fn source_file_md5_hash_is_consistent() {
        // The same file should always produce the same MD5 hash
        let dir = Path::new("testfiles");
        let files1 = get_source_files(dir);
        let files2 = get_source_files(dir);
        for (sf1, sf2) in files1.iter().zip(files2.iter()) {
            assert_eq!(
                sf1.md5_hash, sf2.md5_hash,
                "MD5 hash should be consistent for file: {}",
                sf1.name
            );
        }
    }
  
    #[test]
    fn traverse_test_dir_markdown() {
        let dir = Path::new("testfiles");
        assert_eq!(find_files_with_ending(dir, vec!["md"]).len(), 1);
    }

    #[test]
    fn get_source_files_includes_markdown() {
        let dir = Path::new("testfiles");
        let source_files = get_source_files(dir);
        let md_files: Vec<&SourceFile> = source_files
            .iter()
            .filter(|sf| sf.file_type == SourceFileType::Markdown)
            .collect();
        assert_eq!(md_files.len(), 1);
        assert_eq!(md_files[0].name, "example");
    }
}
