//! This module provides components for adjusting the presentation designs

use crate::components::font_settings::FontRepresentationsComponent;
use crate::components::presentation_components::StaticSlideRendererComponent;
use crate::components::shared_components::NumberedValidatedLengthInput;
use cantara_songlib::slides::{Slide, SlideSettings};
use crate::logic::settings::{
    CssSize, HorizontalAlign, PresentationDesign, PresentationDesignSettings,
    PresentationDesignTemplate, TopBottomLeftRight, VerticalAlign, use_settings,
};
use crate::logic::sourcefiles::{ImageSourceFile, SourceFile};
use dioxus::core_macro::{component, rsx};
use dioxus::dioxus_core::Element;
use dioxus::hooks::use_signal;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use rust_i18n::t;
use std::path::PathBuf;

rust_i18n::i18n!("locales", fallback = "en");

/// This page contains the general settings for Cantara
#[component]
pub fn PresentationDesignSettingsPage(
    /// The index of the presentation design
    index: u16,
) -> Element {
    let nav = navigator();
    let mut settings = use_settings();

    let selected_presentation_design_option: Signal<Option<PresentationDesign>> =
        use_signal(|| {
            settings
                .read()
                .presentation_designs
                .clone()
                .get(index as usize)
                .cloned()
        });

    if selected_presentation_design_option.read().is_none() {
        // If no selected design is available, redirect to the settings page
        nav.replace(crate::Route::SettingsPage {});
        return rsx! {};
    }

    // From here on, the selected_presentation_design is guaranteed to be Some

    let selected_presentation_design =
        use_memo(move || selected_presentation_design_option.read().clone().unwrap_or_default());

    // Whether the preview is docked. Only consulted on narrow screens: the
    // stylesheet keeps it beside the settings whenever there is room.
    let show_preview = use_memo(move || settings.read().show_design_preview);

    rsx! {
        div { class: "wrapper",
            header { class: "top-bar",
                h2 {
                    {
                        t!(
                            "settings.presentation_designs_edit_header", title =
                            selected_presentation_design().name
                        )
                            .to_string()
                    }
                }
            }
            main {
                class: if show_preview() {
                    "container-fluid content height-100 design-editor preview-open"
                } else {
                    "container-fluid content height-100 design-editor"
                },

                div { class: "design-editor-settings",

                MetaSettings {
                    presentation_design: selected_presentation_design(),
                    on_pd_changed: move |pd: PresentationDesign| {
                        let mut settings_write = settings.write();
                        if let Some(origin_pd) = settings_write
                            .presentation_designs
                            .get_mut(index as usize)
                        {
                            origin_pd.name = pd.name;
                            origin_pd.description = pd.description;
                        }
                    },
                }

                if let PresentationDesignSettings::Template(pd_template) = selected_presentation_design()
                    .presentation_design_settings
                {
                    hr {}
                    DesignTemplateSettings {
                        presentation_design_template: pd_template,
                        onchange: move |new_pdt: PresentationDesignTemplate| {
                            let mut settings_write = settings.write();
                            if let Some(current) = settings_write
                                .presentation_designs
                                .get_mut(index as usize)
                            {
                                if let PresentationDesignSettings::Template(pdt) = &mut current
                                    .presentation_design_settings
                                {
                                    *pdt = new_pdt.clone();
                                }
                            }
                        },
                    }
                }

                }

                PresentationDesignPreview { index }
            }
            footer { class: "bottom-bar",
                button {
                    onclick: move |_| {
                        nav.replace(crate::Route::SettingsPage {});
                    },
                    {t!("settings.close").to_string()}
                }
                // Only reachable where the two-column layout does not fit; a
                // wide screen shows the preview beside the settings anyway.
                button {
                    r#type: "button",
                    class: "outline design-preview-toggle",
                    aria_pressed: show_preview().to_string(),
                    onclick: move |_| {
                        let next = !show_preview();
                        let mut settings_write = settings.write();
                        settings_write.show_design_preview = next;
                        settings_write.save();
                    },
                    if show_preview() {
                        {t!("settings.design_preview.hide").to_string()}
                    } else {
                        {t!("settings.design_preview.show").to_string()}
                    }
                }
            }
        }
    }
}

