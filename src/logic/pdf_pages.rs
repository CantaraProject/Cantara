//! Which pages of a PDF belong in the presentation.
//!
//! A handout is rarely wanted whole. The reading is on page three, the notices
//! are on the last two, and the six pages in between are not for the wall. So
//! an element that is a PDF can carry a pattern saying which of its pages to
//! show — written the way a print dialog is written, because that is the one
//! notation everybody already knows:
//!
//! ```text
//! 1-4       the first four pages
//! 1,3-5     page one, then three to five
//! 1-3+6     one to three, and six
//! (empty)   every page, which is what it does without being asked
//! ```
//!
//! Separators are taken generously — a comma, a full stop, a plus, a semicolon
//! or a space all mean "and". Somebody typing a list of pages should not have
//! to find out which of those this program happens to want, and none of them
//! can mean anything else here.
//!
//! # The order is the document's
//!
//! `3+1` shows page one and then page three, not the other way round, and a
//! page named twice is shown once. This is a *selection* of pages and not a
//! running order: that is how every print dialog reads it, and rearranging a
//! handout is not something a pattern in a text field should quietly do.

use serde::{Deserialize, Serialize};

/// Why a pattern could not be read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PageSelectionError {
    /// Something in it is neither a page number nor a separator.
    Unreadable(String),

    /// A page number of nought. Pages are counted from one, and `0-3` is
    /// almost certainly meant as `1-3`.
    ZeroPage,

    /// A range that ends before it begins, such as `5-3`.
    Backwards { from: u32, to: u32 },

    /// A range with a side missing, such as `4-` or `-4`.
    HalfRange(String),
}

impl PageSelectionError {
    /// The message to show, as a translation key with its parameters.
    ///
    /// Kept beside the error so that a new one cannot be added without a
    /// message: the match is exhaustive.
    pub fn message_key(&self) -> (&'static str, Vec<(&'static str, String)>) {
        match self {
            PageSelectionError::Unreadable(part) => (
                "selection.pdf_pages_error_unreadable",
                vec![("part", part.clone())],
            ),
            PageSelectionError::ZeroPage => ("selection.pdf_pages_error_zero", vec![]),
            PageSelectionError::Backwards { from, to } => (
                "selection.pdf_pages_error_backwards",
                vec![("from", from.to_string()), ("to", to.to_string())],
            ),
            PageSelectionError::HalfRange(part) => (
                "selection.pdf_pages_error_half_range",
                vec![("part", part.clone())],
            ),
        }
    }
}

/// The pages of a PDF to show, as the ranges they were written as.
///
/// An empty selection means every page — that is what an empty field means,
/// and it is also what a new element starts as.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct PageSelection {
    /// Inclusive, one-based, in the order they were written.
    ranges: Vec<(u32, u32)>,
}

/// What separates one part of a pattern from the next.
///
/// Deliberately several. See the note in the module documentation.
const SEPARATORS: [char; 5] = [',', '.', '+', ';', ' '];

/// What joins the two ends of a range. The second is the dash a word processor
/// makes of the first when nobody asked it to.
const RANGE_DASHES: [char; 2] = ['-', '\u{2013}'];

impl PageSelection {
    /// Reads a pattern such as `1-3+6`.
    ///
    /// An empty pattern — or one of nothing but separators — is every page.
    pub fn parse(pattern: &str) -> Result<PageSelection, PageSelectionError> {
        let mut ranges: Vec<(u32, u32)> = Vec::new();

        for part in pattern
            .split(SEPARATORS)
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            ranges.push(parse_part(part)?);
        }

        Ok(PageSelection { ranges })
    }

    /// Whether this stands for every page.
    pub fn is_all(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Whether this page is in the selection.
    pub fn includes(&self, page: u32) -> bool {
        self.is_all()
            || self
                .ranges
                .iter()
                .any(|(from, to)| page >= *from && page <= *to)
    }

    /// The pages to show of a document that has `total` of them.
    ///
    /// In the document's own order and without repeats — see the module
    /// documentation. Pages the document does not have are left out rather
    /// than reported: `1-100` on a four-page handout means the handout.
    pub fn pages(&self, total: u32) -> Vec<u32> {
        (1..=total).filter(|page| self.includes(*page)).collect()
    }
}

/// One part of a pattern: a page, or a range of them.
fn parse_part(part: &str) -> Result<(u32, u32), PageSelectionError> {
    // The dash has to be looked for from the second character on, so that a
    // negative-looking `-4` is reported as the half range it is rather than
    // read as something else.
    let dash = part
        .char_indices()
        .skip(1)
        .find(|(_, character)| RANGE_DASHES.contains(character));

    let Some((dash, dash_char)) = dash else {
        if part.starts_with(RANGE_DASHES) {
            return Err(PageSelectionError::HalfRange(part.to_string()));
        }
        let page = parse_page(part)?;
        return Ok((page, page));
    };

    // The dash's own length, not one byte: the en dash is three, and slicing
    // past it by one lands inside the character.
    let from = part[..dash].trim();
    let to = part[dash + dash_char.len_utf8()..].trim();

    if to.is_empty() {
        return Err(PageSelectionError::HalfRange(part.to_string()));
    }

    let from = parse_page(from)?;
    let to = parse_page(to)?;

    if to < from {
        return Err(PageSelectionError::Backwards { from, to });
    }

    Ok((from, to))
}

