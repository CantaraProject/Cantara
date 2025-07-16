//! This module contains the functions for changing the font settings as defined in the [FontRepresentation] struct.

use crate::components::shared_components::NumberedValidatedLengthInput;
use crate::logic::settings::{CssSize, FontRepresentation};
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
                { t!("settings.fonts.primary_font") }
            }
        }
        else if let Some(true) = is_spoiler {
            div {
                class: "badge-2",
                { t!("settings.fonts.spoiler_font") }
            }
        }
        else if let Some(true) = is_meta {
            div {
                class: "badge-3",
                { t!("settings.fonts.meta_font") }
            }
        }
        else {
            div {
                class: "badge-inactive",
                { t!("settings.fonts.secondary_font") }
            }
        }

        form {
            label {
                { t!("settings.fonts.size") }
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

            fieldset {
                label {
                    { t!("settings.color") }
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
        }
    )
}
