//! Reading one source's tag names as another's.
//!
//! Songs arrive from many places and none of them agree on what to call
//! things. A file from one collection records the person who wrote the words
//! under `author`, the next one under `writer`, a third under `poet`. A meta
//! line built from `{{composer}}` then stays empty for two of the three, and
//! the only remedy so far was to edit every file by hand.
//!
//! A tag mapping says "where this song has `author`, read it as `composer`".
//!
//! # What it does not do
//!
//! It never changes a song, and never changes a file. The mapping is applied
//! to a copy on its way to the slides; the file on disk keeps the words its
//! author wrote, and the detail view keeps showing them. That is the whole
//! reason this is a *reading* rule and not a migration: a collection shared
//! between two people, each with their own mappings, still has one text.
//!
//! # The three rules
//!
//! 1. **A tag that is already there wins.** A mapping fills a gap; it never
//!    overwrites. If a song has both `author` and `composer`, the mapping
//!    `author` → `composer` changes nothing, because nothing was missing.
//! 2. **One step, never a chain.** Every mapping reads the song's *original*
//!    tags. With `author` → `composer` and `composer` → `arranger`, a song
//!    that only has `author` gets a `composer` and no `arranger`. Chaining
//!    would make the result depend on the order of a list the user thinks of
//!    as a set, and would need a cycle check to boot — `a` → `b` together with
//!    `b` → `a` is a perfectly reasonable thing to write down, and here it
//!    simply means both names are readable as each other.
//! 3. **The first mapping to fill a target wins.** Two mappings pointing at
//!    the same target are not an error: `author` → `composer` and `writer` →
//!    `composer` is exactly how one unifies three collections. They are tried
//!    in the order the user listed them.
//!
//! Names are matched without regard to case or surrounding space, so a file
//! writing `Author:` is reached by a mapping written as `author`.

use cantara_songlib::song::Song;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One rule: where a song has [`from`](Self::from), let it also be read as
/// [`to`](Self::to).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct TagMapping {
    /// The name the song file uses.
    pub from: String,

    /// The name the slide template asks for.
    pub to: String,
}

