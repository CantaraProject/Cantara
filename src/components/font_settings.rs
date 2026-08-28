//! This module contains the functions for changing the font settings as defined in the [FontRepresentation] struct.

use crate::components::shared_components::{NumberedValidatedLengthInput, RangeInput};
use crate::logic::css::CssFontFamily;
use crate::logic::fonts::{self, FontFamily, FontSource};
use crate::logic::settings::{CssSize, FontOutline, FontRepresentation, HorizontalAlign};
use dioxus::logger::tracing;
use dioxus::prelude::*;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use rgb::RGB8;
use rust_i18n::t;
use std::sync::Arc;

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

            // Bold and italic together, the way every program that sets type
            // offers them, with the full weight list under them for anyone who
            // wants light or semibold. The bold switch is a view of the weight
            // — see [`FontRepresentation::is_bold`] — so there is still only
            // one thing stored and nothing that can fall out of step.
            fieldset {
                legend { { t!("settings.fonts.style").to_string() } }
                div { class: "grid",
                    label {
                        input {
                            r#type: "checkbox",
                            role: "switch",
                            checked: font.is_bold(),
                            onchange: {
                                let base = font.clone();
                                move |event: Event<FormData>| {
                                    let mut updated = base.clone();
                                    updated.set_bold(event.checked());
                                    onchange.call(updated);
                                }
                            }
                        }
                        { t!("settings.fonts.bold").to_string() }
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            role: "switch",
                            checked: font.italic,
                            onchange: {
                                let base = font.clone();
                                move |event: Event<FormData>| {
                                    let mut updated = base.clone();
                                    updated.italic = event.checked();
                                    onchange.call(updated);
                                }
                            }
                        }
                        { t!("settings.fonts.italic").to_string() }
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
                        RangeInput {
                            label: t!("settings.fonts.outline_width").to_string(),
                            min: 0.5,
                            max: 4.0,
                            step: 0.5,
                            value: outline.width,
                            onchange: {
                                let base = font.clone();
                                move |width: f64| {
                                    let mut updated = base.clone();
                                    if let Some(o) = updated.outline.as_mut() {
                                        o.width = width;
                                    }
                                    onchange.call(updated);
                                }
                            },
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
                        RangeInput {
                            label: t!("settings.fonts.shadow_blur").to_string(),
                            min: 0.0,
                            max: 24.0,
                            step: 1.0,
                            value: font.shadow_style.blur,
                            onchange: {
                                let base = font.clone();
                                move |blur: f64| {
                                    let mut updated = base.clone();
                                    updated.shadow_style.blur = blur;
                                    onchange.call(updated);
                                }
                            },
                        }
                        RangeInput {
                            label: t!("settings.fonts.shadow_offset").to_string(),
                            min: -10.0,
                            max: 10.0,
                            step: 1.0,
                            value: font.shadow_style.offset_x,
                            onchange: {
                                let base = font.clone();
                                move |offset: f64| {
                                    let mut updated = base.clone();
                                    updated.shadow_style.offset_x = offset;
                                    updated.shadow_style.offset_y = offset;
                                    onchange.call(updated);
                                }
                            },
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

/// The families that can be offered, and whether the installed ones are still
/// on their way.
///
/// Reading the installed fonts opens every font file on the computer. Doing
/// that here, while the page renders, is what made the design editor take
/// seconds to open: it happened once per font block, and again when the page
/// change had settled and the page was built a second time. Now the list is
/// read once per run on a background thread; until it is there the selector
/// offers the bundled and web-safe families, says so, and gains the rest when
/// they arrive. See [`crate::logic::fonts`].
///
/// The list is read fresh on every render and is cheap to read — see
/// [`fonts::available_now`], which hands out a shared copy. It is deliberately
/// *not* memoised on the generation counter: the counter also moves when a
/// design import brings a font of its own, and a memo keyed on the signal below
/// would go on showing the catalogue as it was before the import, because that
/// signal only moves while the installed families are still being read.
fn use_font_families() -> (Arc<Vec<FontFamily>>, Signal<bool>) {
    // Counts up when the installed families land. Read below, while
    // rendering, because that is what subscribes this selector to it.
    let mut families_ready: Signal<u64> = use_signal(fonts::catalog_generation);
    let mut pending: Signal<bool> = use_signal(fonts::system_fonts_pending);

    use_effect(move || {
        fonts::prepare_system_fonts();
        if !fonts::system_fonts_pending() {
            if *pending.peek() {
                pending.set(false);
            }
            return;
        }
        if !*pending.peek() {
            pending.set(true);
        }

        // A thread cannot write to a signal, so the selector looks instead,
        // and stops as soon as the families are in. The wait is a timer rather
        // than a script in the page: `dioxus_time` runs on the platform's own
        // clock and needs no web view at all.
        spawn(async move {
            loop {
                let generation = fonts::catalog_generation();
                if generation != *families_ready.peek() {
                    families_ready.set(generation);
                }
                if !fonts::system_fonts_pending() {
                    pending.set(false);
                    return;
                }
                crate::logic::timer::sleep(std::time::Duration::from_millis(150)).await;
            }
        });
    });

    // Read while rendering, because that is what subscribes this selector to
    // it: without it, the families that land on the background thread would
    // never reach a selector that is already on the screen.
    let _ = families_ready();

    (fonts::available_now(), pending)
}

/// How many families the list offers at once.
///
/// Every row is drawn in the family it names, which is the point of the list —
/// and also its cost: a web view has to find and instantiate each of those
/// fonts before it can lay the row out. A computer with a designer's collection
/// has thousands, and drawing them all at once is what made the design editor
/// take seconds to open on Linux, three times over, once per font block. So the
/// list shows a windowful and the search finds the rest.
const VISIBLE_FAMILIES: usize = 40;

/// The families whose name matches what has been typed, best match first.
///
/// An empty query keeps the catalogue's own order, which puts the bundled and
/// web-safe families — the ones that travel with a design — at the top.
fn matching_families(families: &[FontFamily], query: &str) -> Vec<FontFamily> {
    let query = query.trim();
    if query.is_empty() {
        return families.iter().take(VISIBLE_FAMILIES).cloned().collect();
    }

    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, usize, FontFamily)> = families
        .iter()
        .enumerate()
        .filter_map(|(position, family)| {
            matcher
                .fuzzy_match(&family.name, query)
                .map(|score| (score, position, family.clone()))
        })
        .collect();

    // The catalogue's order breaks ties, so an equally good bundled match is
    // offered before an installed one.
    scored.sort_by(|one, other| other.0.cmp(&one.0).then(one.1.cmp(&other.1)));
    scored
        .into_iter()
        .take(VISIBLE_FAMILIES)
        .map(|(_, _, family)| family)
        .collect()
}

/// What a family's origin is called in the list.
fn source_label(source: FontSource) -> String {
    match source {
        FontSource::Bundled => t!("settings.fonts.group_bundled").to_string(),
        FontSource::WebSafe => t!("settings.fonts.group_websafe").to_string(),
        FontSource::System => t!("settings.fonts.group_system").to_string(),
        FontSource::Imported => t!("settings.fonts.group_imported").to_string(),
    }
}

/// Picks the font family for one text section of a presentation.
///
/// A search field rather than a drop-down of everything installed. Each row is
/// drawn in the family it names — that is what makes the list worth reading —
/// and a web view has to find and instantiate a font before it can lay such a
/// row out. A `<select>` holding every installed family therefore cost as many
/// font instantiations as the computer has fonts, three times over, before the
/// design editor could appear; on Linux that was the wait. Only a windowful is
/// drawn now, and the search reaches the rest.
///
/// Where a family comes from is said on the row instead of by grouping it,
/// because the distinction matters and a filtered list has no stable groups: a
/// font installed on this computer is not there when the same presentation is
/// opened on another machine or in a browser, while bundled and web-safe
/// families always are.
#[component]
fn FontFamilySelector(
    /// The family currently set, or `None` for the default.
    selected: Option<CssFontFamily>,

    /// Called with the new family whenever the selection changes.
    onchange: EventHandler<Option<CssFontFamily>>,
) -> Element {
    let (families, families_pending) = use_font_families();

    let current = selected
        .as_ref()
        .and_then(|family| family.family.clone())
        .unwrap_or_default();

    // What has been typed into the field, which is *not* the chosen family:
    // the field shows the choice until it is being searched in.
    let mut query = use_signal(String::new);
    let mut open = use_signal(|| false);
    // Which row the arrow keys are on. Kept as an index into what is shown, so
    // it is meaningless once the query changes — hence reset with it.
    let mut highlighted = use_signal(|| 0_usize);

    // Narrowing the catalogue to a windowful is the one part worth memoising:
    // it runs a fuzzy match over every family, and the editor around it is
    // redrawn on every step of a colour picker. Keyed on what is typed, so a
    // catalogue that has grown since — an import bringing a font with it —
    // still reaches it, because `families` is read fresh each render.
    let matches = {
        let families = Arc::clone(&families);
        use_memo(move || matching_families(&families, &query()))
    };

    // The whole catalogue, only to say how much of it is out of sight.
    let total = families.len();

    let mut choose = move |family: Option<String>| {
        match family {
            Some(name) => onchange.call(Some(CssFontFamily::with_family(name))),
            None => onchange.call(None),
        }
        query.set(String::new());
        open.set(false);
        highlighted.set(0);
    };

    let shown = matches();
    let shown_count = shown.len();

    rsx! {
        div { class: "font-family-selector",
            label {
                { t!("settings.fonts.family").to_string() }
                input {
                    r#type: "text",
                    role: "combobox",
                    aria_expanded: open().to_string(),
                    aria_autocomplete: "list",
                    // While the list is open the field is the search; closed,
                    // it says what the block is set to.
                    value: if open() { query() } else { current.clone() },
                    placeholder: t!("settings.fonts.family_default").to_string(),
                    oninput: move |event| {
                        query.set(event.value());
                        highlighted.set(0);
                        open.set(true);
                    },
                    onfocusin: move |_| {
                        query.set(String::new());
                        highlighted.set(0);
                        open.set(true);
                    },
                    // Leaving the field without having picked anything is not a
                    // change: the block keeps the family it had, and the field
                    // goes back to showing it.
                    onfocusout: move |_| {
                        open.set(false);
                        query.set(String::new());
                    },
                    onkeydown: move |event: Event<KeyboardData>| {
                        match event.key() {
                            Key::ArrowDown => {
                                event.prevent_default();
                                if !open() {
                                    open.set(true);
                                } else if shown_count > 0 {
                                    highlighted.set((highlighted() + 1).min(shown_count - 1));
                                }
                            }
                            Key::ArrowUp => {
                                event.prevent_default();
                                highlighted.set(highlighted().saturating_sub(1));
                            }
                            Key::Enter => {
                                event.prevent_default();
                                if open()
                                    && let Some(family) = shown.get(highlighted())
                                {
                                    choose(Some(family.name.clone()));
                                }
                            }
                            Key::Escape => {
                                open.set(false);
                                query.set(String::new());
                            }
                            _ => {}
                        }
                    },
                }
            }

            if open() {
                div {
                    class: "font-family-list",
                    role: "listbox",
                    aria_label: t!("settings.fonts.family").to_string(),

                    // Always reachable, and first: a design that names no
                    // family is drawn in whatever the platform picks, and
                    // getting back to that must not need a search.
                    button {
                        r#type: "button",
                        role: "option",
                        aria_selected: current.is_empty().to_string(),
                        class: if current.is_empty() {
                            "font-family-option font-family-option-active"
                        } else {
                            "font-family-option"
                        },
                        // The field must not lose focus on the way to the
                        // click, or the list would close before it arrives.
                        onmousedown: move |event: Event<MouseData>| event.prevent_default(),
                        onclick: move |_| choose(None),
                        span { class: "font-family-option-name",
                            { t!("settings.fonts.family_default").to_string() }
                        }
                    }

                    for (position , family) in shown.iter().enumerate() {
                        button {
                            key: "{family.name}",
                            r#type: "button",
                            role: "option",
                            aria_selected: (current == family.name).to_string(),
                            class: if position == highlighted() || current == family.name {
                                "font-family-option font-family-option-active"
                            } else {
                                "font-family-option"
                            },
                            onmousedown: move |event: Event<MouseData>| event.prevent_default(),
                            onclick: {
                                let name = family.name.clone();
                                move |_| choose(Some(name.clone()))
                            },
                            // Drawn in the family it names — the whole reason
                            // the list is worth looking at, and affordable
                            // because only a windowful of them exists.
                            span {
                                class: "font-family-option-name",
                                style: "font-family: '{family.name}';",
                                "{family.name}"
                            }
                            span { class: "font-family-option-source",
                                { source_label(family.source) }
                            }
                        }
                    }

                    // The installed families are read on a thread of their own,
                    // so the list opens with the ones that need no reading and
                    // says that more are coming rather than looking complete.
                    if families_pending() {
                        for row in 0..3 {
                            div {
                                key: "skeleton-{row}",
                                class: "font-family-option font-family-option-skeleton",
                                aria_hidden: "true",
                                span { class: "skeleton skeleton-text" }
                            }
                        }
                        p { class: "font-family-list-hint", aria_live: "polite",
                            { t!("settings.fonts.family_loading").to_string() }
                        }
                    } else if shown_count == 0 {
                        p { class: "font-family-list-hint",
                            { t!("settings.fonts.family_no_match").to_string() }
                        }
                    } else if total > shown_count {
                        p { class: "font-family-list-hint",
                            {
                                t!(
                                    "settings.fonts.family_more", count = total - shown_count
                                )
                                    .to_string()
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod family_search_tests {
    use super::*;

    fn catalogue() -> Vec<FontFamily> {
        [
            ("Alpha Sans", FontSource::Bundled),
            ("Arial", FontSource::WebSafe),
            ("Beta Serif", FontSource::System),
            ("Gamma Mono", FontSource::System),
        ]
        .into_iter()
        .map(|(name, source)| FontFamily {
            name: name.to_string(),
            source,
        })
        .collect()
    }

    /// The whole point of the window: a computer with thousands of fonts must
    /// not put thousands of rows into the page, because each of them costs the
    /// web view a font to find and instantiate before it can be laid out.
    #[test]
    fn test_no_more_than_a_windowful_is_ever_offered() {
        let many: Vec<FontFamily> = (0..5_000)
            .map(|number| FontFamily {
                name: format!("Family {number}"),
                source: FontSource::System,
            })
            .collect();

        assert_eq!(matching_families(&many, "").len(), VISIBLE_FAMILIES);
        assert_eq!(matching_families(&many, "Family").len(), VISIBLE_FAMILIES);
    }

    /// With nothing typed the catalogue keeps its own order, which puts the
    /// families that travel with a design first.
    #[test]
    fn test_the_families_that_travel_are_offered_first() {
        let offered = matching_families(&catalogue(), "");

        assert_eq!(offered.first().map(|family| family.source), Some(FontSource::Bundled));
    }

    /// The search is what reaches the families the window leaves out, so it has
    /// to find one by part of its name rather than only by its start.
    #[test]
    fn test_a_family_is_found_by_part_of_its_name() {
        let offered = matching_families(&catalogue(), "serif");

        assert_eq!(
            offered.iter().map(|family| family.name.as_str()).collect::<Vec<_>>(),
            vec!["Beta Serif"]
        );
    }

    /// A query nothing matches yields nothing, rather than falling back to the
    /// whole list — the field would otherwise say the search had worked.
    #[test]
    fn test_a_query_nothing_matches_offers_nothing() {
        assert!(matching_families(&catalogue(), "zzzzzzzz").is_empty());
    }

    /// Every origin has a name to show on the row. The list stopped grouping
    /// them when it started filtering, and an unnamed origin would leave the
    /// most important distinction — does this font travel with the design —
    /// unsaid.
    #[test]
    fn test_every_origin_is_named() {
        for source in [
            FontSource::Bundled,
            FontSource::WebSafe,
            FontSource::System,
            FontSource::Imported,
        ] {
            assert!(!source_label(source).is_empty());
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
