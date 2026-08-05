//! Finding an element of the library.
//!
//! Two things are searched, and they are searched differently: the *name* of an
//! element fuzzily, its *text* by words. Why, and what a song's text actually
//! is, is written at [`search_source_files`] and [`index_text`].
//!
//! The text of every element is held in an index that
//! [`refresh_search_cache`] fills after the library has been scanned, so that
//! typing never waits for a file to be read or a PDF to be parsed.

use crate::logic::sourcefiles::{SourceFile, SourceFileType, read_source_file};
use cantara_songlib::exporter::text::{TextSettings, text_from_song};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// The indexed text of every element, keyed by file path.
static SONG_CONTENT_CACHE: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();

// Dedicated cache for per-page PDF text, keyed by "{path}#page={N}" strings.
static PDF_PAGE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<PathBuf, String>> {
    SONG_CONTENT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pdf_page_cache() -> &'static Mutex<HashMap<String, String>> {
    PDF_PAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drops everything the index holds.
///
/// Only the tests need this: in the app the index is replaced wholesale by
/// [`refresh_search_cache`] whenever the library is scanned, but the tests
/// share one process and have to start from a known state.
#[cfg(test)]
pub fn invalidate_search_cache() {
    if let Some(m) = SONG_CONTENT_CACHE.get()
        && let Ok(mut map) = m.lock() {
            map.clear();
        }
    if let Some(m) = PDF_PAGE_CACHE.get()
        && let Ok(mut map) = m.lock() {
            map.clear();
        }
}

/// How many pages a PDF has, for the detail view's paging.
#[cfg(not(target_arch = "wasm32"))]
pub fn pdf_page_count(path: &std::path::Path) -> Option<u32> {
    lopdf::Document::load(path)
        .ok()
        .map(|doc| doc.get_pages().len() as u32)
}

/// The web build has no file system to read the document from.
#[cfg(target_arch = "wasm32")]
pub fn pdf_page_count(_path: &std::path::Path) -> Option<u32> {
    None
}

/// The text of one page of a PDF, if the index already holds it.
///
/// Desktop only — the web build has no path to look the document up by and
/// goes through [`extract_pdf_page_text_from_bytes`] instead.
///
/// Never parses the document: this is called while a presentation is running,
/// where a pause of a second would be on the screen for everyone to see.
/// [`refresh_search_cache`] is what fills the page index.
#[cfg(not(target_arch = "wasm32"))]
pub fn extract_pdf_page_text(path: &std::path::Path, page_number: u32) -> Option<String> {
    let cache_key = format!("{}#page={}", path.display(), page_number);
    if let Ok(map) = pdf_page_cache().lock() {
        map.get(&cache_key).cloned()
    } else {
        None
    }
}

/// Extracts plain text from a specific page of a PDF stored in the web VFS (WASM only).
/// `path_key` is the VFS path used for caching; results are stored in `PDF_PAGE_CACHE`.
#[cfg(target_arch = "wasm32")]
pub fn extract_pdf_page_text_from_bytes(bytes: &[u8], page_number: u32, path_key: &str) -> Option<String> {
    let cache_key = format!("{}#page={}", path_key, page_number);
    if let Ok(map) = pdf_page_cache().lock()
        && let Some(cached) = map.get(&cache_key) {
            return Some(cached.clone());
        }
    let doc = lopdf::Document::load_mem(bytes).ok()?;
    let text = doc.extract_text(&[page_number]).ok()?;
    if let Ok(mut map) = pdf_page_cache().lock() {
        map.insert(cache_key, text.clone());
    }
    Some(text)
}

/// What a file contributes to the index.
///
/// A song is *not* indexed as it lies on disk. Its file is a score: YAML keys,
/// Lilypond note groups, ABC headers — none of which anyone searches for, all
/// of which produce hits that look like noise ("d4" matches half the library).
/// The song library can write a song out as plain text, and that is what goes
/// into the index instead: the title and the verses in singing order, which is
/// exactly what a user has in mind when they remember a line and want the song.
///
/// Anything that cannot be parsed as a song falls back to its raw contents —
/// a broken file is still better searched than not searched at all.
///
/// A PDF contributes nothing here: it is the one format whose text has to be
/// parsed out of it, which is far too slow to do while someone is typing, and
/// [`index_pdf`] does it during the scan instead.
fn index_text(file: &SourceFile) -> Option<String> {
    match file.file_type {
        SourceFileType::Song => {
            let content = read_source_file(file).ok()?;
            Some(lyrics_of(file, &content).unwrap_or(content))
        }
        SourceFileType::Markdown => read_source_file(file).ok(),
        _ => None,
    }
}

/// What one file of the library contributes to the two indexes: the text the
/// search matches against, and — for a PDF — the text of each of its pages,
/// keyed as [`PDF_PAGE_CACHE`] holds them.
type IndexedFile = (Option<String>, Vec<(String, String)>);

/// What one PDF contributes to both indexes: the text of each of its pages,
/// keyed as the page index holds them, and all of it as one text for the
/// search.
///
/// The document is parsed *once* for both. It used to be loaded and walked
/// twice per scan — once to fill the page index the presenter console reads
/// from, once more for the search index — which on a library of scanned
/// scores is the single most expensive thing the program does.
#[cfg(not(target_arch = "wasm32"))]
fn index_pdf(path: &std::path::Path) -> IndexedFile {
    let Ok(document) = lopdf::Document::load(path) else {
        return (None, Vec::new());
    };

    let path_str = path.display().to_string();
    let mut pages: Vec<(String, String)> = Vec::new();
    for number in document.get_pages().keys().copied() {
        match document.extract_text(&[number]) {
            Ok(text) => pages.push((format!("{}#page={}", path_str, number), text)),
            Err(error) => log::debug!(
                "Text extraction failed for page {} of {}: {}",
                number, path_str, error
            ),
        }
    }

    if pages.is_empty() {
        return (None, pages);
    }
    let text = pages
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<&str>>()
        .join("\n");
    (Some(text), pages)
}

/// The lyrics of a song file, as plain text.
fn lyrics_of(file: &SourceFile, content: &str) -> Option<String> {
    let song = crate::logic::export::song_from_content(file.file_name(), content).ok()?;
    text_from_song(&song, &TextSettings::default()).ok()
}

/// Reads every file of the library into the index.
///
/// Both the reading and the parsing happen outside the lock, which is taken
/// once at the end to swap the whole index in one go — a search running in
/// parallel is never blocked for longer than that. The files are independent
/// of one another, so on the desktop they are read a handful at a time; see
/// [`crate::logic::parallel`].
pub fn refresh_search_cache(source_files: &[SourceFile]) {
    // What each file contributes: its indexed text, and — for a PDF — the
    // text of each of its pages.
    #[cfg(not(target_arch = "wasm32"))]
    let indexed: Vec<IndexedFile> =
        crate::logic::parallel::map_parallel(source_files, |file| {
            if file.file_type == SourceFileType::Pdf {
                index_pdf(&file.path)
            } else {
                (index_text(file), Vec::new())
            }
        });

    // The web build has neither threads to read in parallel with nor a
    // document to parse: its libraries are already in memory.
    #[cfg(target_arch = "wasm32")]
    let indexed: Vec<IndexedFile> = source_files
        .iter()
        .map(|file| (index_text(file), Vec::new()))
        .collect();

    let mut entries: Vec<(PathBuf, String)> = Vec::with_capacity(source_files.len());
    let mut pages: Vec<(String, String)> = Vec::new();
    for (file, (text, file_pages)) in source_files.iter().zip(indexed) {
        if let Some(text) = text {
            entries.push((file.path.clone(), text));
        }
        pages.extend(file_pages);
    }

    if let Ok(mut map) = cache().lock() {
        map.clear();
        map.extend(entries);
    }

    // The page index is replaced rather than added to, so that the pages of a
    // repository the user has removed do not stay behind. It is filled here
    // only where a document can be parsed; the web build caches the single
    // page it is shown, see [`extract_pdf_page_text_from_bytes`].
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(mut map) = pdf_page_cache().lock() {
        map.clear();
        map.extend(pages);
    }
    #[cfg(target_arch = "wasm32")]
    drop(pages);
}

/// The indexed text of a file, from the index if it is there.
///
/// A file that has not been indexed yet is read on the spot — except for a
/// PDF, which is left to [`refresh_search_cache`]: parsing one takes long
/// enough to be felt between two keystrokes.
pub fn read_source_file_content(source_file: &SourceFile) -> Option<String> {
    if let Ok(map) = cache().lock()
        && let Some(cached) = map.get(&source_file.path)
    {
        return Some(cached.clone());
    }

    if source_file.file_type == SourceFileType::Pdf {
        return None;
    }

    let text = index_text(source_file)?;
    if let Ok(mut map) = cache().lock() {
        map.insert(source_file.path.clone(), text.clone());
    }
    Some(text)
}

/// A passage of an element's text that matched, ready to be shown.
#[derive(Clone, PartialEq, Debug)]
pub struct Excerpt {
    /// The passage itself, cut out around the match.
    pub text: String,
    /// Which characters of `text` matched, as positions in `char`s.
    pub highlights: Vec<usize>,
}

/// One element the search found.
#[derive(Clone, PartialEq, Debug)]
pub struct SearchResult {
    pub source_file: SourceFile,
    /// Which characters of the element's name matched, as positions in `char`s.
    /// Empty when the name did not match and only the content did.
    pub title_highlights: Vec<usize>,
    /// The passage that matched, when the hit came from the content.
    pub excerpt: Option<Excerpt>,
    /// How well this hit fits the query. Higher is better; only meaningful in
    /// comparison with the other results of the same search.
    pub score: i64,
}

/// How much a hit in the name outweighs a hit in the text.
///
/// Someone who types a title wants that song, not the twelve others that
/// happen to quote it — and a title hit is the more precise statement, since
/// the name is short and the text is long.
const TITLE_WEIGHT: i64 = 1000;

/// The score every content hit gets, before the bonus for matching more words.
const CONTENT_BASE_SCORE: i64 = 100;

/// How much text is shown around a hit in the content, in characters.
const EXCERPT_CONTEXT: usize = 40;

/// Searches the library for `query`.
///
/// Names are matched *fuzzily*: the characters of the query have to turn up in
/// the name in order, but not next to each other, so "amzgrace" and "amazing g"
/// both find "Amazing Grace" and a typo does not send the user back to the
/// keyboard. Each hit carries a score, and the list is ordered by it.
///
/// The text of an element is matched by words instead. A fuzzy match over a
/// whole song would succeed for practically every song — the letters of any
/// short query can be found scattered through a page of lyrics — so this looks
/// for every word of the query, in any order, which is what someone half
/// remembering a line actually needs.
pub fn search_source_files(source_files: &[SourceFile], query: &str) -> Vec<SearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let matcher = SkimMatcherV2::default();
    let words: Vec<Vec<char>> = query.split_whitespace().map(fold).collect();

    let mut results: Vec<SearchResult> = Vec::new();

    for source_file in source_files {
        if let Some((score, indices)) = matcher.fuzzy_indices(&source_file.name, query) {
            results.push(SearchResult {
                source_file: source_file.clone(),
                title_highlights: indices,
                excerpt: None,
                score: score * TITLE_WEIGHT,
            });
            continue;
        }

        let searchable = matches!(
            source_file.file_type,
            SourceFileType::Song | SourceFileType::Markdown | SourceFileType::Pdf
        );
        if !searchable {
            continue;
        }

        let Some(content) = read_source_file_content(source_file) else {
            continue;
        };
        let Some((excerpt, matched_words)) = content_match(&content, &words) else {
            continue;
        };

        results.push(SearchResult {
            source_file: source_file.clone(),
            title_highlights: Vec::new(),
            excerpt: Some(excerpt),
            score: CONTENT_BASE_SCORE * matched_words as i64,
        });
    }

    // Best first; among equally good hits the alphabet decides, so the list
    // does not reshuffle itself for no visible reason.
    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.source_file.name.cmp(&b.source_file.name))
    });

    results
}

