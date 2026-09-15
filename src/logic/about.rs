//! What the program says about itself: its version, its copyright, and the
//! prose that goes under them.
//!
//! The page that shows all this is
//! [`crate::components::about_components`]; everything decidable without a
//! screen is decided here, so that the language fallback — the only part with
//! real logic in it — can be tested without a browser.
//!
//! The texts are compiled in from `docs/about/` by `build.rs`, one file per
//! language. See `docs/specs/0005-add-info-page.md`.
//!
//! # A note on the markup
//!
//! These texts are rendered as Markdown and put into the page through
//! `dangerous_inner_html`, which means raw HTML in them reaches the browser.
//! That is safe *here* and only here: they come out of the repository and are
//! baked into the binary, so anyone able to change them can change the program
//! anyway. It is not a precedent for a file a user supplies.

include!(concat!(env!("OUT_DIR"), "/about_data.rs"));

/// The year Cantara was first released, and the year the copyright runs from.
///
/// Fixed, and not to be "updated" alongside the other one: it says when the
/// work began, not when this binary was built.
const FIRST_YEAR: i32 = 2015;

/// Who holds the copyright.
const AUTHOR: &str = "Jan Martin Reckel";

/// A build that predates the first release is a build with a broken clock, and
/// the copyright line it writes would read backwards.
///
/// Both numbers are known while compiling, so this is the compiler's job and
/// not a test's: a machine whose clock is wrong does not produce a binary that
/// fails its tests, it produces no binary at all.
const _: () = assert!(
    BUILD_YEAR >= FIRST_YEAR,
    "this was built before Cantara was first released — check the clock"
);

/// What this binary calls itself.
///
/// `Cargo.toml`'s version, which is the one thing mechanically true of the
/// program in front of the user — and the same one that is already stamped
/// into every file Cantara exports.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The copyright line, ending in the year this binary was built.
///
/// Why the build year and what that costs is explained at `build_year` in
/// `build.rs`; the short of it is that a development build carried across New
/// Year keeps the old year until something else rebuilds it.
pub fn copyright() -> String {
    format!("© {FIRST_YEAR}–{BUILD_YEAR} {AUTHOR}")
}

/// The about prose for a locale, falling back to English.
///
/// `locale` is whatever the system gave — `de`, `de-DE`, `en-GB`, `pt_BR`.
/// Only the primary subtag decides, so every German-speaking region reads the
/// same text; there is no sense in which Austrian German would want a
/// different description of the program, and a per-region file is a
/// translation nobody would ever fill in.
///
/// English is always there: `build.rs` refuses to build without it.
pub fn text_for(locale: &str) -> &'static str {
    let language = primary_subtag(locale);

    find(&language)
        .or_else(|| find("en"))
        .unwrap_or("")
}

/// The language part of a locale tag, lower-cased.
///
/// Both separators are accepted: `get_locale` gives `de-DE` on every platform
/// Cantara runs on, but a `LANG` of `de_DE.UTF-8` is what a Unix shell hands
/// out, and reading that as a language called `de_de` would silently fall back
/// to English.
fn primary_subtag(locale: &str) -> String {
    locale
        .split(['-', '_', '.'])
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase()
}

fn find(language: &str) -> Option<&'static str> {
    ABOUT_TEXTS
        .iter()
        .find(|(known, _)| *known == language)
        .map(|(_, text)| *text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the fallback: a language nobody has written a text
    /// for still gets a page with something on it, rather than an empty one
    /// that looks like a rendering failure.
    #[test]
    fn a_language_nobody_has_written_for_reads_english() {
        assert_eq!(text_for("pt-BR"), text_for("en"));
    }

    /// Regions share a language. `de-DE` and `de-AT` are the same text, and
    /// the bare tag is too — the file is named `cantara-info-de.md`, so a
    /// lookup for `de-DE` that did not strip the region would find nothing.
    #[test]
    fn every_region_of_a_language_reads_that_language() {
        assert_eq!(text_for("de-DE"), text_for("de"));
        assert_eq!(text_for("de-AT"), text_for("de"));
        assert_ne!(text_for("de"), text_for("en"));
    }

    /// A `LANG` from a Unix shell looks like `de_DE.UTF-8`, and reading that
    /// whole as the language would quietly serve English to every German
    /// desktop that set it.
    #[test]
    fn a_unix_style_locale_is_read_as_a_language_too() {
        assert_eq!(text_for("de_DE.UTF-8"), text_for("de"));
    }

    /// Case is not a language. A `LANG` of `DE` is German.
    #[test]
    fn the_case_of_the_tag_does_not_decide_the_language() {
        assert_eq!(text_for("DE"), text_for("de"));
    }

    /// Nothing at all is still a page: the locale can be absent, and the
    /// caller substitutes a default, but an empty string must not reach the
    /// reader as an empty page.
    #[test]
    fn an_unusable_locale_still_gets_a_text() {
        assert_eq!(text_for(""), text_for("en"));
        assert!(!text_for("").is_empty());
    }

    /// The copyright says when the work began and when this binary was built,
    /// and those are two different numbers with two different reasons.
    #[test]
    fn the_copyright_runs_from_the_first_release_to_the_build() {
        let line = copyright();

        assert!(line.contains(&FIRST_YEAR.to_string()), "got {line}");
        assert!(line.contains(&BUILD_YEAR.to_string()), "got {line}");
        assert!(line.contains(AUTHOR), "got {line}");
    }

    // That the build year is not before the first release is checked while
    // compiling — see the `const _` beside the constants. A test here would
    // be the same assertion, made later and to less effect.

    /// The version is the crate's, not a second number kept by hand.
    #[test]
    fn the_version_is_the_one_the_crate_was_built_with() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        assert!(!version().is_empty());
    }
}
