use std::fs;
use std::path::{Path, PathBuf};

!// This module provides functionality for handling available source files (for creating output) in Cantara.

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
pub fn find_files_recursive(dir: &Path, ending: &str, depth: usize) -> Vec<PathBuf> {
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
                            if file_name_str.ends_with(ending) {
                                result.push(path.clone());
                            }
                        }
                    }
                }
                // If it's a directory, recurse into it
                else if path.is_dir() {
                    let sub_result = find_files_recursive(&path, ending, depth + 1);
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
/// * `ending` - The suffix to match (e.g., ".txt").
///
/// # Returns
/// A vector of `PathBuf`s containing the full paths of matching files.
///
/// # Notes
/// - Returns an empty vector if the directory does not exist or is not a directory.
/// - The `ending` should include the dot if matching extensions (e.g., ".txt").
/// - Matching is case-sensitive.
/// - Symlinks are followed (default behavior of `is_file` and `is_dir`).
pub fn find_files_with_ending(dir: &Path, ending: &str) -> Vec<PathBuf> {
    // Check if the directory exists and is a directory
    if !dir.exists() || !dir.is_dir() {
        return Vec::new();
    }
    
    // Start recursive traversal at depth 0.
    find_files_recursive(dir, ending, 0)
}