/// Lower-cases a text, one character per character.
///
/// `str::to_lowercase` is not usable here, because for a few characters it
/// returns more than one — Turkish `İ` becomes `i` followed by a combining dot
/// — and every position after such a character would be off by one. The
/// excerpt and its highlights are cut out of the *original* text by position,
/// so a shift there does not merely mislead the highlighting, it can index
/// past the end of the text and bring the program down.
///
/// Where a character has no single-character lower case, the first character
/// of its expansion is used, which is the letter itself; the mark that would
/// have followed is not something anyone types into the search field.
fn fold(text: &str) -> Vec<char> {
    text.chars()
        .map(|character| character.to_lowercase().next().unwrap_or(character))
        .collect()
}

/// Whether the character at `at` begins a word.
fn starts_word(text: &[char], at: usize) -> bool {
    at == 0 || !text[at - 1].is_alphanumeric()
}

/// The first position at which `needle` begins a word in `haystack`.
///
/// A word of the query may be cut short — someone typing `wretc` is on their
/// way to `wretch` and should see the song before they get there — but it has
/// to *begin* where a word begins. Without that, `he` is found inside `the`
/// and a two-letter query brings back the whole library.
fn find_word(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    (0..haystack.len())
        .find(|start| starts_word(haystack, *start) && haystack[*start..].starts_with(needle))
}

