//! This module contains the functions for changing the font settings as defined in the [FontRepresentation] struct.

use crate::components::shared_components::NumberedValidatedLengthInput;
use crate::logic::css::CssFontFamily;
use crate::logic::fonts::{self, FontFamily, FontSource};
use crate::logic::settings::{CssSize, FontOutline, FontRepresentation, HorizontalAlign};
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
    // Driven by the prop rather than by a copy taken on the first render: the
    // design is owned by the settings, and a copy here would go stale the
    // moment anything else touched it — which is why the preview stopped
    // following the font settings.
    let fonts_count = fonts.len();

    rsx!(
        article {
            for (idx, font) in fonts.clone().into_iter().enumerate() {
                SingleFontRepresentationComponent {
                    key: "{idx}",
                    font: font,
                    is_primary: idx == 0,
                    is_spoiler: spoiler_index == Some(Some(idx as u16)),
                    is_meta: meta_index == Some(Some(idx as u16)),
                    // The first three blocks are the main, spoiler and meta
                    // text of every slide; removing one would leave the design
                    // without a way to draw part of a slide.
                    removable: idx >= 3,
                    onchange: {
                        let fonts = fonts.clone();
                        move |new_font| {
                            let mut updated = fonts.clone();
                            if let Some(reference) = updated.get_mut(idx) {
                                *reference = new_font;
                                onchange.call(updated);
                            } else {
                                tracing::error!("Error while overriding font.");
                            }
                        }
                    },
                    onremove: {
                        let fonts = fonts.clone();
                        move |_| {
                            let mut updated = fonts.clone();
                            if idx < updated.len() {
                                updated.remove(idx);
                                onchange.call(updated);
                            }
                        }
                    },
                }

                // Add a horizontal line between fonts
                if idx + 1 < fonts_count {
                    hr { }
                }
            }

            // A complex presentation can show any number of languages, so the
            // design has to be able to grow a block for each of them.
            button {
                r#type: "button",
                class: "outline",
                onclick: {
                    let fonts = fonts.clone();
                    move |_| {
                        let mut updated = fonts.clone();
                        updated.push(FontRepresentation::default());
                        onchange.call(updated);
                    }
                },
                { t!("settings.fonts.add_block").to_string() }
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

    /// Called when the user removes this block.
    onremove: EventHandler<()>,

    /// Whether the font should be marked as primary
    is_primary: bool,

    /// Whether the font should be marked as spoiler font
    is_spoiler: bool,

    /// Whether the font should be marked as meta font
    is_meta: bool,

    /// Whether this block may be removed.
    removable: bool,
) -> Element {
    // The block is driven by its prop: a copy kept here would freeze at the
    // value of the first render, so anything that changed the design elsewhere
    // would stop showing up. Each handler therefore clones the block it edits —
    // one shared copy could only be moved into a single closure.

    let label = if is_primary {
        t!("settings.fonts.primary_font").to_string()
    } else if is_spoiler {
        t!("settings.fonts.spoiler_font").to_string()
    } else if is_meta {
        t!("settings.fonts.meta_font").to_string()
    } else if let Some(language) = font.language.as_deref().filter(|code| !code.is_empty()) {
        t!("settings.fonts.language_font", language = language).to_string()
    } else {
        t!("settings.fonts.secondary_font").to_string()
    };

    rsx!(
        div { class: "font-block-header",
            // The badge is drawn in the block's own colours, so the list reads
            // as a set of samples rather than a set of identical chips.
            div { class: "font-block-badge", style: "{badge_style(&font)}", "{label}" }

            if removable {
                button {
                    r#type: "button",
                    class: "outline secondary font-block-remove",
                    aria_label: t!("settings.fonts.remove_block").to_string(),
                    onclick: move |_| onremove.call(()),
                    "✕"
                }
            }
        }

        form {
            FontFamilySelector {
                selected: font.font_family.clone(),
                onchange: {
                    let base = font.clone();
                    move |new_family: Option<CssFontFamily>| {
                        let mut updated = base.clone();
                        updated.font_family = new_family;
                        onchange.call(updated);
                    }
                }
            }

            label {
                { t!("settings.fonts.size").to_string() }
                fieldset {
                    role: "group",
                    NumberedValidatedLengthInput {
                        value: font.font_size.clone(),
                        placeholder: "",
                        onchange: {
                            let base = font.clone();
                            move |new_size: CssSize| {
                                let mut updated = base.clone();
                                updated.font_size = new_size;
                                onchange.call(updated);
                            }
                        }
                    }
                }
            }

            LineHeightInput {
                line_height: font.line_height,
                onchange: {
                    let base = font.clone();
                    move |new_line_height: f64| {
                        let mut updated = base.clone();
                        updated.line_height = new_line_height;
                        onchange.call(updated);
                    }
                }
            }

            fieldset {
                label {
                    { t!("settings.color").to_string() }
                    input {
                        r#type: "color",
                        value: font.color.to_hex(),
                        // `oninput` rather than `onchange`: the colour picker
                        // only commits on close, so the preview would not
                        // follow while a colour is being chosen.
                        oninput: {
                            let base = font.clone();
                            move |event| {
                                let mut updated = base.clone();
                            let new_color = event.value().to_rgb8().unwrap_or(RGB8::new(255,255,255));
                                    updated.color = new_color.into();
                                onchange.call(updated);
                            }
                        }
                    }
                }
            }

            WeightSelector {
                weight: font.weight,
                onchange: {
                    let base = font.clone();
                    move |new_weight: u16| {
                        let mut updated = base.clone();
                        updated.weight = new_weight;
                        onchange.call(updated);
                    }
                }
            }

            HorizontalAlignmentSelector {
                default: font.horizontal_alignment,
                onchange: {
                    let base = font.clone();
                    move |new_align: HorizontalAlign| {
                        let mut updated = base.clone();
                        updated.horizontal_alignment = new_align;
                        onchange.call(updated);
                    }
                }
            }

            // An outline keeps light text legible over a busy photograph
            // without having to darken the whole background.
            fieldset {
                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: font.outline.is_some(),
                        onchange: {
                            let base = font.clone();
                            move |event: Event<FormData>| {
                                let mut updated = base.clone();
                                updated.outline = if event.checked() {
                                    Some(FontOutline::default())
                                } else {
                                    None
                                };
                                onchange.call(updated);
                            }
                        }
                    }
                    { t!("settings.fonts.outline").to_string() }
                }

                if let Some(outline) = font.outline {
                    div { class: "grid",
                        label {
                            { t!("settings.color").to_string() }
                            input {
                                r#type: "color",
                                value: outline.color.to_hex(),
                                oninput: {
                                    let base = font.clone();
                                    move |event: Event<FormData>| {
                                        let color = event.value().to_rgb8().unwrap_or(RGB8::new(0, 0, 0));
                                        let mut updated = base.clone();
                                        if let Some(o) = updated.outline.as_mut() {
                                            o.color = color.into();
                                        }
                                        onchange.call(updated);
                                    }
                                }
                            }
                        }
                        label {
                            { format!("{}: {}", t!("settings.fonts.outline_width"), outline.width) }
                            input {
                                r#type: "range",
                                min: "0.5", max: "4", step: "0.5",
                                value: "{outline.width}",
                                oninput: {
                                    let base = font.clone();
                                    move |event: Event<FormData>| {
                                        let width = event.value().parse::<f64>().unwrap_or(1.0);
                                        let mut updated = base.clone();
                                        if let Some(o) = updated.outline.as_mut() {
                                            o.width = width;
                                        }
                                        onchange.call(updated);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            fieldset {
                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: font.shadow,
                        onchange: {
                            let base = font.clone();
                            move |event: Event<FormData>| {
                                let mut updated = base.clone();
                                updated.shadow = event.checked();
                                onchange.call(updated);
                            }
                        }
                    }
                    { t!("settings.fonts.shadow").to_string() }
                }

                if font.shadow {
                    div { class: "grid",
                        label {
                            { t!("settings.color").to_string() }
                            input {
                                r#type: "color",
                                value: font.shadow_style.color.to_hex(),
                                oninput: {
                                    let base = font.clone();
                                    move |event: Event<FormData>| {
                                        let color = event.value().to_rgb8().unwrap_or(RGB8::new(0, 0, 0));
                                        let mut updated = base.clone();
                                        updated.shadow_style.color = color.into();
                                        onchange.call(updated);
                                    }
                                }
                            }
                        }
                        label {
                            { format!("{}: {}", t!("settings.fonts.shadow_blur"), font.shadow_style.blur) }
                            input {
                                r#type: "range",
                                min: "0", max: "24", step: "1",
                                value: "{font.shadow_style.blur}",
                                oninput: {
                                    let base = font.clone();
                                    move |event: Event<FormData>| {
                                        let blur = event.value().parse::<f64>().unwrap_or(6.0);
                                        let mut updated = base.clone();
                                        updated.shadow_style.blur = blur;
                                        onchange.call(updated);
                                    }
                                }
                            }
                        }
                        label {
                            { format!("{}: {}", t!("settings.fonts.shadow_offset"), font.shadow_style.offset_x) }
                            input {
                                r#type: "range",
                                min: "-10", max: "10", step: "1",
                                value: "{font.shadow_style.offset_x}",
                                oninput: {
                                    let base = font.clone();
                                    move |event: Event<FormData>| {
                                        let offset = event.value().parse::<f64>().unwrap_or(2.0);
                                        let mut updated = base.clone();
                                        updated.shadow_style.offset_x = offset;
                                        updated.shadow_style.offset_y = offset;
                                        onchange.call(updated);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Only a block beyond the three fixed ones can take a language:
            // the main, spoiler and meta blocks are used on every slide.
            if !is_primary && !is_spoiler && !is_meta {
                label {
                    { t!("settings.fonts.language").to_string() }
                    input {
                        r#type: "text",
                        placeholder: "de",
                        value: font.language.clone().unwrap_or_default(),
                        oninput: {
                            let base = font.clone();
                            move |event: Event<FormData>| {
                                let code = event.value().trim().to_string();
                                let mut updated = base.clone();
                                updated.language = if code.is_empty() { None } else { Some(code) };
                                onchange.call(updated);
                            }
                        }
                    }
                    small { { t!("settings.fonts.language_hint").to_string() } }
                }
            }
        }
    )
}

/// The badge above a font block, drawn in that block's own colours.
///
/// The block's colour becomes the background and its family the lettering, so
/// the badge doubles as a sample. The text is black or white, whichever stands
/// out against that background — a fixed colour would disappear on half the
/// designs.
fn badge_style(font: &FontRepresentation) -> String {
    let background = font.color;
    let foreground = if is_light(background) { "#000" } else { "#fff" };

    let family = font
        .font_family
        .as_ref()
        .and_then(|family| family.family.clone())
        .filter(|family| !family.trim().is_empty())
        .unwrap_or_else(|| "inherit".to_string());

    format!(
        "background-color: rgb({}, {}, {}); color: {}; font-family: {};",
        background.r, background.g, background.b, foreground, family
    )
}

/// Whether black text reads better than white on this colour.
///
/// Uses the relative luminance of the sRGB channels rather than a plain
/// average: the eye is far more sensitive to green than to blue, so an average
/// would call a saturated blue "light" and put black on it.
fn is_light(color: rgb::RGBA8) -> bool {
    let luminance = 0.2126 * color.r as f64 + 0.7152 * color.g as f64 + 0.0722 * color.b as f64;
    luminance > 140.0
}

/// Picks how heavy the type is drawn.
#[component]
fn WeightSelector(weight: u16, onchange: EventHandler<u16>) -> Element {
    let options: [(u16, &str); 4] = [
        (300, "settings.fonts.weight_light"),
        (400, "settings.fonts.weight_regular"),
        (600, "settings.fonts.weight_semibold"),
        (700, "settings.fonts.weight_bold"),
    ];

    rsx! {
        label {
            { t!("settings.fonts.weight").to_string() }
            select {
                onchange: move |event| {
                    onchange.call(event.value().parse::<u16>().unwrap_or(400));
                },
                for (value , key) in options {
                    option {
                        value: "{value}",
                        selected: weight == value,
                        { t!(key).to_string() }
                    }
                }
            }
        }
    }
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
#[cfg(test)]
mod badge_tests {
    use super::*;
    use rgb::RGBA8;

    /// A badge drawn in the block's colour needs lettering that survives it —
    /// a fixed colour would disappear on half the designs.
    #[test]
    fn test_the_badge_text_contrasts_with_the_block_colour() {
        let dark = FontRepresentation {
            color: RGBA8::new(20, 20, 20, 255),
            ..FontRepresentation::default()
        };
        let light = FontRepresentation {
            color: RGBA8::new(240, 240, 240, 255),
            ..FontRepresentation::default()
        };

        assert!(badge_style(&dark).contains("color: #fff"));
        assert!(badge_style(&light).contains("color: #000"));
    }

    /// Luminance, not a plain average: the eye barely registers blue, so an
    /// average would call a saturated blue light and put black on it.
    #[test]
    fn test_saturated_blue_counts_as_dark() {
        let blue = FontRepresentation {
            color: RGBA8::new(0, 0, 255, 255),
            ..FontRepresentation::default()
        };
        // A plain average of (0 + 0 + 255) / 3 = 85 would already read as dark,
        // but green is the telling channel: pure green must read as light.
        let green = FontRepresentation {
            color: RGBA8::new(0, 255, 0, 255),
            ..FontRepresentation::default()
        };

        assert!(badge_style(&blue).contains("color: #fff"));
        assert!(badge_style(&green).contains("color: #000"));
    }

    /// The badge doubles as a sample, so it has to carry the block's own
    /// colour and family.
    #[test]
    fn test_the_badge_shows_the_block_colour_and_family() {
        let font = FontRepresentation {
            color: RGBA8::new(12, 34, 56, 255),
            font_family: Some(CssFontFamily::with_family("Georgia".to_string())),
            ..FontRepresentation::default()
        };

        let style = badge_style(&font);

        assert!(style.contains("background-color: rgb(12, 34, 56)"));
        assert!(style.contains("font-family: Georgia"));
    }

    /// A block with no family set must not produce a broken declaration.
    #[test]
    fn test_a_block_without_a_family_inherits_one() {
        let style = badge_style(&FontRepresentation::default());

        assert!(style.contains("font-family: inherit"));
    }
}
