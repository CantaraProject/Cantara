//! Saying in words what a slide division does.
//!
//! [`SlideSettings`] is a struct of seven fields, and the settings page used to
//! show exactly that: its `Debug` output, braces and all. What a user wants to
//! know before picking one is what it *does* — one language or three, a title
//! slide or not, where the metadata line goes — and that is what this builds.
//!
//! The sentences are not built here. Each line is a translation key with its
//! parameters, so this stays a decision about the settings rather than about
//! language, and can be tested without a view. See
//! [`crate::components::shared_components::translate`].

use cantara_songlib::slides::{LanguageConfiguration, SlideElement, SlideSettings};

/// One line of the summary: a translation key and what to put into it.
pub type SummaryLine = (&'static str, Vec<(&'static str, String)>);

/// What the division does, line by line, in the order a reader wants it.
///
/// The layout first — it decides what a slide even looks like — then what is
/// added around the lyrics, then the line wrap.
pub fn summary_lines(settings: &SlideSettings) -> Vec<SummaryLine> {
    let mut lines: Vec<SummaryLine> = Vec::new();

    lines.push(layout_line(&settings.language));

    if settings.title_slide {
        lines.push(("display.summary.title_slide", vec![]));
    }
    if settings.show_spoiler {
        lines.push(("display.summary.spoiler", vec![]));
    }
    if settings.empty_last_slide {
        lines.push(("display.summary.empty_last_slide", vec![]));
    }

    if let Some(positions) = meta_line(settings) {
        lines.push(positions);
    }

    lines.push(match settings.max_lines {
        Some(lines_per_slide) => (
            "display.summary.max_lines",
            vec![("count", lines_per_slide.to_string())],
        ),
        None => ("display.summary.whole_block", vec![]),
    });

    lines
}

/// The one-line answer to "what is on a slide".
fn layout_line(language: &LanguageConfiguration) -> SummaryLine {
    match language {
        LanguageConfiguration::SingleLanguage(None) => {
            ("display.summary.layout_single_default", vec![])
        }
        LanguageConfiguration::SingleLanguage(Some(language)) => (
            "display.summary.layout_single_named",
            vec![("language", language.clone())],
        ),
        LanguageConfiguration::MultiLanguage(languages) if languages.is_empty() => {
            ("display.summary.layout_single_default", vec![])
        }
        LanguageConfiguration::MultiLanguage(languages) => (
            "display.summary.layout_multi",
            vec![
                ("count", languages.len().to_string()),
                ("languages", languages.join(", ")),
            ],
        ),
        LanguageConfiguration::Complex(elements) => {
            let notation = elements
                .iter()
                .any(|element| matches!(element, SlideElement::Notation));
            let languages: Vec<String> = elements
                .iter()
                .filter_map(|element| match element {
                    SlideElement::Lyrics(language) => Some(language.clone()),
                    SlideElement::Notation => None,
                })
                .collect();

            match notation {
                true => (
                    "display.summary.layout_complex",
                    vec![
                        ("count", languages.len().to_string()),
                        ("languages", languages.join(", ")),
                    ],
                ),
                // A complex layout without the staff is a stack of languages
                // by another name, and saying "notation" would be wrong.
                false => (
                    "display.summary.layout_multi",
                    vec![
                        ("count", languages.len().to_string()),
                        ("languages", languages.join(", ")),
                    ],
                ),
            }
        }
    }
}

/// Where the metadata line appears, if it appears at all.
fn meta_line(settings: &SlideSettings) -> Option<SummaryLine> {
    let show = &settings.show_meta_information;
    let mut positions: Vec<&'static str> = Vec::new();
    if show.title_slide {
        positions.push("display.summary.meta_title");
    }
    if show.first_slide {
        positions.push("display.summary.meta_first");
    }
    if show.last_slide {
        positions.push("display.summary.meta_last");
    }

    if positions.is_empty() {
        return None;
    }

    Some((
        "display.summary.meta",
        vec![
            // The positions are keys of their own; the view translates each and
            // joins them, which is what keeps the list readable in a language
            // that orders them differently.
            ("positions", positions.join("\u{1f}")),
        ],
    ))
}

/// How the positions of the metadata line are packed into one parameter.
///
/// A unit separator rather than a comma: a translated position may well
/// contain one.
pub const POSITION_SEPARATOR: char = '\u{1f}';

#[cfg(test)]
mod tests {
    use super::*;
    use cantara_songlib::slides::ShowMetaInformation;

