//! This module provides search functionality for source files in Cantara.

use crate::logic::sourcefiles::{SourceFile, SourceFileType};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// Simple global cache for song file contents. Avoids re-reading from disk on every search.
static SONG_CONTENT_CACHE: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<PathBuf, String>> {
    SONG_CONTENT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clears the entire search cache. Call this to invalidate cached file contents.
pub fn invalidate_search_cache() {
    if let Some(m) = SONG_CONTENT_CACHE.get() {
        if let Ok(mut map) = m.lock() {
            map.clear();
        }
    }
}

/// Optionally (re)populate the cache with the provided list of source files.
/// This will read all Song files from disk and cache their contents.
/// If a file can't be read, it will simply be skipped.
pub fn refresh_search_cache(source_files: &[SourceFile]) {
    let mut map = cache().lock().expect("cache poisoned");
    map.clear();
    for sf in source_files {
        if sf.file_type == SourceFileType::Song {
            if let Ok(content) = fs::read_to_string(&sf.path) {
                map.insert(sf.path.clone(), content);
            }
        }
    }
}

/// Helper function to read the content of a source file, using the cache for Song files
pub fn read_source_file_content(source_file: &SourceFile) -> Option<String> {
    if source_file.file_type != SourceFileType::Song {
        return None;
    }

    // Try cache first
    if let Ok(mut map) = cache().lock() {
        if let Some(cached) = map.get(&source_file.path) {
            return Some(cached.clone());
        }
        // Not cached: read from disk and store
        if let Ok(content) = fs::read_to_string(&source_file.path) {
            map.insert(source_file.path.clone(), content.clone());
            return Some(content);
        }
    }

    None
}

/// Struct to represent a search result
#[derive(Clone, PartialEq)]
pub struct SearchResult {
    pub source_file: SourceFile,
    pub matched_content: Option<String>,
    pub is_title_match: bool,
}

/// Helper function to perform fuzzy search on source files
pub fn search_source_files(source_files: &[SourceFile], query: &str) -> Vec<SearchResult> {
    if query.is_empty() {
        return Vec::new();
    }

    let query = query.to_lowercase();
    let mut results = Vec::new();

    for source_file in source_files {
        let name_lower = source_file.name.to_lowercase();
        let is_title_match = name_lower.contains(&query);

        // Check if the query matches the title
        if is_title_match {
            results.push(SearchResult {
                source_file: source_file.clone(),
                matched_content: None,
                is_title_match: true,
            });
            continue;
        }

        // Check if the query matches the content (for song files)
        if source_file.file_type == SourceFileType::Song {
            if let Some(content) = read_source_file_content(source_file) {
                let content_lower = content.to_lowercase();
                if content_lower.contains(&query) {
                    // Find the context around the match
                    let match_index = content_lower.find(&query).unwrap();

                    // Convert byte indices to char indices for safe slicing
                    let content_chars: Vec<char> = content.chars().collect();
                    let _content_lower_chars: Vec<char> = content_lower.chars().collect();

                    // Find the character index corresponding to the byte index
                    let mut char_count: usize = 0;
                    let mut match_char_index: usize = 0;

                    for (i, _) in content_lower.char_indices() {
                        if i == match_index {
                            match_char_index = char_count;
                            break;
                        }
                        char_count += 1;
                    }

                    // Calculate safe character indices for the context
                    let start_char = match_char_index.saturating_sub(30);
                    let end_char =
                        (match_char_index + query.chars().count() + 30).min(content_chars.len());

                    // Create the context string from character indices
                    let context: String = content_chars[start_char..end_char].iter().collect();

                    results.push(SearchResult {
                        source_file: source_file.clone(),
                        matched_content: Some(context),
                        is_title_match: false,
                    });
                }
            }
        }
    }

    // Sort results: title matches first, then content matches
    results.sort_by(|a, b| {
        if a.is_title_match && !b.is_title_match {
            std::cmp::Ordering::Less
        } else if !a.is_title_match && b.is_title_match {
            std::cmp::Ordering::Greater
        } else {
            a.source_file.name.cmp(&b.source_file.name)
        }
    });

    results
}
