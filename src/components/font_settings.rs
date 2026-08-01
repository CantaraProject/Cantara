//! This module contains the functions for changing the font settings as defined in the [FontRepresentation] struct.

use crate::components::shared_components::NumberedValidatedLengthInput;
use crate::logic::css::CssFontFamily;
use crate::logic::fonts::{self, FontFamily, FontSource};
use crate::logic::settings::{CssSize, FontRepresentation, HorizontalAlign};
use dioxus::logger::tracing;
use dioxus::prelude::*;
use rgb::RGB8;
use rust_i18n::t;

use crate::logic::conversions::*;

rust_i18n::i18n!("locales", fallback = "en");

/// A component which renders and provides the manipulation features for [FontRepresentation]s
#[component]
pub fn FontRepresentationsComponent(
    /// The font representation as a vector
    fonts: Vec<FontRepresentation>,

    /// The index of the font configuration for default spoilers
    spoiler_index: Option<Option<u16>>,

    /// The index of the font configuration for default meta-block
    meta_index: Option<Option<u16>>,

    /// The event which will be triggered if the given font representation has been changed by the user
    onchange: EventHandler<Vec<FontRepresentation>>,
) -> Element {
    let mut fonts = use_signal(|| fonts);
    let fonts_count = use_memo(move || fonts.len());

    rsx!(
        article {
            for (idx, font) in fonts().into_iter().enumerate() {
                SingleFontRepresentationComponent {
                    font: font,
                    is_primary: match idx {
                        0 => true,
                        _ => false
                    },
                    is_spoiler: spoiler_index == Some(Some(idx as u16)),
                    is_meta: meta_index == Some(Some(idx as u16)),
                    onchange: move |new_font| {
                        match fonts.write().get_mut(idx) {
                            Some(reference) => {
                                *reference = new_font;
                            },
                            None => tracing::error!("Error while overriding font.")
                        }
                        onchange.call(fonts());
                    }
                }

                // Add a horizontal line between fonts
                if idx < fonts_count() -1 {
                    hr { }
                }
            }
        }
    )
}

/// This component renders a single [FontRepresentation] and allows manipulation
#[component]
fn SingleFontRepresentationComponent(
    /// The font representation item
    font: FontRepresentation,

    /// An event which will be triggered when the font has been updated
    onchange: EventHandler<FontRepresentation>,

    /// Whether the font should be marked as primary
    is_primary: Option<bool>,

    /// Whether the font should be marked as spoiler font
    is_spoiler: Option<bool>,

    /// Whether the font should be marked as meta font
    is_meta: Option<bool>,
) -> Element {
    let mut font = use_signal(|| font);

    rsx!(
        if is_primary.unwrap_or(false) {
            div {
                class: "badge",
                { t!("settings.fonts.primary_font").to_string() }
            }
        }
        else if let Some(true) = is_spoiler {
            div {
                class: "badge-2",
                { t!("settings.fonts.spoiler_font").to_string() }
            }
        }
        else if let Some(true) = is_meta {
            div {
                class: "badge-3",
                { t!("settings.fonts.meta_font").to_string() }
            }
        }
        else {
            div {
                class: "badge-inactive",
                { t!("settings.fonts.secondary_font").to_string() }
            }
        }

        form {
            FontFamilySelector {
                selected: font().font_family.clone(),
                onchange: move |new_family: Option<CssFontFamily>| {
                    font.write().font_family = new_family;
                    onchange.call(font());
                }
            }

            label {
                { t!("settings.fonts.size").to_string() }
                fieldset {
                    role: "group",
                    NumberedValidatedLengthInput {
                        value: font().font_size,
                        placeholder: "",
                        onchange: move |new_size: CssSize| {
                            font.write().font_size = new_size;
                            onchange.call(font());
                        }
                    }
                }
            }

            LineHeightInput {
                line_height: font().line_height,
                onchange: move |new_line_height| {
                    font.write().line_height = new_line_height;
                    onchange.call(font());
                }
            }

            fieldset {
                label {
                    { t!("settings.color").to_string() }
                    input {
                        type: "color",
                        value: font().color.to_hex(),
                        onchange: move |event| {
                            let new_color = event.value().to_rgb8().unwrap_or(RGB8::new(255,255,255));
                            font.write().color = new_color.into();
                            onchange.call(font());
                        }
                    }
                }
            }

            HorizontalAlignmentSelector {
                default: font().horizontal_alignment,
                onchange: move |new_align| {
                    font.write().horizontal_alignment = new_align;
                    onchange.call(font());
                }
            }
        }
    )
}