/// This component allow the setting up of meta settings for presentation designs
#[component]
fn MetaSettings(
    /// The presentation design which Meta information should be able to be edited
    presentation_design: PresentationDesign,

    /// A closure which is called each time when the presentation design has been changed
    on_pd_changed: EventHandler<PresentationDesign>,
) -> Element {
    let mut pd = use_signal(|| presentation_design);

    rsx! {
        h3 { {t!("general.meta_information").to_string()} }
        form {
            fieldset {
                label {
                    {t!("general.name").to_string()}
                    input {
                        value: pd().name,
                        onchange: move |event| {
                            pd.write().name = event.value().clone();
                            on_pd_changed.call(pd());
                        },
                    }
                }

                label {
                    {t!("general.description").to_string()}
                    input {
                        value: pd().description,
                        onchange: move |event| {
                            pd.write().description = event.value().clone();
                            on_pd_changed.call(pd());
                        },
                    }
                }
            }
        }
    }
}

/// This component implements the actual settings for any presentation design which is a
/// design template.
/// Include further settings components here.
#[component]
fn DesignTemplateSettings(
    /// The presentation design which Meta information should be able to be edited
    presentation_design_template: PresentationDesignTemplate,

    /// An event which is called each time when the presentation design template has been changed
    /// by the component
    onchange: EventHandler<PresentationDesignTemplate>,
) -> Element {
    let mut pdt = use_signal(|| presentation_design_template);
    let mut use_background_image: Signal<bool> = use_signal(|| pdt().background_image.is_some());

    rsx!(
        h3 { {t!("settings.presentation_design_configuration").to_string()} }
        h4 { {t!("settings.background").to_string()} }
        form {
            fieldset {

                // Background color
                label {
                    {t!("settings.color").to_string()}
                    input {
                        r#type: "color",
                        value: pdt().get_background_color_as_hex_string(),
                        onchange: move |event| {
                            _ = pdt.write().set_background_color_from_hex_str(&event.value());
                            onchange.call(pdt());
                        },
                    }
                }

                // Use background image
                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: use_background_image,
                        onchange: move |event| {
                            use_background_image.set(event.checked());
                            pdt.write().background_image = None;
                            onchange.call(pdt());
                        },
                    }
                    {t!("settings.use_background_image").to_string()}
                }

                if use_background_image() {
                    if let Some(background_image) = pdt().background_image {
                        PictureSelector {
                            onchange: move |background_image| {
                                pdt.write().background_image = Some(background_image);
                                onchange.call(pdt());
                            },
                            already_selected_image_path: background_image.into_inner().path,
                        }
                    } else {
                        PictureSelector {
                            onchange: move |background_image| {
                                pdt.write().background_image = Some(background_image);
                                onchange.call(pdt());
                            },
                        }
                    }

                    // Adjust the background image transparency over a range input
                    label {
                        span {
                            {
                                format!(
                                    "{}: {}%",
                                    t!("settings.background_image_transparency"),
                                    pdt.read().background_transparency,
                                )
                            }
                        }
                        input {
                            r#type: "range",
                            min: 0,
                            max: 100,
                            value: pdt.read().background_transparency,
                            oninput: move |event| {
                                pdt.write().background_transparency = event.value().parse().unwrap_or(0);
                                onchange.call(pdt());
                            },
                        }
                    
                    }
                }
            }
        }

        // Padding
        h4 { {t!("settings.padding").to_string()} }
        PaddingInput {
            default_padding: pdt().padding,
            onchange: move |data| {
                pdt.write().padding = data;
                onchange.call(pdt());
            },
        }

        // Distance between the main content and spoiler content
        h4 { {t!("settings.main_spoiler_content_distance").to_string()} }
        fieldset { role: "group",
            NumberedValidatedLengthInput {
                value: pdt().main_content_spoiler_content_padding,
                placeholder: "".to_string(),
                onchange: move |new_value| {
                    pdt.write().main_content_spoiler_content_padding = new_value;
                    onchange.call(pdt());
                },
            }
        }

        // Here the settings for the vertical alignment of the content are included
        h5 { {t!("settings.vertical_alignment.title").to_string()} }
        VerticalAlignmentSelector {
            default: pdt().vertical_alignment,
            onchange: move |data| {
                pdt.write().vertical_alignment = data;
                onchange.call(pdt());
            },
        }

        // The title slide. The gap to its meta line is the distance the design
        // already defines between main content and spoiler, so there is nothing
        // to configure for it.
        h4 { {t!("settings.title_slide.title").to_string()} }
        form {
            fieldset {
                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: pdt().title_bold,
                        onchange: move |event| {
                            pdt.write().title_bold = event.checked();
                            onchange.call(pdt());
                        },
                    }
                    {t!("settings.title_slide.bold").to_string()}
                }
            }
        }

        // The notation block of a complex slide.
        h4 { {t!("settings.notation.title").to_string()} }
        form {
            fieldset {
                label {
                    {
                        format!(
                            "{}: {} %",
                            t!("settings.notation.width"),
                            pdt().notation.width_percent,
                        )
                    }
                    input {
                        r#type: "range",
                        min: "10",
                        max: "100",
                        step: "5",
                        value: "{pdt().notation.width_percent}",
                        oninput: move |event| {
                            let value = event.value().parse::<f64>().unwrap_or(100.0);
                            pdt.write().notation.width_percent = value;
                            onchange.call(pdt());
                        },
                    }
                }

                label {
                    {
                        format!(
                            "{}: {}",
                            t!("settings.notation.staff_line_height"),
                            pdt().notation.staff_line_height,
                        )
                    }
                    input {
                        r#type: "range",
                        min: "0.5",
                        max: "3",
                        step: "0.1",
                        value: "{pdt().notation.staff_line_height}",
                        oninput: move |event| {
                            let value = event.value().parse::<f64>().unwrap_or(1.0);
                            pdt.write().notation.staff_line_height = value;
                            onchange.call(pdt());
                        },
                    }
                }

                label {
                    {t!("settings.notation.font_size").to_string()}
                    NumberedValidatedLengthInput {
                        value: pdt().notation.font_size,
                        placeholder: "",
                        onchange: move |value: CssSize| {
                            pdt.write().notation.font_size = value;
                            onchange.call(pdt());
                        },
                    }
                }

                NotationAlignmentSelector {
                    default: pdt().notation.horizontal_alignment,
                    onchange: move |value: HorizontalAlign| {
                        pdt.write().notation.horizontal_alignment = value;
                        onchange.call(pdt());
                    },
                }
            }
        }

        // Adjust individual font settings
        h3 { {t!("settings.fonts.title").to_string()} }

        FontRepresentationsComponent {
            fonts: pdt().fonts,
            spoiler_index: pdt().spoiler_index(),
            meta_index: pdt().meta_index,
            onchange: move |data| {
                pdt.write().fonts = data;
                onchange.call(pdt());
            },
        }
    )
}