impl TagMapping {
    /// Build a mapping.
    pub fn new(from: &str, to: &str) -> TagMapping {
        TagMapping {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    /// Why this rule cannot be used, as a translation key — or `None` when it
    /// is fine.
    ///
    /// Kept beside the rule rather than in the settings page so that the same
    /// answer reaches the list, the row being edited and the tests. A rule
    /// that is not usable is not an error to be reported anywhere: the
    /// settings simply mark the row and [`apply`] passes it over, so a
    /// half-typed rule never changes what is on screen.
    pub fn problem(&self) -> Option<&'static str> {
        let from = normalise(&self.from);
        let to = normalise(&self.to);

        if from.is_empty() || to.is_empty() {
            return Some("settings.tag_mapping_incomplete");
        }
        if from == to {
            return Some("settings.tag_mapping_circular");
        }
        None
    }

    /// Whether the rule is complete enough to be applied.
    pub fn is_usable(&self) -> bool {
        self.problem().is_none()
    }
}

/// The form a tag name is compared in.
///
/// One collection writes `Author`, the next ` author `. Neither should need
/// its own rule, so the comparison ignores case and the space around the name.
fn normalise(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The tags of a song as the slides should read them.
///
/// The song's own tags are all there, unchanged; the mappings only ever add.
/// See the [module documentation](self) for the three rules this follows.
pub fn mapped_tags(
    tags: &BTreeMap<String, String>,
    mappings: &[TagMapping],
) -> BTreeMap<String, String> {
    // What the song itself says, by normalised name. Every mapping reads from
    // here and never from the growing result, which is what keeps one mapping
    // from feeding the next.
    let original: BTreeMap<String, &String> = tags
        .iter()
        .map(|(name, value)| (normalise(name), value))
        .collect();

    let mut mapped = tags.clone();
    let mut filled: Vec<String> = Vec::new();

    for mapping in mappings.iter().filter(|mapping| mapping.is_usable()) {
        let to = normalise(&mapping.to);

        // Rule 1: the song's own tag wins. Rule 3: so does an earlier mapping.
        if original.contains_key(&to) || filled.contains(&to) {
            continue;
        }

        let Some(value) = original.get(&normalise(&mapping.from)) else {
            continue;
        };

        // The target is written as the user spelled it in the rule: that is
        // the name their template asks for.
        mapped.insert(mapping.to.trim().to_string(), (*value).clone());
        filled.push(to);
    }

    mapped
}

/// The song as the slides should read it.
///
/// A copy — the song that was passed in is untouched, and so is the file it
/// came from.
pub fn apply(song: &Song, mappings: &[TagMapping]) -> Song {
    // Nothing configured is the normal case; it should cost nothing.
    if mappings.is_empty() {
        return song.clone();
    }

    let mapped = mapped_tags(song.tags(), mappings);
    if mapped.len() == song.tags().len() {
        return song.clone();
    }

    let mut result = song.clone();
    for (name, value) in mapped {
        result.set_tag(&name, &value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn a_missing_target_is_filled_from_the_source() {
        let mapped = mapped_tags(
            &tags(&[("author", "John Newton")]),
            &[TagMapping::new("author", "composer")],
        );

        assert_eq!(mapped.get("composer").map(String::as_str), Some("John Newton"));
    }

    /// The song's own words are never replaced by a rule.
    #[test]
    fn an_existing_target_is_left_alone() {
        let mapped = mapped_tags(
            &tags(&[("author", "John Newton"), ("composer", "William Walker")]),
            &[TagMapping::new("author", "composer")],
        );

        assert_eq!(
            mapped.get("composer").map(String::as_str),
            Some("William Walker")
        );
    }

    #[test]
    fn the_source_tag_survives() {
        let mapped = mapped_tags(
            &tags(&[("author", "John Newton")]),
            &[TagMapping::new("author", "composer")],
        );

        assert_eq!(mapped.get("author").map(String::as_str), Some("John Newton"));
        assert_eq!(mapped.len(), 2);
    }

    /// Rule 2. `author` becomes readable as `composer`, but that new reading is
    /// not itself a source for the next rule.
    #[test]
    fn mappings_do_not_chain() {
        let mapped = mapped_tags(
            &tags(&[("author", "John Newton")]),
            &[
                TagMapping::new("author", "composer"),
                TagMapping::new("composer", "arranger"),
            ],
        );

        assert_eq!(mapped.get("composer").map(String::as_str), Some("John Newton"));
        assert_eq!(mapped.get("arranger"), None);
    }

    /// A consequence of rule 2 that would otherwise need guarding against:
    /// two names declared readable as each other is a sensible thing to write.
    #[test]
    fn a_cycle_is_harmless() {
        let mapped = mapped_tags(
            &tags(&[("author", "John Newton")]),
            &[
                TagMapping::new("author", "composer"),
                TagMapping::new("composer", "author"),
            ],
        );

        assert_eq!(mapped.get("author").map(String::as_str), Some("John Newton"));
        assert_eq!(mapped.get("composer").map(String::as_str), Some("John Newton"));
        assert_eq!(mapped.len(), 2);
    }

    /// Rule 3: three collections, three names for the same thing, one target.
    #[test]
    fn the_first_rule_to_reach_a_target_wins() {
        let mappings = [
            TagMapping::new("author", "composer"),
            TagMapping::new("writer", "composer"),
        ];

        let from_second = mapped_tags(&tags(&[("writer", "Anna")]), &mappings);
        assert_eq!(from_second.get("composer").map(String::as_str), Some("Anna"));

        let both = mapped_tags(&tags(&[("author", "Bea"), ("writer", "Anna")]), &mappings);
        assert_eq!(both.get("composer").map(String::as_str), Some("Bea"));
    }

    #[test]
    fn names_are_matched_without_case_or_space() {
        let mapped = mapped_tags(
            &tags(&[("Author", "John Newton")]),
            &[TagMapping::new("  author ", " composer")],
        );

        assert_eq!(mapped.get("composer").map(String::as_str), Some("John Newton"));
    }

    #[test]
    fn a_rule_without_a_match_changes_nothing() {
        let original = tags(&[("author", "John Newton")]);
        let mapped = mapped_tags(&original, &[TagMapping::new("poet", "composer")]);

        assert_eq!(mapped, original);
    }

    /// A row the user is still typing must not move anything on the screen.
    #[test]
    fn half_written_rules_are_passed_over() {
        let original = tags(&[("author", "John Newton")]);

        for rule in [
            TagMapping::new("author", ""),
            TagMapping::new("", "composer"),
            TagMapping::new("   ", "composer"),
            TagMapping::new("author", "AUTHOR"),
        ] {
            assert!(rule.problem().is_some(), "{rule:?} should be rejected");
            assert_eq!(mapped_tags(&original, &[rule]), original);
        }
    }

    #[test]
    fn a_usable_rule_has_no_problem() {
        assert_eq!(TagMapping::new("author", "composer").problem(), None);
        assert!(TagMapping::new("author", "composer").is_usable());
    }

    #[test]
    fn the_song_that_goes_in_is_not_touched() {
        let mut song = Song::new("Amazing Grace");
        song.set_tag("author", "John Newton");

        let mapped = apply(&song, &[TagMapping::new("author", "composer")]);

        assert_eq!(song.tag("composer"), None, "the original gained a tag");
        assert_eq!(
            mapped.tag("composer").map(String::as_str),
            Some("John Newton")
        );
        assert_eq!(mapped.title, song.title);
    }

    #[test]
    fn no_mappings_means_the_same_song() {
        let mut song = Song::new("Amazing Grace");
        song.set_tag("author", "John Newton");

        assert_eq!(apply(&song, &[]), song);
    }
}
