//! This module contains the functions for changing the font settings as defined in the [FontRepresentation] struct.

use dioxus::html::input::placeholder;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use rgb::RGB8;
use crate::logic::settings::{CssSize, FontRepresentation};
use rust_i18n::t;
use crate::components::shared_components::NumberedValidatedLengthInput;

use crate::logic::conversions::*;

rust_i18n::i18n!("locales", fallback = "en");

/// A component which renders and provides the manipulation features for [FontRepresentation]s
#[component]
pub fn FontRepresentationsComponent(
    /// The font representation as a vector
    fonts: Vec<FontRepresentation>,

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
) -> Element {
    let mut font = use_signal(|| font);

    rsx!(
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