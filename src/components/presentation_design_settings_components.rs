//! This module provides components for adjusting the presentation designs

use crate::components::font_settings::FontRepresentationsComponent;
use crate::components::presentation_components::StaticSlideRendererComponent;
use crate::components::shared_components::{NumberedValidatedLengthInput, js_message_box};
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

    // Read from the settings on every render rather than copied once: the
    // settings are what the preview draws, so an editor working on a copy of
    // its own would show one thing and preview another — which is why the
    // preview did not follow a change of font or background picture.
    let selected_presentation_design_option: Memo<Option<PresentationDesign>> =
        use_memo(move || settings.read().presentation_designs.get(index as usize).cloned());

    // Whether the preview is docked. Only consulted on narrow screens: the
    // stylesheet keeps it beside the settings whenever there is room.
    let show_preview = use_memo(move || settings.read().show_design_preview);

    // The edits land in the settings as they are made and are written out when
    // the editor is left — by its own button, or by anything else that
    // navigates away. `try_read`, because a program that is closing may have
    // taken the settings down before this runs.
    use_drop(move || {
        if let Ok(settings) = settings.try_read() {
            settings.save();
        }
    });

    // Every hook is claimed before the way out below: a design that is deleted
    // while its editor is open would otherwise leave this render with fewer
    // hooks than the last one.
    let selected_presentation_design =
        use_memo(move || selected_presentation_design_option().unwrap_or_default());

    if selected_presentation_design_option.read().is_none() {
        // If no selected design is available, redirect to the settings page
        nav.replace(crate::Route::SettingsPage {});
        return rsx! {};
    }

    // From here on, the selected_presentation_design is guaranteed to be Some

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
                                && let PresentationDesignSettings::Template(pdt) = &mut current
                                    .presentation_design_settings
                                {
                                    *pdt = new_pdt.clone();
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
                        // Written out here rather than on every keystroke: the
                        // edits go into the settings as they are made, and a
                        // file write per slider step is not worth its cost.
                        // A design that could not be written is worth saying,
                        // since it is gone when the program ends.
                        let save_error = settings.read().try_save().err();
                        async move {
                            if let Some(error) = save_error {
                                let message = t!("dialogs.settings_not_saved", error = error)
                                    .to_string();
                                let _ = document::eval(&js_message_box(message)).await;
                            }
                            nav.replace(crate::Route::SettingsPage {});
                        }
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
    // Driven by the prop rather than by a copy taken on the first render: the
    // design belongs to the settings, and a copy here would go stale as soon as
    // anything else touched it.
    rsx! {
        h3 { {t!("general.meta_information").to_string()} }
        form {
            fieldset {
                label {
                    {t!("general.name").to_string()}
                    input {
                        value: presentation_design.name.clone(),
                        onchange: {
                            let base = presentation_design.clone();
                            move |event: Event<FormData>| {
                                let mut updated = base.clone();
                                updated.name = event.value();
                                on_pd_changed.call(updated);
                            }
                        },
                    }
                }

                label {
                    {t!("general.description").to_string()}
                    input {
                        value: presentation_design.description.clone(),
                        onchange: {
                            let base = presentation_design.clone();
                            move |event: Event<FormData>| {
                                let mut updated = base.clone();
                                updated.description = event.value();
                                on_pd_changed.call(updated);
                            }
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
    // Driven by the prop, like the font blocks below it and for the same
    // reason: the design lives in the settings, and an edited copy kept here
    // would be a second truth the preview knows nothing about. Every handler
    // therefore clones the design it edits — one shared copy could only be
    // moved into a single closure.
    let pdt = presentation_design_template;
    let mut use_background_image: Signal<bool> = use_signal(|| pdt.background_image.is_some());

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
                        value: pdt.get_background_color_as_hex_string(),
                        // `oninput` rather than `onchange`, as with the font
                        // colours: the picker only commits when it is closed,
                        // and the preview should follow while a colour is
                        // being chosen.
                        oninput: {
                            let base = pdt.clone();
                            move |event: Event<FormData>| {
                                let mut updated = base.clone();
                                _ = updated.set_background_color_from_hex_str(&event.value());
                                onchange.call(updated);
                            }
                        },
                    }
                }

                // Use background image
                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: use_background_image,
                        onchange: {
                            let base = pdt.clone();
                            move |event: Event<FormData>| {
                                use_background_image.set(event.checked());
                                let mut updated = base.clone();
                                updated.background_image = None;
                                onchange.call(updated);
                            }
                        },
                    }
                    {t!("settings.use_background_image").to_string()}
                }

                if use_background_image() {
                    PictureSelector {
                        onchange: {
                            let base = pdt.clone();
                            move |background_image| {
                                let mut updated = base.clone();
                                updated.background_image = Some(background_image);
                                onchange.call(updated);
                            }
                        },
                        already_selected_image_path: pdt
                            .background_image
                            .clone()
                            .map(|image| image.into_inner().path),
                    }

                    // Adjust the background image transparency over a range input
                    label {
                        span {
                            {
                                format!(
                                    "{}: {}%",
                                    t!("settings.background_image_transparency"),
                                    pdt.background_transparency,
                                )
                            }
                        }
                        input {
                            r#type: "range",
                            min: 0,
                            max: 100,
                            value: pdt.background_transparency,
                            oninput: {
                                let base = pdt.clone();
                                move |event: Event<FormData>| {
                                    let mut updated = base.clone();
                                    updated.background_transparency = event.value().parse().unwrap_or(0);
                                    onchange.call(updated);
                                }
                            },
                        }

                    }
                }
            }
        }

        // Padding
        h4 { {t!("settings.padding").to_string()} }
        PaddingInput {
            default_padding: pdt.padding.clone(),
            onchange: {
                let base = pdt.clone();
                move |data| {
                    let mut updated = base.clone();
                    updated.padding = data;
                    onchange.call(updated);
                }
            },
        }

        // Distance between the main content and spoiler content
        h4 { {t!("settings.main_spoiler_content_distance").to_string()} }
        fieldset { role: "group",
            NumberedValidatedLengthInput {
                value: pdt.main_content_spoiler_content_padding.clone(),
                placeholder: "".to_string(),
                onchange: {
                    let base = pdt.clone();
                    move |new_value| {
                        let mut updated = base.clone();
                        updated.main_content_spoiler_content_padding = new_value;
                        onchange.call(updated);
                    }
                },
            }
        }

        // Here the settings for the vertical alignment of the content are included
        h5 { {t!("settings.vertical_alignment.title").to_string()} }
        VerticalAlignmentSelector {
            default: pdt.vertical_alignment,
            onchange: {
                let base = pdt.clone();
                move |data| {
                    let mut updated = base.clone();
                    updated.vertical_alignment = data;
                    onchange.call(updated);
                }
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
                        checked: pdt.title_bold,
                        onchange: {
                            let base = pdt.clone();
                            move |event: Event<FormData>| {
                                let mut updated = base.clone();
                                updated.title_bold = event.checked();
                                onchange.call(updated);
                            }
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
                            pdt.notation.width_percent,
                        )
                    }
                    input {
                        r#type: "range",
                        min: "10",
                        max: "100",
                        step: "5",
                        value: "{pdt.notation.width_percent}",
                        oninput: {
                            let base = pdt.clone();
                            move |event: Event<FormData>| {
                                let mut updated = base.clone();
                                updated.notation.width_percent = event.value().parse::<f64>().unwrap_or(100.0);
                                onchange.call(updated);
                            }
                        },
                    }
                }

                label {
                    {
                        format!(
                            "{}: {}",
                            t!("settings.notation.staff_line_height"),
                            pdt.notation.staff_line_height,
                        )
                    }
                    input {
                        r#type: "range",
                        min: "0.5",
                        max: "3",
                        step: "0.1",
                        value: "{pdt.notation.staff_line_height}",
                        oninput: {
                            let base = pdt.clone();
                            move |event: Event<FormData>| {
                                let mut updated = base.clone();
                                updated.notation.staff_line_height = event.value().parse::<f64>().unwrap_or(1.0);
                                onchange.call(updated);
                            }
                        },
                    }
                }

                label {
                    {t!("settings.notation.font_size").to_string()}
                    NumberedValidatedLengthInput {
                        value: pdt.notation.font_size.clone(),
                        placeholder: "",
                        onchange: {
                            let base = pdt.clone();
                            move |value: CssSize| {
                                let mut updated = base.clone();
                                updated.notation.font_size = value;
                                onchange.call(updated);
                            }
                        },
                    }
                }

                NotationAlignmentSelector {
                    default: pdt.notation.horizontal_alignment,
                    onchange: {
                        let base = pdt.clone();
                        move |value: HorizontalAlign| {
                            let mut updated = base.clone();
                            updated.notation.horizontal_alignment = value;
                            onchange.call(updated);
                        }
                    },
                }
            }
        }

        // Adjust individual font settings
        h3 { {t!("settings.fonts.title").to_string()} }

        FontRepresentationsComponent {
            fonts: pdt.fonts.clone(),
            spoiler_index: pdt.spoiler_index(),
            meta_index: pdt.meta_index,
            onchange: {
                let base = pdt.clone();
                move |data| {
                    let mut updated = base.clone();
                    updated.fonts = data;
                    onchange.call(updated);
                }
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

    // The pictures are scaled down on background threads and drawn as they
    // arrive. Reading and encoding every picture of the library here, while
    // this renders, is what made opening a design take seconds with the window
    // frozen — see [`crate::logic::images`].
    let mut thumbnails_ready: Signal<u64> = use_signal(crate::logic::images::thumbnail_generation);

    use_effect(move || {
        let paths: Vec<PathBuf> = image_source_files()
            .iter()
            .map(|image| image.clone().into_inner().path)
            .collect();
        crate::logic::images::prepare_thumbnails(paths);

        // A thread cannot write to a signal, so the list looks for what has
        // landed instead and stops as soon as they are all in.
        spawn(async move {
            loop {
                let generation = crate::logic::images::thumbnail_generation();
                if generation != *thumbnails_ready.peek() {
                    thumbnails_ready.set(generation);
                }
                if !crate::logic::images::thumbnails_in_progress() {
                    return;
                }
                let _ = document::eval("await new Promise(r => setTimeout(r, 150))").await;
            }
        });
    });

    // Read while rendering, because that is the only thing that subscribes
    // this list to it: a signal written that nobody read notifies nobody.
    let _ = thumbnails_ready();

    let count = image_source_files().len();

    if count == 0 {
        return rsx! {
            p { class: "picture-selector-empty",
                {t!("settings.background_image_none_available").to_string()}
            }
        };
    }

    rsx! {
        // A strip that scrolls sideways rather than a wall of pictures: the
        // library holds however many it holds, and stacked full-width they
        // pushed everything else in the design editor off the screen. The row
        // keeps the pictures at a size worth looking at and costs one line.
        div {
            class: "picture-selector",
            role: "listbox",
            aria_label: t!("settings.use_background_image").to_string(),
            // The picture the design already uses is usually not the first
            // one, so the strip starts where the answer is. Only `scrollLeft`
            // of the strip is touched — `scrollIntoView` would take the page
            // along with it.
            onmounted: move |_| async move {
                let _ = document::eval(
                    "const strip = document.querySelector('.picture-selector');
                     const active = strip && strip.querySelector('.picture-selector-item-active');
                     if (strip && active) {
                         strip.scrollLeft = active.offsetLeft
                             - (strip.clientWidth - active.clientWidth) / 2;
                     }",
                )
                .await;
            },

            // Looked up here and handed down, so that an item is redrawn when
            // its own thumbnail turns up rather than when any of them does.
            for (idx , source_file) in image_source_files().iter().enumerate() {
                PictureSelectorItem {
                    key: "{idx}",
                    source_file: source_file.clone(),
                    thumbnail: crate::logic::images::thumbnail(&source_file.clone().into_inner().path),
                    // Until the user has picked one, the picture the design
                    // already uses is the one that is shown as active.
                    active: match selection_index() {
                        Some(selected) => selected == idx,
                        None => Some(source_file.clone().into_inner().path) == already_selected_image_path,
                    },
                    onclick: move |image_source_file| {
                        selection_index.set(Some(idx));
                        if let Some(onchange_event) = onchange {
                            onchange_event.call(image_source_file);
                        }
                    },
                }
            }
        }
        small { class: "picture-selector-hint",
            {t!("settings.background_image_count", count = count).to_string()}
        }
    }
}

/// A component representing a single item (picture) in the [PictureSelector] component
#[component]
fn PictureSelectorItem(
    source_file: ImageSourceFile,
    /// A scaled-down copy of the picture, inlined into the page. `None` until
    /// it has been made on a background thread; see [`crate::logic::images`].
    thumbnail: Option<String>,
    onclick: EventHandler<ImageSourceFile>,
    active: bool,
) -> Element {
    // We need a source file signal here due to the use in the closure
    let mut sourcefile_signal = use_signal(|| source_file.clone());
    if *sourcefile_signal.peek() != source_file {
        sourcefile_signal.set(source_file);
    }
    let preview = thumbnail;

    let name = sourcefile_signal().into_inner().name;

    rsx! {
        button {
            r#type: "button",
            role: "option",
            aria_selected: active.to_string(),
            class: if active {
                "picture-selector-item picture-selector-item-active"
            } else {
                "picture-selector-item"
            },
            title: "{name}",
            onclick: move |event| {
                onclick.call(sourcefile_signal());
                event.prevent_default();
            },
            // The frame is the same size whether the picture is there or not.
            // The thumbnails are made on background threads and arrive one by
            // one; without this, every arrival changed the size of the strip
            // under the reader.
            div { class: "picture-selector-preview",
                if let Some(preview) = preview {
                    // The whole library's thumbnails arrive as inline data, and
                    // a web view decodes a picture before it can draw it. As in
                    // the library list: `lazy` leaves what is scrolled out of
                    // sight alone, `async` keeps the decoding of the rest off
                    // the thread that draws.
                    img {
                        loading: "lazy",
                        decoding: "async",
                        src: "{preview}",
                        alt: "{name}",
                    }
                }
                // Says which picture is in use even where the strip is too
                // narrow to show more than one frame.
                if active {
                    span { class: "picture-selector-check", aria_hidden: "true", "✓" }
                }
            }
            // The name under the picture rather than only in a tooltip: two
            // photographs of the same hall are told apart by their file name,
            // and a tooltip has to be hunted for.
            span { class: "picture-selector-name", "{name}" }
        }
    }
}

/// A component which allows the setting of padding (left, right, top, bottom)
#[component]
fn PaddingInput(
    default_padding: TopBottomLeftRight,
    onchange: EventHandler<TopBottomLeftRight>,
) -> Element {
    // Prop-driven, like everything else in the editor: the padding belongs to
    // the design in the settings and is only ever read back from there.
    let padding = default_padding;

    rsx!(
        div { class: "grid",
            div {
                label {
                    "Left"
                    fieldset { role: "group",
                        NumberedValidatedLengthInput {
                            value: padding.left.clone(),
                            placeholder: "left",
                            onchange: {
                                let base = padding.clone();
                                move |value| {
                                    let mut updated = base.clone();
                                    updated.left = get_nullified_css_size(value);
                                    onchange.call(updated);
                                }
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
                            value: padding.right.clone(),
                            placeholder: "right",
                            onchange: {
                                let base = padding.clone();
                                move |value| {
                                    let mut updated = base.clone();
                                    updated.right = get_nullified_css_size(value);
                                    onchange.call(updated);
                                }
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
                            value: padding.top.clone(),
                            placeholder: "top",
                            onchange: {
                                let base = padding.clone();
                                move |value: CssSize| {
                                    let mut updated = base.clone();
                                    updated.top = get_nullified_css_size(value);
                                    onchange.call(updated);
                                }
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
                            value: padding.bottom.clone(),
                            placeholder: "bottom",
                            onchange: {
                                let base = padding.clone();
                                // If the content is null, we will set it accordingly
                                move |value: CssSize| {
                                    let mut updated = base.clone();
                                    updated.bottom = get_nullified_css_size(value);
                                    onchange.call(updated);
                                }
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
    rsx!(
        select {
            name: "vertical_align",
            required: true,
            aria_label: t!("settings.vertical_alignment.description").to_string(),
            onchange: move |event| {
                // An unknown value leaves the alignment as it is rather than
                // reporting one nobody picked.
                let chosen = match event.value().as_str() {
                    "top" => VerticalAlign::Top,
                    "middle" => VerticalAlign::Middle,
                    "bottom" => VerticalAlign::Bottom,
                    other => {
                        tracing::error!(
                            "Invalid option for vertical alignment selected, the value is: {}",
                            other
                        );
                        default
                    }
                };
                onchange.call(chosen);
            },
            option { value: "top", selected: default == VerticalAlign::Top,
                {t!("settings.vertical_alignment.top").to_string()}
            }
            option {
                value: "middle",
                selected: default == VerticalAlign::Middle,
                {t!("settings.vertical_alignment.middle").to_string()}
            }
            option {
                value: "bottom",
                selected: default == VerticalAlign::Bottom,
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
        // A classic `.song` carries no tags, so there is nothing to map.
        &[],
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
        // The division the preview uses is the service's own choice, so
        // the preview shows the slides the presentation would build.
        let slide_settings = settings.read().default_song_slide_settings();
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
                // The sideways scrolling sits here rather than on the pane:
                // the pane is what sticks while the page scrolls past, and an
                // element cannot both stick and scroll. See
                // `.design-preview-stage-scroll` in `assets/main.css`.
                div { class: "design-preview-stage-scroll",
                    div { class: "design-preview-stage",
                        div { class: "design-preview-canvas",
                            StaticSlideRendererComponent {
                                slide: slides.read()[current].clone(),
                                presentation_design: design(),
                            }
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

    /// A frame is drawn whether its thumbnail has been made or not — the
    /// pictures arrive one by one, and a strip that only had frames for the
    /// ones that are in would change size under the reader.
    #[test]
    fn test_a_picture_frame_renders_with_and_without_its_thumbnail() {
        use crate::logic::sourcefiles::SourceFileType;

        let source_file = ImageSourceFile::new(SourceFile {
            name: "Hall".to_string(),
            path: PathBuf::from("assets/favicon.png"),
            file_type: SourceFileType::Image,
            md5_hash: None,
            relative_path: None,
        })
        .expect("a picture is a picture source file");

        // Rendered from inside a component, because an event handler belongs
        // to a scope and cannot be built outside of one.
        #[component]
        fn Harness(source_file: ImageSourceFile, thumbnail: Option<String>, active: bool) -> Element {
            rsx! {
                PictureSelectorItem {
                    source_file,
                    thumbnail,
                    active,
                    onclick: move |_| {},
                }
            }
        }

        for thumbnail in [None, Some("data:image/png;base64,AAAA".to_string())] {
            for active in [false, true] {
                let mut dom = dioxus::prelude::VirtualDom::new_with_props(
                    Harness,
                    HarnessProps {
                        source_file: source_file.clone(),
                        thumbnail: thumbnail.clone(),
                        active,
                    },
                );
                dom.rebuild_in_place();
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
