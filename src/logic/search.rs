//! This module provides search functionality for source files in Cantara.

use crate::logic::sourcefiles::{SourceFile, SourceFileType};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// Cache for full-document content keyed by file path (Song, Markdown, full PDF text).
static SONG_CONTENT_CACHE: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();

// Dedicated cache for per-page PDF text, keyed by "{path}#page={N}" strings.
static PDF_PAGE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<PathBuf, String>> {
    SONG_CONTENT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pdf_page_cache() -> &'static Mutex<HashMap<String, String>> {
    PDF_PAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clears the entire search cache. Call this to invalidate cached file contents.
pub fn invalidate_search_cache() {
    if let Some(m) = SONG_CONTENT_CACHE.get() {
        if let Ok(mut map) = m.lock() {
            map.clear();
        }
    }
    if let Some(m) = PDF_PAGE_CACHE.get() {
        if let Ok(mut map) = m.lock() {
            map.clear();
        }
    }
}

/// Extracts plain text from a PDF file (non-WASM only).
/// Returns all page texts concatenated, or None if extraction fails.
#[cfg(not(target_arch = "wasm32"))]
fn extract_pdf_text(path: &std::path::Path) -> Option<String> {
    let doc = lopdf::Document::load(path).ok()?;
    let pages = doc.get_pages();
    let page_numbers: Vec<u32> = pages.keys().copied().collect();
    let mut texts = Vec::new();
    for page_num in page_numbers {
        if let Ok(text) = doc.extract_text(&[page_num]) {
            texts.push(text);
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

/// Extracts plain text from a specific page of a PDF file (non-WASM only).
/// Results are cached in `PDF_PAGE_CACHE` so the PDF is only parsed once per
/// path+page combination.
#[cfg(not(target_arch = "wasm32"))]
pub fn extract_pdf_page_text(path: &std::path::Path, page_number: u32) -> Option<String> {
    let cache_key = format!("{}#page={}", path.display(), page_number);
    if let Ok(map) = pdf_page_cache().lock() {
        if let Some(cached) = map.get(&cache_key) {
            return Some(cached.clone());
        }
    }
    let doc = lopdf::Document::load(path).ok()?;
    let text = doc.extract_text(&[page_number]).ok()?;
    if let Ok(mut map) = pdf_page_cache().lock() {
        map.insert(cache_key, text.clone());
    }
    Some(text)
}

/// Extracts plain text from a PDF stored in the web VFS (WASM only).
#[cfg(target_arch = "wasm32")]
fn extract_pdf_text_from_bytes(bytes: &[u8]) -> Option<String> {
    let doc = lopdf::Document::load_mem(bytes).ok()?;
    let pages = doc.get_pages();
    let page_numbers: Vec<u32> = pages.keys().copied().collect();
    let mut texts = Vec::new();
    for page_num in page_numbers {
        if let Ok(text) = doc.extract_text(&[page_num]) {
            texts.push(text);
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

/// Extracts plain text from a specific page of a PDF stored in the web VFS (WASM only).
/// `path_key` is the VFS path used for caching; results are stored in `PDF_PAGE_CACHE`.
#[cfg(target_arch = "wasm32")]
pub fn extract_pdf_page_text_from_bytes(bytes: &[u8], page_number: u32, path_key: &str) -> Option<String> {
    let cache_key = format!("{}#page={}", path_key, page_number);
    if let Ok(map) = pdf_page_cache().lock() {
        if let Some(cached) = map.get(&cache_key) {
            return Some(cached.clone());
        }
    }
    let doc = lopdf::Document::load_mem(bytes).ok()?;
    let text = doc.extract_text(&[page_number]).ok()?;
    if let Ok(mut map) = pdf_page_cache().lock() {
        map.insert(cache_key, text.clone());
    }
    Some(text)
}

/// Optionally (re)populate the cache with the provided list of source files.
/// This will read all Song, Markdown, and PDF files from disk and cache their contents.
/// If a file can't be read, it will simply be skipped.
pub fn refresh_search_cache(source_files: &[SourceFile]) {
    let mut map = cache().lock().expect("cache poisoned");
    map.clear();
    for sf in source_files {
        match sf.file_type {
            SourceFileType::Song | SourceFileType::Markdown => {
                if let Ok(content) = fs::read_to_string(&sf.path) {
                    map.insert(sf.path.clone(), content);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            SourceFileType::Pdf => {
                if let Some(content) = extract_pdf_text(&sf.path) {
                    map.insert(sf.path.clone(), content);
                }
            }
            _ => {}
        }
    }
}

/// Helper function to read the content of a source file, using the cache for Song, Markdown, and PDF files
pub fn read_source_file_content(source_file: &SourceFile) -> Option<String> {
    match source_file.file_type {
        SourceFileType::Song | SourceFileType::Markdown => {
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
        #[cfg(not(target_arch = "wasm32"))]
        SourceFileType::Pdf => {
            // Try cache first
            if let Ok(mut map) = cache().lock() {
                if let Some(cached) = map.get(&source_file.path) {
                    return Some(cached.clone());
                }
                // Not cached: extract from PDF and store
                if let Some(content) = extract_pdf_text(&source_file.path) {
                    map.insert(source_file.path.clone(), content.clone());
                    return Some(content);
                }
            }
            None
        }
        #[cfg(target_arch = "wasm32")]
        SourceFileType::Pdf => {
            // Try cache first
            if let Ok(mut map) = cache().lock() {
                if let Some(cached) = map.get(&source_file.path) {
                    return Some(cached.clone());
                }
            }
            // Read from web VFS
            if let Some(path_str) = source_file.path.to_str() {
                if let Some(bytes) = crate::logic::settings::RepositoryType::web_read_file(path_str) {
                    if let Some(content) = extract_pdf_text_from_bytes(&bytes) {
                        if let Ok(mut map) = cache().lock() {
                            map.insert(source_file.path.clone(), content.clone());
                        }
                        return Some(content);
                    }
                }
            }
            None
        }
        _ => None,
    }
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

        // Check if the query matches the content (for song, markdown, and PDF files)
        let should_search_content = matches!(
            source_file.file_type,
            SourceFileType::Song | SourceFileType::Markdown | SourceFileType::Pdf
        );

        if should_search_content {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::sourcefiles::{SourceFile, SourceFileType};
    use std::path::PathBuf;

    #[test]
    fn search_markdown_content() {
        let sf = SourceFile {
            name: "example".to_string(),
            path: PathBuf::from("testfiles/example.md"),
            file_type: SourceFileType::Markdown,
        };
        let results = search_source_files(&[sf], "slide");
        assert!(!results.is_empty(), "Should find markdown file by content");
    }

    #[test]
    fn search_pdf_content() {
        // This PDF fixture is expected to have no extractable text; it should not produce matches.
        let sf = SourceFile {
            name: "Example".to_string(),
            path: PathBuf::from("testfiles/Example.pdf"),
            file_type: SourceFileType::Pdf,
        };
        let results = search_source_files(&[sf], "test");
        assert!(
            results.is_empty(),
            "PDFs without extractable text should not produce search results"
        );
    }

    #[test]
    fn search_returns_markdown_title_match() {
        let sf = SourceFile {
            name: "example".to_string(),
            path: PathBuf::from("testfiles/example.md"),
            file_type: SourceFileType::Markdown,
        };
        let results = search_source_files(&[sf], "example");
        assert!(!results.is_empty(), "Should find markdown file by title");
        assert!(results[0].is_title_match, "Should be a title match");
    }

    #[test]
    fn refresh_cache_includes_markdown() {
        let sf = SourceFile {
            name: "example".to_string(),
            path: PathBuf::from("testfiles/example.md"),
            file_type: SourceFileType::Markdown,
        };
        invalidate_search_cache();
        refresh_search_cache(&[sf.clone()]);
        let content = read_source_file_content(&sf);
        assert!(content.is_some(), "Markdown content should be cached");
        assert!(
            content.unwrap().contains("slide"),
            "Markdown content should contain 'slide'"
        );
    }
}