    fn keys(settings: &SlideSettings) -> Vec<&'static str> {
        summary_lines(settings)
            .into_iter()
            .map(|(key, _)| key)
            .collect()
    }

    /// The default division is what most users see, and every part of it has
    /// to be accounted for: what a slide holds, what is added around it, and
    /// how much goes on one.
    #[test]
    fn the_default_division_is_described_in_full() {
        let keys = keys(&SlideSettings::default());

        assert_eq!(
            keys,
            vec![
                "display.summary.layout_single_default",
                "display.summary.title_slide",
                "display.summary.spoiler",
                "display.summary.empty_last_slide",
                "display.summary.meta",
                "display.summary.whole_block",
            ]
        );
    }

    /// What is switched off is not mentioned: a list of "no, no, no" says less
    /// than a short list of what the division actually does.
    #[test]
    fn what_is_off_is_left_out() {
        let settings = SlideSettings {
            title_slide: false,
            show_spoiler: false,
            empty_last_slide: false,
            show_meta_information: ShowMetaInformation {
                title_slide: false,
                first_slide: false,
                last_slide: false,
            },
            ..SlideSettings::default()
        };

        assert_eq!(
            keys(&settings),
            vec![
                "display.summary.layout_single_default",
                "display.summary.whole_block",
            ]
        );
    }

    /// The languages are the point of a multi-language layout, so they are in
    /// the line rather than only their number.
    #[test]
    fn the_languages_are_named() {
        let settings = SlideSettings {
            language: LanguageConfiguration::MultiLanguage(vec![
                "de".to_string(),
                "en".to_string(),
            ]),
            ..SlideSettings::default()
        };

        let (key, parameters) = summary_lines(&settings).remove(0);
        assert_eq!(key, "display.summary.layout_multi");
        assert_eq!(parameters[0], ("count", "2".to_string()));
        assert_eq!(parameters[1], ("languages", "de, en".to_string()));
    }

    /// A complex layout is only complex while it has a staff in it; without
    /// one it is a stack of languages and must not claim otherwise.
    #[test]
    fn a_complex_layout_without_a_staff_is_not_called_notation() {
        let with_staff = SlideSettings {
            language: LanguageConfiguration::Complex(vec![
                SlideElement::Notation,
                SlideElement::Lyrics("de".to_string()),
            ]),
            ..SlideSettings::default()
        };
        let without_staff = SlideSettings {
            language: LanguageConfiguration::Complex(vec![SlideElement::Lyrics("de".to_string())]),
            ..SlideSettings::default()
        };

        assert_eq!(keys(&with_staff)[0], "display.summary.layout_complex");
        assert_eq!(keys(&without_staff)[0], "display.summary.layout_multi");
    }

    /// A line wrap is worth saying out loud — it is the setting that decides
    /// how often the congregation is asked to look up.
    #[test]
    fn the_line_wrap_is_stated_either_way() {
        let wrapped = SlideSettings {
            max_lines: Some(4),
            ..SlideSettings::default()
        };

        let (key, parameters) = summary_lines(&wrapped).pop().expect("a last line");
        assert_eq!(key, "display.summary.max_lines");
        assert_eq!(parameters[0], ("count", "4".to_string()));

        assert_eq!(
            summary_lines(&SlideSettings::default()).pop().map(|(key, _)| key),
            Some("display.summary.whole_block")
        );
    }

    /// Every line the summary can produce has to have something to say in it.
    /// `rust_i18n` answers a missing key with the key itself, which is what
    /// the settings page was showing before this module existed.
    #[test]
    fn every_line_has_a_translation() {
        let divisions = [
            SlideSettings::default(),
            SlideSettings {
                max_lines: Some(4),
                language: LanguageConfiguration::MultiLanguage(vec!["de".to_string()]),
                ..SlideSettings::default()
            },
            SlideSettings {
                language: LanguageConfiguration::SingleLanguage(Some("de".to_string())),
                ..SlideSettings::default()
            },
            SlideSettings {
                language: LanguageConfiguration::Complex(vec![SlideElement::Notation]),
                show_meta_information: ShowMetaInformation {
                    title_slide: true,
                    first_slide: true,
                    last_slide: true,
                },
                ..SlideSettings::default()
            },
        ];

        for division in divisions {
            for (key, parameters) in summary_lines(&division) {
                assert_ne!(
                    rust_i18n::t!(key),
                    key,
                    "{key} has no translation"
                );
                for (name, value) in parameters {
                    if name != "positions" {
                        continue;
                    }
                    for position in value.split(POSITION_SEPARATOR) {
                        assert_ne!(
                            rust_i18n::t!(position),
                            position,
                            "{position} has no translation"
                        );
                    }
                }
            }
        }
    }

    /// Where the metadata line goes is one line naming the places, so that
    /// three switches do not become three sentences.
    #[test]
    fn the_metadata_positions_are_one_line() {
        let settings = SlideSettings {
            show_meta_information: ShowMetaInformation {
                title_slide: true,
                first_slide: false,
                last_slide: true,
            },
            ..SlideSettings::default()
        };

        let (_, parameters) = summary_lines(&settings)
            .into_iter()
            .find(|(key, _)| *key == "display.summary.meta")
            .expect("the metadata line");

        let positions: Vec<&str> = parameters[0].1.split(POSITION_SEPARATOR).collect();
        assert_eq!(
            positions,
            vec!["display.summary.meta_title", "display.summary.meta_last"]
        );
    }
}