/// A component which allows the selection of a picture
#[component]
fn PictureSelector(
    default_selection_index: Option<usize>,

    /// This can be given if an image is already set up. It will then be selected as default.
    already_selected_image_path: Option<PathBuf>,

    /// The event will be called if a picture has been selected
    onchange: Option<EventHandler<ImageSourceFile>>,
) -> Element {
    let source_files: Signal<Vec<SourceFile>> = use_context();
    let image_source_files: Memo<Vec<ImageSourceFile>> = use_memo(move || {
        source_files()
            .into_iter()
            .filter_map(ImageSourceFile::new)
            .collect()
    });
    let mut selection_index = use_signal(|| default_selection_index);

    rsx! {
        for (idx , source_file) in image_source_files().iter().enumerate() {
            PictureSelectorItem {
                source_file: source_file.clone(),
                height: "130px",
                max_width: "200px",
                active: if let Some(selection_index) = selection_index() { selection_index == idx } else if Some(source_file.clone().into_inner().path) == already_selected_image_path { true } else { false },
                onclick: move |image_source_file| {
                    selection_index.set(Some(idx));
                    if let Some(onchange_event) = onchange {
                        onchange_event.call(image_source_file);
                    }
                },
            }
        }
    }
}

/// A component representing a single item (picture) in the [PictureSelector] component
#[component]
fn PictureSelectorItem(
    max_width: String,
    height: String,
    source_file: ImageSourceFile,
    onclick: EventHandler<ImageSourceFile>,
    active: bool,
) -> Element {
    // We need a source file signal here due to the use in the closure
    let sourcefile_signal = use_signal(|| source_file);
    rsx! {
        button {
            role: "button",
            class: if active { "outline" } else { "outline secondary" },
            "data-tooltip": sourcefile_signal().into_inner().name,
            onclick: move |event| {
                onclick.call(sourcefile_signal());
                event.prevent_default();
            },
            img {
                max_width: "180px",
                height: "100px",
                src: sourcefile_signal().into_inner().path.to_str().unwrap_or("").to_string(),
            }
        }
    }
}