/// A single page number.
fn parse_page(text: &str) -> Result<u32, PageSelectionError> {
    let page: u32 = text
        .parse()
        .map_err(|_| PageSelectionError::Unreadable(text.to_string()))?;

    match page {
        0 => Err(PageSelectionError::ZeroPage),
        page => Ok(page),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(pattern: &str, total: u32) -> Vec<u32> {
        PageSelection::parse(pattern)
            .unwrap_or_else(|error| panic!("{pattern:?} did not read: {error:?}"))
            .pages(total)
    }

    /// The three the user is shown as examples have to work.
    #[test]
    fn the_documented_patterns_read_as_documented() {
        assert_eq!(pages("1-4", 10), vec![1, 2, 3, 4]);
        assert_eq!(pages("1,3-5", 10), vec![1, 3, 4, 5]);
        assert_eq!(pages("1-3+6", 10), vec![1, 2, 3, 6]);
    }

    /// Nobody should have to find out which separator this program wants.
    #[test]
    fn every_separator_means_and() {
        for pattern in ["1,3", "1.3", "1+3", "1;3", "1 3", "1, 3", "1 , 3"] {
            assert_eq!(pages(pattern, 5), vec![1, 3], "for {pattern:?}");
        }
    }

    #[test]
    fn an_empty_pattern_is_every_page() {
        assert!(PageSelection::parse("").expect("reads").is_all());
        assert!(PageSelection::parse("   ").expect("reads").is_all());
        assert_eq!(pages("", 3), vec![1, 2, 3]);
    }

    #[test]
    fn a_single_page_is_a_selection_of_one() {
        assert_eq!(pages("2", 5), vec![2]);
    }

    /// A selection, not a running order — see the module documentation.
    #[test]
    fn the_order_is_the_documents_own() {
        assert_eq!(pages("3+1", 5), vec![1, 3]);
    }

    #[test]
    fn a_page_named_twice_is_shown_once() {
        assert_eq!(pages("1-3,2", 5), vec![1, 2, 3]);
        assert_eq!(pages("2,2,2", 5), vec![2]);
    }

    /// A handout is however long it is; asking for more of it is not an error.
    #[test]
    fn pages_the_document_does_not_have_are_left_out() {
        assert_eq!(pages("1-100", 4), vec![1, 2, 3, 4]);
        assert_eq!(pages("9-12", 4), Vec::<u32>::new());
    }

    #[test]
    fn a_dash_a_word_processor_made_is_still_a_range() {
        assert_eq!(pages("2\u{2013}4", 6), vec![2, 3, 4]);
    }

    #[test]
    fn nonsense_is_reported_rather_than_ignored() {
        assert_eq!(
            PageSelection::parse("1-a"),
            Err(PageSelectionError::Unreadable("a".to_string()))
        );
        assert_eq!(
            PageSelection::parse("erste Seite"),
            Err(PageSelectionError::Unreadable("erste".to_string()))
        );
    }

    /// `0-3` is not a page range, and reading it as `1-3` would be guessing.
    #[test]
    fn page_nought_is_reported() {
        assert_eq!(PageSelection::parse("0"), Err(PageSelectionError::ZeroPage));
        assert_eq!(PageSelection::parse("0-3"), Err(PageSelectionError::ZeroPage));
    }

    #[test]
    fn a_backwards_range_is_reported() {
        assert_eq!(
            PageSelection::parse("5-3"),
            Err(PageSelectionError::Backwards { from: 5, to: 3 })
        );
    }

    /// Half-written, which is what a field looks like while it is being typed.
    #[test]
    fn a_range_missing_a_side_is_reported() {
        assert!(matches!(
            PageSelection::parse("4-"),
            Err(PageSelectionError::HalfRange(_))
        ));
        assert!(matches!(
            PageSelection::parse("-4"),
            Err(PageSelectionError::HalfRange(_))
        ));
    }

    #[test]
    fn includes_answers_for_every_page() {
        let selection = PageSelection::parse("2-3,7").expect("reads");

        assert!(!selection.includes(1));
        assert!(selection.includes(2));
        assert!(selection.includes(3));
        assert!(!selection.includes(4));
        assert!(selection.includes(7));
    }

    #[test]
    fn everything_is_included_when_nothing_was_asked_for() {
        let selection = PageSelection::default();

        assert!(selection.is_all());
        assert!(selection.includes(1));
        assert!(selection.includes(999));
    }

    #[test]
    fn every_error_has_a_message() {
        for error in [
            PageSelectionError::Unreadable(String::new()),
            PageSelectionError::ZeroPage,
            PageSelectionError::Backwards { from: 5, to: 3 },
            PageSelectionError::HalfRange(String::new()),
        ] {
            let (key, _) = error.message_key();
            assert!(
                crate::logic::localisation::is_translated(key),
                "{key} has no message"
            );
        }
    }
}
