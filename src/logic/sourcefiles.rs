//! This module provides functionality for handling available source files (for creating output) in Cantara.


use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The maximal depth for recursive file searching. Implemented as a constant to prevent loops.
const MAX_DEPTH: usize = 6;

/// Recursively finds all files in a directory whose filenames end with the given suffix,
/// up to a recursion depth of the constant [MAX_DEPTH].
///
/// # Arguments
/// * `dir` - The starting directory path.
/// * `ending` - The suffix to match (e.g., ".txt").
/// * `depth` - The current recursion depth (starts at 0).
///
/// # Returns
/// A vector of `PathBuf`s containing the full paths of matching files.
fn find_files_recursive(dir: &Path, endings: &Vec<&'static str>, depth: usize) -> Vec<PathBuf> {
    let mut result = Vec::new();

    // Stop recursion beyond depth 6
    if depth > MAX_DEPTH {
        return result;
    }

    // Read directory entries, skip if there's an error
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();

                // If it's a file, check if its name ends with the given ending
                if path.is_file() {
                    if let Some(file_name) = path.file_name() {
                        if let Some(file_name_str) = file_name.to_str() {
                            for ending in endings {
                                if file_name_str.ends_with(ending) {
                                    result.push(path.clone());
                                }
                            }
                        }
                    }
                }
                // If it's a directory, recurse into it
                else if path.is_dir() {
                    let sub_result = find_files_recursive(&path, endings, depth + 1);
                    result.extend(sub_result);
                }
            }
        }
    }

    result
}

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
fn find_files_with_ending(dir: &Path, endings: Vec<&'static str>) -> Vec<PathBuf> {
    // Check if the directory exists and is a directory
    if !dir.exists() || !dir.is_dir() {
        return Vec::new();
    }

    // Start recursive traversal at depth 0.
    find_files_recursive(dir, &endings, 0)
}

/// Every file below `dir` that Cantara can read, each listed once.
///
/// Matching goes through [`SourceFileType::of`], so this cannot fall behind the
/// list of supported formats.
fn find_supported_files(dir: &Path) -> Vec<PathBuf> {
    let suffixes: Vec<&'static str> = SourceFileType::SUFFIXES
        .iter()
        .map(|(suffix, _)| *suffix)
        .collect();

    let mut files = find_files_with_ending(dir, suffixes);
    // A file whose name ends with two of the suffixes — `.song.yml` ends with
    // both `.song.yml` and `.yml` were that ever added — would otherwise be
    // listed twice and appear twice in the song list.
    files.sort();
    files.dedup();
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
    find_supported_files(start_dir)
        .into_iter()
        .filter_map(|file| {
            let file_name = file.file_name()?.to_str()?;
            let file_type = SourceFileType::of(file_name)?;

            // Read the file content once to compute the MD5 hash.
            // The file path is stored in `SourceFile.path`, so the content is not
            // retained after this function returns; subsequent reads happen on demand.
            let md5_hash = fs::read(&file)
                .ok()
                .map(|content| format!("{:x}", md5::compute(&content)));

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

/// This is a wrapper around [SourceFile] which ensures that the [SourceFile] is a PDF
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PdfSourceFile(SourceFile);

impl PdfSourceFile {
    // Constructor that enforces the FileType::Pdf constraint
    pub fn new(source_file: SourceFile) -> Option<Self> {
        if matches!(source_file.file_type, SourceFileType::Pdf) {
            Some(PdfSourceFile(source_file))
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

impl SourceFile {
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

    #[test]
    fn pdf_source_file_wrapper() {
        let pdf_sf = SourceFile {
            name: "test".to_string(),
            path: PathBuf::from("test.pdf"),
            file_type: SourceFileType::Pdf,
            md5_hash: None,
            relative_path: None,
        };
        assert!(PdfSourceFile::new(pdf_sf).is_some());

        let song_sf = SourceFile {
            name: "test".to_string(),
            path: PathBuf::from("test.song"),
            file_type: SourceFileType::Song,
            md5_hash: None,
            relative_path: None,
        };
        assert!(PdfSourceFile::new(song_sf).is_none());
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