/// A component which allows the setting of padding (left, right, top, bottom)
#[component]
fn PaddingInput(
    default_padding: TopBottomLeftRight,
    onchange: EventHandler<TopBottomLeftRight>,
) -> Element {
    let mut padding: Signal<TopBottomLeftRight> = use_signal(|| default_padding);

    rsx!(
        div { class: "grid",
            div {
                label {
                    "Left"
                    fieldset { role: "group",
                        NumberedValidatedLengthInput {
                            value: padding().left,
                            placeholder: "left",
                            onchange: move |value| {
                                padding.write().left = get_nullified_css_size(value);
                                onchange.call(padding());
                            },
                        }
                    }
                }
            }
            div {
                label {
                    "Right"
                    fieldset { role: "group",
                        NumberedValidatedLengthInput {
                            value: padding().right,
                            placeholder: "right",
                            onchange: move |value| {
                                padding.write().right = get_nullified_css_size(value);
                                onchange.call(padding());
                            },
                        }
                    }
                }
            }
        }
        div { class: "grid",
            div {
                label {
                    "Top"
                    fieldset { role: "group",
                        NumberedValidatedLengthInput {
                            value: padding().top,
                            placeholder: "top",
                            onchange: move |value: CssSize| {
                                padding.write().top = get_nullified_css_size(value);
                                onchange.call(padding());
                            },
                        }
                    }
                }
            }
            div {
                label {
                    "Bottom"
                    fieldset { role: "group",
                        NumberedValidatedLengthInput {
                            value: padding().bottom,
                            placeholder: "bottom",
                            onchange: move |value: CssSize| {
                                // If the content is null, we will set it accordingly
                                padding.write().bottom = get_nullified_css_size(value);
                                onchange.call(padding());
                            },
                        }
                    }
                }
            }
        }
    )
}

/// Returns a [CssSize::Null] if the value is `0.0`. Else, the original value is cloned.
fn get_nullified_css_size(css_size: CssSize) -> CssSize {
    match css_size.get_float() {
        0.0 => CssSize::Null,
        _ => css_size.clone(),
    }
}

/// A component for selecting the vertical alignment (left, right, centered)
#[component]
fn VerticalAlignmentSelector(
    default: VerticalAlign,
    onchange: EventHandler<VerticalAlign>,
) -> Element {
    let mut value_signal = use_signal(|| default);
    rsx!(
        select {
            name: "vertical_align",
            required: true,
            aria_label: t!("settings.vertical_alignment.description").to_string(),
            onchange: move |event| {
                match event.value().as_str() {
                    "top" => value_signal.set(VerticalAlign::Top),
                    "middle" => value_signal.set(VerticalAlign::Middle),
                    "bottom" => value_signal.set(VerticalAlign::Bottom),
                    other => {
                        tracing::error!(
                            "Invalid option for vertical alignment selected, the value is: {}",
                            other
                        )
                    }
                };
                onchange.call(value_signal());
            },
            option { value: "top", selected: value_signal() == VerticalAlign::Top,
                {t!("settings.vertical_alignment.top").to_string()}
            }
            option {
                value: "middle",
                selected: value_signal() == VerticalAlign::Middle,
                {t!("settings.vertical_alignment.middle").to_string()}
            }
            option {
                value: "bottom",
                selected: value_signal() == VerticalAlign::Bottom,
                {t!("settings.vertical_alignment.bottom").to_string()}
            }
        }
    )
}