/// Picks the font family for one text section of a presentation.
///
/// The families are grouped by where they come from, because the distinction
/// matters: a font installed on this computer is not there when the same
/// presentation is opened on another machine or in a browser, while bundled and
/// web-safe families always are.
#[component]
fn FontFamilySelector(
    /// The family currently set, or `None` for the default.
    selected: Option<CssFontFamily>,

    /// Called with the new family whenever the selection changes.
    onchange: EventHandler<Option<CssFontFamily>>,
) -> Element {
    // Enumerating the installed fonts touches the file system, so it is done
    // once and kept for as long as the settings page lives.
    let families = use_hook(fonts::available);

    let current = selected
        .as_ref()
        .and_then(|family| family.family.clone())
        .unwrap_or_default();

    let group = |source: FontSource| -> Vec<FontFamily> {
        families
            .iter()
            .filter(|family| family.source == source)
            .cloned()
            .collect()
    };

    let groups = [
        (FontSource::Bundled, "settings.fonts.group_bundled"),
        (FontSource::WebSafe, "settings.fonts.group_websafe"),
        (FontSource::System, "settings.fonts.group_system"),
    ];

    rsx! {
        label {
            { t!("settings.fonts.family").to_string() }
            select {
                value: "{current}",
                onchange: move |event| {
                    let value = event.value();
                    if value.is_empty() {
                        onchange.call(None);
                    } else {
                        onchange.call(Some(CssFontFamily::with_family(value)));
                    }
                },
                // An empty value means "whatever the platform picks", which is
                // what a fresh design starts with.
                option { value: "", selected: current.is_empty(),
                    { t!("settings.fonts.family_default").to_string() }
                }
                for (source , label) in groups {
                    if !group(source).is_empty() {
                        optgroup { label: t!(label).to_string(),
                            for family in group(source) {
                                option {
                                    value: "{family.name}",
                                    selected: current == family.name,
                                    // Shown in its own font so the list can be
                                    // read at a glance.
                                    style: "font-family: '{family.name}';",
                                    "{family.name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An input field to change the line height
#[component]
fn LineHeightInput(
    line_height: f64,
    onchange: EventHandler<f64>,
) -> Element {
    rsx!(
        fieldset {
            label {
                { { format!("{}: {}", t!("settings.fonts.line_height"), line_height) } }
                input {
                    type: "range",
                    min: "1",
                    max: "2",
                    step: 0.1,
                    value: line_height,
                    onchange: move |event| {
                        let new_line_height = event.value().parse::<f64>().unwrap_or(1.0);
                        onchange.call(new_line_height);
                    }
                }
            }
        }
    )
}

/// A component for selecting the horizontal text alignment
#[component]
fn HorizontalAlignmentSelector(
    default: HorizontalAlign,
    onchange: EventHandler<HorizontalAlign>,
) -> Element {
    let mut value_signal = use_signal(|| default);
    rsx!(
        fieldset {
            label {
                { t!("settings.horizontal_alignment.title").to_string() }
                select {
                    name: "horizontal_align",
                    required: true,
                    aria_label: t!("settings.horizontal_alignment.title").to_string(),
                    onchange: move |event| {
                        let new_align = match event.value().as_str() {
                            "left" => HorizontalAlign::Left,
                            "centered" => HorizontalAlign::Centered,
                            "right" => HorizontalAlign::Right,
                            "justify" => HorizontalAlign::Justify,
                            "justify_with_hyphenation" => HorizontalAlign::JustifyWithHyphenation,
                            other => {
                                tracing::error!("Invalid option for horizontal alignment selected, the value is: {}", other);
                                HorizontalAlign::Centered
                            }
                        };
                        value_signal.set(new_align);
                        onchange.call(new_align);
                    },
                    option {
                        value: "left",
                        selected: value_signal() == HorizontalAlign::Left,
                        { t!("settings.horizontal_alignment.left").to_string() }
                    }
                    option {
                        value: "centered",
                        selected: value_signal() == HorizontalAlign::Centered,
                        { t!("settings.horizontal_alignment.centered").to_string() }
                    }
                    option {
                        value: "right",
                        selected: value_signal() == HorizontalAlign::Right,
                        { t!("settings.horizontal_alignment.right").to_string() }
                    }
                    option {
                        value: "justify",
                        selected: value_signal() == HorizontalAlign::Justify,
                        { t!("settings.horizontal_alignment.justify").to_string() }
                    }
                    option {
                        value: "justify_with_hyphenation",
                        selected: value_signal() == HorizontalAlign::JustifyWithHyphenation,
                        { t!("settings.horizontal_alignment.justify_with_hyphenation").to_string() }
                    }
                }
            }
        }
    )
}