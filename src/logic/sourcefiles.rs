//! This module provides functionality for handling available source files (for creating output) in Cantara.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The maximal depth for recursive file searching. Implemented as a constant to prevent loops.
const MAX_DEPTH: usize = 6;

/// Recursively finds all files in a directory whose filenames end with the given suffix,
/// up to a recursion depth of 6.
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceFileType {
    Song,
    Presentation,
    Image,
    Video,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceFile {
    pub name: String,
    pub path: PathBuf,
    pub file_type: SourceFileType,
}

/// This function will get all source files in a given directory which can be imported and used by Cantara
///
/// # Parameters
/// - `start_dir`: The borrowed [Path] reference where the recursive search for source files starts
/// # Returns
/// - A vector of [SourceFile]s which contains all results.
/// If no file was found, an empty vector is returned.
///
/// # Hint
/// To prevent infinitive recursion (e.g. if there are symbolic links causing a loop) the maximum depth for recursive search is determined by [MAX_DEPTH].
pub fn get_source_files(start_dir: &Path) -> Vec<SourceFile> {
    let mut source_files: Vec<SourceFile> = vec![];

    find_files_with_ending(start_dir, vec!["song", "jpg", "png"])
        .iter()
        .for_each(|file| {
            let file_extension: &str = file
                .extension()
                .unwrap_or(OsStr::new(""))
                .to_str()
                .unwrap_or("");
            let file_type_option: Option<SourceFileType> =
                match file_extension.to_lowercase().as_str() {
                    "song" => Some(SourceFileType::Song),
                    "png" => Some(SourceFileType::Image),
                    "jpg" => Some(SourceFileType::Image),
                    "jpeg" => Some(SourceFileType::Image),
                    _ => None,
                };
            if let Some(source_file_type) = file_type_option {
                source_files.push(SourceFile {
                    name: file
                        .clone()
                        .file_stem()
                        .unwrap_or(OsStr::new(""))
                        .to_str()
                        .unwrap_or("")
                        .to_string(),
                    path: file.clone(),
                    file_type: source_file_type,
                })
            }
        });

    source_files
}

/// This is a wrapper around source file which ensures that the [SourceFile] is an image
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

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn traverse_test_dir() {
        let dir = Path::new("testfiles");
        assert_eq!(find_files_with_ending(dir, vec!["song"]).len(), 2);
        assert_eq!(
            find_files_with_ending(dir, vec!["non_existing_ending"]).len(),
            0
        );
    }
}