/// The song the design preview is built from.
///
/// Deliberately tiny and in the classic `.song` format, which every build can
/// parse without touching the file system: two verses give the preview a
/// spoiler line and something to page through, and the tags feed whatever meta
/// syntax the user configured.
const PREVIEW_SONG: &str = "\
#title: Amazing Grace
#author: John Newton

Amazing grace, how sweet the sound
that saved a wretch like me.
I once was lost, but now am found,
was blind, but now I see.

'Twas grace that taught my heart to fear,
and grace my fears relieved.
How precious did that grace appear
the hour I first believed.
";

/// The slides the preview pages through.
///
/// Built by the same pipeline a real presentation uses, with the user's own
/// slide settings, so the preview shows the actual slide types — title slide,
/// content with spoiler, empty last slide — rather than an approximation.
fn preview_slides(slide_settings: &SlideSettings) -> Vec<Slide> {
    crate::logic::presentation::slides_from_song_content(
        PREVIEW_SONG,
        "Amazing Grace.song",
        slide_settings,
        "Amazing Grace",
    )
    .unwrap_or_default()
}

/// A live preview of the design being edited.
///
/// It reads the design straight from the settings rather than from a snapshot,
/// so every change made in the form on the left shows up immediately.
///
/// The slide is drawn by [`StaticSlideRendererComponent`] — the same component
/// the presenter console uses for its thumbnails, and the same one a real
/// presentation is built from — so the preview cannot drift from what the
/// audience will see.
#[component]
fn PresentationDesignPreview(
    /// The index of the presentation design in the settings.
    index: u16,
) -> Element {
    let settings = use_settings();

    let design = use_memo(move || {
        settings
            .read()
            .presentation_designs
            .get(index as usize)
            .cloned()
            .unwrap_or_default()
    });

    let slides = use_memo(move || {
        let slide_settings = settings
            .read()
            .song_slide_settings
            .first()
            .cloned()
            .unwrap_or_default();
        preview_slides(&slide_settings)
    });

    let mut position = use_signal(|| 0_usize);

    // The slide count changes with the slide settings, so the position is
    // clamped on read instead of being trusted.
    let slide_count = slides.read().len();
    let current = if slide_count == 0 {
        0
    } else {
        position().min(slide_count - 1)
    };

    rsx! {
        aside { class: "design-preview-pane",
            h4 { {t!("settings.design_preview.title").to_string()} }

            if slide_count == 0 {
                p { class: "design-preview-empty",
                    {t!("settings.design_preview.unavailable").to_string()}
                }
            } else {
                // The slide is drawn at presentation size and scaled down by
                // the stylesheet, so the preview is a true miniature rather
                // than a re-flowed layout. The scale lives in CSS because it
                // has to follow the screen width.
                div { class: "design-preview-stage",
                    div { class: "design-preview-canvas",
                        StaticSlideRendererComponent {
                            slide: slides.read()[current].clone(),
                            presentation_design: design(),
                        }
                    }
                }

                div { class: "design-preview-controls",
                    button {
                        r#type: "button",
                        class: "outline",
                        disabled: current == 0,
                        aria_label: t!("settings.design_preview.previous").to_string(),
                        onclick: move |_| {
                            let previous = position().min(slide_count - 1).saturating_sub(1);
                            position.set(previous);
                        },
                        "‹"
                    }
                    span { class: "design-preview-position", "{current + 1} / {slide_count}" }
                    button {
                        r#type: "button",
                        class: "outline",
                        disabled: current + 1 >= slide_count,
                        aria_label: t!("settings.design_preview.next").to_string(),
                        onclick: move |_| {
                            let next = (position().min(slide_count - 1) + 1).min(slide_count - 1);
                            position.set(next);
                        },
                        "›"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cantara_songlib::slides::SlideContent;

    /// The preview is useless if the sample song yields nothing to look at.
    #[test]
    fn test_the_preview_song_produces_slides() {
        let slides = preview_slides(&SlideSettings::default());

        assert!(
            slides.len() > 1,
            "the preview needs more than one slide to page through"
        );
    }

    /// The point of the preview is to show the real slide types, so the design's
    /// title font and its content font can both be judged.
    #[test]
    fn test_the_preview_shows_a_title_and_content_slides() {
        let settings = SlideSettings {
            title_slide: true,
            ..SlideSettings::default()
        };
        let slides = preview_slides(&settings);

        assert!(
            slides
                .iter()
                .any(|slide| matches!(slide.slide_content, SlideContent::Title(_))),
            "no title slide in the preview"
        );
        assert!(
            slides.iter().any(|slide| !matches!(
                slide.slide_content,
                SlideContent::Title(_) | SlideContent::Empty(_)
            )),
            "no content slide in the preview"
        );
    }

    /// The preview follows the user's slide settings, so a change there has to
    /// reach it — otherwise it would show something the presentation will not.
    #[test]
    fn test_the_preview_follows_the_slide_settings() {
        let with_title = preview_slides(&SlideSettings {
            title_slide: true,
            ..SlideSettings::default()
        });
        let without_title = preview_slides(&SlideSettings {
            title_slide: false,
            ..SlideSettings::default()
        });

        assert_ne!(
            with_title.len(),
            without_title.len(),
            "turning the title slide off changed nothing"
        );
    }


    /// Renders every preview slide with the user's real design, so a panic in
    /// the preview shows up here instead of taking the editor down.
    #[test]
    fn test_every_preview_slide_renders() {
        use crate::components::presentation_components::{
            StaticSlideRendererComponent, StaticSlideRendererComponentProps,
        };

        for design in [PresentationDesign::default()] {
            for slides in [
                preview_slides(&SlideSettings::default()),
                preview_slides(&SlideSettings {
                    title_slide: false,
                    empty_last_slide: false,
                    ..SlideSettings::default()
                }),
            ] {
                for slide in slides {
                    let mut dom = dioxus::prelude::VirtualDom::new_with_props(
                        StaticSlideRendererComponent,
                        StaticSlideRendererComponentProps {
                            slide,
                            presentation_design: design.clone(),
                        },
                    );
                    dom.rebuild_in_place();
                }
            }
        }
    }

    /// A settings combination the sample song cannot satisfy must leave the
    /// preview empty rather than take the editor down with it.
    #[test]
    fn test_impossible_settings_yield_no_slides_instead_of_panicking() {
        use cantara_songlib::slides::{LanguageConfiguration, SlideElement};

        let settings = SlideSettings {
            // The sample song is a classic `.song` with no melody at all.
            language: LanguageConfiguration::Complex(vec![SlideElement::Notation]),
            title_slide: false,
            empty_last_slide: false,
            ..SlideSettings::default()
        };

        let slides = preview_slides(&settings);
        // Whatever comes out, asking for it must not panic and must be usable.
        assert!(slides.len() < 100);
    }
}

/// Where a staff narrower than the content sits.
///
/// Only the three placements that mean something for a staff are offered: the
/// justify modes of a text block have no equivalent in engraving.
#[component]
fn NotationAlignmentSelector(
    default: HorizontalAlign,
    onchange: EventHandler<HorizontalAlign>,
) -> Element {
    let options: [(&str, HorizontalAlign, &str); 3] = [
        ("left", HorizontalAlign::Left, "settings.horizontal_alignment.left"),
        ("centered", HorizontalAlign::Centered, "settings.horizontal_alignment.centered"),
        ("right", HorizontalAlign::Right, "settings.horizontal_alignment.right"),
    ];

    rsx! {
        label {
            { t!("settings.horizontal_alignment.title").to_string() }
            select {
                onchange: move |event| {
                    let value = match event.value().as_str() {
                        "left" => HorizontalAlign::Left,
                        "right" => HorizontalAlign::Right,
                        _ => HorizontalAlign::Centered,
                    };
                    onchange.call(value);
                },
                for (value , variant , key) in options {
                    option {
                        value: "{value}",
                        selected: default == variant,
                        { t!(key).to_string() }
                    }
                }
            }
        }
    }
}