/// Looks for every word of the query in `content` and cuts out the passage
/// around the first of them.
///
/// Returns the passage and how many of the words were found — all of them, or
/// nothing: a query is a description of one line, and a text that holds only
/// half of its words is not that line.
fn content_match(content: &str, words: &[Vec<char>]) -> Option<(Excerpt, usize)> {
    let original: Vec<char> = content.chars().collect();
    let haystack: Vec<char> = fold(content);
    debug_assert_eq!(haystack.len(), original.len());

    // Where each word turns up, in characters. One occurrence per word is
    // enough to place the excerpt; the rest are found again below, inside it.
    let mut hits: Vec<usize> = Vec::with_capacity(words.len());
    for needle in words {
        hits.push(find_word(&haystack, needle)?);
    }

    let first = hits.into_iter().min()?;
    let start = first.saturating_sub(EXCERPT_CONTEXT);
    let end = (first + EXCERPT_CONTEXT * 2).min(original.len());
    let excerpt: String = original[start..end].iter().collect();

    // Highlight every occurrence of every word that falls inside the excerpt,
    // not just the one that placed it. Whether a word begins is asked of the
    // whole text rather than of the cut-out passage, so that a word the cut
    // happens to run through is not mistaken for one starting at the edge.
    let excerpt_lower = &haystack[start..end];
    let mut highlights: Vec<usize> = Vec::new();
    for needle in words {
        if needle.is_empty() {
            continue;
        }
        for offset in 0..excerpt_lower.len() {
            if starts_word(&haystack, start + offset)
                && excerpt_lower[offset..].starts_with(needle)
            {
                highlights.extend(offset..offset + needle.len());
            }
        }
    }
    highlights.sort_unstable();
    highlights.dedup();

    Some((
        Excerpt {
            text: excerpt,
            highlights,
        },
        words.len(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::sourcefiles::{SourceFile, SourceFileType};
    use std::path::PathBuf;

    fn file(name: &str, path: &str, file_type: SourceFileType) -> SourceFile {
        SourceFile {
            name: name.to_string(),
            path: PathBuf::from(path),
            file_type,
            md5_hash: None,
            relative_path: None,
        }
    }

    fn song(name: &str, path: &str) -> SourceFile {
        file(name, path, SourceFileType::Song)
    }

    /// The point of the fuzzy match: a name is found without being spelled out
    /// in full, and a slip of the finger does not cost the hit.
    #[test]
    fn a_name_is_found_without_being_typed_out() {
        let library = vec![song("Amazing Grace", "testfiles/Amazing Grace.song")];

        for query in ["amazing grace", "amazing", "amzgrace", "AmGr"] {
            let results = search_source_files(&library, query);
            assert_eq!(results.len(), 1, "'{query}' should find the song");
            assert!(
                !results[0].title_highlights.is_empty(),
                "'{query}' should be a name match"
            );
        }
    }

    /// A query that shares no letters with anything finds nothing — a fuzzy
    /// match must still be able to say no.
    #[test]
    fn a_query_that_fits_nothing_finds_nothing() {
        let library = vec![song("Amazing Grace", "testfiles/Amazing Grace.song")];

        assert!(search_source_files(&library, "xylophon").is_empty());
    }

    /// The name is the stronger statement, so it comes first even when another
    /// song quotes the words in its text.
    #[test]
    fn a_name_match_outranks_a_content_match() {
        invalidate_search_cache();
        let library = vec![
            song("Alas, and Did My Savior Bleed", "testfiles/Alas, and Did My Savior Bleed.song"),
            song("Amazing Grace", "testfiles/Amazing Grace.song"),
        ];

        let results = search_source_files(&library, "amazing");

        assert!(!results.is_empty());
        assert_eq!(results[0].source_file.name, "Amazing Grace");
        assert!(!results[0].title_highlights.is_empty());
    }

    /// What a song contributes to the index is its lyrics, not its file: the
    /// YAML keys and the Lilypond notes of the score must not be searchable,
    /// or every second query would hit them.
    #[test]
    fn a_song_is_indexed_as_lyrics_rather_than_as_its_file() {
        invalidate_search_cache();
        let indexed = index_text(&song("Amazing Grace", "testfiles/Amazing Grace.song.yml"))
            .expect("the song can be read");

        assert!(
            indexed.contains("Amazing grace, How sweet the sound"),
            "the lyrics belong in the index, got: {indexed}"
        );
        assert!(
            !indexed.contains("version:") && !indexed.contains("\\breathe"),
            "neither the YAML keys nor the notation belong in it, got: {indexed}"
        );
    }

    /// Following from that: a search for a note group finds nothing, while a
    /// search for a line of the lyrics finds the song.
    #[test]
    fn the_notation_is_not_searchable_but_the_lyrics_are() {
        invalidate_search_cache();
        let library = vec![song("Amazing Grace", "testfiles/Amazing Grace.song.yml")];

        let lyrics = search_source_files(&library, "wretch like me");
        assert_eq!(lyrics.len(), 1, "a line of the lyrics should find the song");
        assert!(
            lyrics[0].excerpt.is_some(),
            "a hit in the text should come with the passage it was found in"
        );

        assert!(
            search_source_files(&library, "breathe").is_empty(),
            "a Lilypond command should not be searchable"
        );
    }

    /// The words of a query may be scattered over the line, and they may be in
    /// any order — someone half remembering a line does not remember its
    /// grammar.
    #[test]
    fn the_words_of_a_query_are_found_in_any_order() {
        invalidate_search_cache();
        let library = vec![song("Amazing Grace", "testfiles/Amazing Grace.song.yml")];

        assert_eq!(search_source_files(&library, "wretch sound").len(), 1);
        // …but every word has to be there.
        assert!(search_source_files(&library, "wretch banana").is_empty());
    }

    /// The passage that is shown carries the positions of what matched, so the
    /// list can pick them out without searching the text again.
    #[test]
    fn a_passage_knows_which_characters_matched() {
        let (excerpt, matched) =
            content_match("Amazing grace, how sweet the sound", &[fold("sweet")])
                .expect("the word is in the text");

        assert_eq!(matched, 1);
        let highlighted: String = excerpt
            .text
            .chars()
            .enumerate()
            .filter(|(index, _)| excerpt.highlights.contains(index))
            .map(|(_, character)| character)
            .collect();
        assert_eq!(highlighted, "sweet");
    }

    /// A word of the query has to start where a word starts. Without that,
    /// two letters of a half-typed query are found inside every second word
    /// of the library and the result list is noise.
    #[test]
    fn a_word_is_found_at_the_start_of_a_word_only() {
        assert!(
            content_match("how sweet the sound", &[fold("he")]).is_none(),
            "'he' must not be found inside 'the'"
        );
        assert!(
            content_match("how sweet the sound", &[fold("the")]).is_some(),
            "'the' is a word of the text"
        );
    }

    /// …but it may stop short of the end, or the list would stay empty until
    /// the last letter of a word has been typed.
    #[test]
    fn a_word_of_the_query_may_be_half_typed() {
        for query in ["s", "swe", "sweet"] {
            assert!(
                content_match("how sweet the sound", &[fold(query)]).is_some(),
                "'{query}' should find 'sweet'"
            );
        }
    }

    /// Lower-casing must not move any character: the passage and the positions
    /// of what matched in it are cut out of the original text by position, and
    /// `İ` lower-cases to *two* characters.
    ///
    /// Before this, the mismatch shifted the highlights and could index past
    /// the end of the text.
    #[test]
    fn a_character_whose_lower_case_is_longer_does_not_shift_the_passage() {
        let content = format!("{} Amazing grace", "İ".repeat(50));
        let content = content.as_str();
        assert_eq!(fold(content).len(), content.chars().count());
        assert!(
            content.to_lowercase().chars().count() > content.chars().count(),
            "the fixture no longer reproduces the mismatch"
        );

        let (excerpt, _) = content_match(content, &[fold("grace")])
            .expect("the word is in the text");

        let highlighted: String = excerpt
            .text
            .chars()
            .enumerate()
            .filter(|(index, _)| excerpt.highlights.contains(index))
            .map(|(_, character)| character)
            .collect();
        assert_eq!(highlighted, "grace");
    }

    /// The search is indifferent to case in both directions.
    #[test]
    fn case_does_not_matter_in_the_content() {
        let (excerpt, _) = content_match("Amazing GRACE", &[fold("Grace")])
            .expect("the word is in the text");

        assert!(excerpt.text.contains("GRACE"));
        assert!(!excerpt.highlights.is_empty());
    }

    #[test]
    fn search_markdown_content() {
        invalidate_search_cache();
        let sf = file("example", "testfiles/example.md", SourceFileType::Markdown);

        let results = search_source_files(&[sf], "slide");

        assert!(!results.is_empty(), "Should find markdown file by content");
    }

    #[test]
    fn search_returns_markdown_title_match() {
        let sf = file("example", "testfiles/example.md", SourceFileType::Markdown);

        let results = search_source_files(&[sf], "example");

        assert!(!results.is_empty(), "Should find markdown file by title");
        assert!(!results[0].title_highlights.is_empty(), "Should be a title match");
    }

    #[test]
    fn search_pdf_content() {
        // This PDF fixture is expected to have no extractable text; it should not produce matches.
        invalidate_search_cache();
        let sf = file("Example", "testfiles/Example.pdf", SourceFileType::Pdf);

        let results = search_source_files(&[sf], "unfindable");

        assert!(
            results.is_empty(),
            "PDFs without extractable text should not produce search results"
        );
    }

    #[test]
    fn search_pdf_with_text() {
        // MultiPage.pdf has pages with embedded text ("Page 1", "Page 2", "Page 3").
        // Query "page 2" contains a space+digit so it matches the content but not the
        // filename "MultiPage" (which would only match the bare word "page").
        let sf = file("MultiPage", "testfiles/MultiPage.pdf", SourceFileType::Pdf);
        // PDF content search requires the index to be filled first: nothing
        // parses a document while the user is typing.
        invalidate_search_cache();
        refresh_search_cache(std::slice::from_ref(&sf));

        let results = search_source_files(&[sf], "page 2");

        assert!(!results.is_empty(), "PDF with embedded text should produce search results");
        assert!(
            results[0].title_highlights.is_empty(),
            "Should be a content match, not a title match"
        );
    }

    #[test]
    fn refresh_cache_includes_markdown() {
        let sf = file("example", "testfiles/example.md", SourceFileType::Markdown);
        invalidate_search_cache();
        refresh_search_cache(std::slice::from_ref(&sf));

        let content = read_source_file_content(&sf);

        assert!(content.is_some(), "Markdown content should be cached");
        assert!(
            content.unwrap().contains("slide"),
            "Markdown content should contain 'slide'"
        );
    }

    #[test]
    fn refresh_cache_includes_pdf_text() {
        // MultiPage.pdf has embedded text ("Page 1", "Page 2", "Page 3") across its pages.
        let sf = file("MultiPage", "testfiles/MultiPage.pdf", SourceFileType::Pdf);
        invalidate_search_cache();
        refresh_search_cache(std::slice::from_ref(&sf));

        let content = read_source_file_content(&sf);

        assert!(content.is_some(), "PDF content should be cached after refresh");
        assert!(
            content.unwrap().contains("Page"),
            "Cached PDF content should contain 'Page'"
        );
    }
}
