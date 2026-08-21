//! Shared components reusable across different parts of the program.

use crate::components::presentation_components::{
    PresentationRendererComponent, PresentationRole,
};
use crate::logic::presentation::{create_amazing_grace_presentation, create_single_item_presentation};
use crate::logic::settings::{CssSize, PresentationDesign, use_settings};
use crate::logic::states::{RunningPresentation, SelectedItemRepresentation};
use cantara_songlib::slides::SlideSettings;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::{
    FaFileCode, FaFilePdf, FaFilm, FaImage, FaMusic, FaPenToSquare,
};
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// Whether the page has been drawn once already.
///
/// A section that is expensive to build asks this and shows a placeholder
/// until it is true, so that the page appears at once and fills in rather than
/// staying blank while the whole of it is laid out. The design editor is the
/// case that needs it: a library's worth of picture frames and a full-size
/// slide preview are more work than a web view gets through in a frame, and on
/// the Linux web view considerably more.
///
/// Returns `false` on the first render and `true` from the next tick on. The
/// wait is a timer rather than a script in the page — see
/// [`crate::logic::timer`].
pub fn use_after_first_paint() -> ReadSignal<bool> {
    let mut drawn = use_signal(|| false);

    // `use_hook` rather than `use_effect`: this has to happen exactly once per
    // mount, and `use_hook` says so outright. An effect would be right too —
    // it re-runs only when a signal it read has changed, and this reads none —
    // but that is a rule about the body of the closure, and a later edit that
    // read a signal in here would quietly start a timer per change.
    use_hook(|| {
        spawn(async move {
            // One frame's worth: long enough that the first paint is out,
            // short enough not to be a visible pause of its own.
            crate::logic::timer::sleep(std::time::Duration::from_millis(16)).await;
            drawn.set(true);
        });
    });

    drawn.into()
}

/// A stand-in for a block of settings that has not been drawn yet.
///
/// Sized like the thing it replaces, so the page is laid out once instead of
/// jumping as each section arrives.
#[component]
pub fn SettingsSkeleton(
    /// How many fields to stand in for.
    fields: usize,
) -> Element {
    rsx! {
        div { class: "skeleton-font-block", aria_hidden: "true",
            span { class: "skeleton skeleton-heading" }
            for field in 0..fields {
                span { key: "{field}", class: "skeleton skeleton-field" }
            }
        }
    }
}

#[component]
pub fn DeleteIcon() -> Element {
    rsx! { Icon { icon: FaTrashCan } }
}

#[component]
pub fn EditIcon() -> Element {
    rsx! { Icon { icon: FaPenToSquare } }
}

#[component]
pub fn MusicIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaMusic, width: width.unwrap_or(20) } }
}

#[component]
pub fn ImageIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaImage, width: width.unwrap_or(20) } }
}

#[component]
pub fn PdfIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaFilePdf, width: width.unwrap_or(20) } }
}

#[component]
pub fn MarkdownIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaFileCode, width: width.unwrap_or(20) } }
}

#[component]
pub fn VideoIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaFilm, width: width.unwrap_or(20) } }
}

/// A component displaying multiple presentation designs in an "Amazing Grace" presentation.
#[component]
pub fn PresentationDesignSelector(
    /// Read-only, so that the list may be handed a memo over the settings
    /// rather than a copy that has to be written back.
    presentation_designs: ReadSignal<Vec<PresentationDesign>>,
    song_slide_settings: Option<SlideSettings>,
    viewer_width: usize,
    active_item: Signal<Option<usize>>,
) -> Element {
    let song_slide_settings = use_signal(|| song_slide_settings.unwrap_or_default());

    rsx! {
        div {
            class: "presentation-design-selector",
            for (index, design) in presentation_designs.read().iter().enumerate() {
                span {
                    class: format!("presentation-design-selector-item {}", if active_item() == Some(index) { "active" } else { "" }),
                    tabindex: index,
                    key: "{index}",
                    // `content-visibility` used to sit here and was taken out
                    // again: this element has no size of its own, so a skipped
                    // tile collapsed and the page height changed as it scrolled
                    // into view. It now sits one level in, on the frame that
                    // states its size in pixels — see [`PresentationViewer`],
                    // where the same idea works because the box cannot
                    // collapse. Do not put it back here.
                    SelectablePresentationViewer {
                        presentation: create_amazing_grace_presentation(design, &song_slide_settings()),
                        width: viewer_width,
                        title: design.name.clone(),
                        index,
                        current_selection: active_item
                    }
                }
            }
        }
    }
}

/// A wrapper component around PresentationViewer that allows selecting it.
#[component]
fn SelectablePresentationViewer(
    presentation: RunningPresentation,
    width: usize,
    title: String,
    index: usize,
    current_selection: Signal<Option<usize>>,
) -> Element {
    rsx! {
        PresentationViewer {
            presentation,
            width,
            title: Some(title),
            selected: Some(index == current_selection().unwrap_or(usize::MAX)),
            // A design is judged on more than its title slide: this is where
            // the verses, the spoiler line and the empty last slide can be
            // looked at without opening the editor.
            navigable: true,
            onclick: move |_| {
                tracing::debug!("Selected Presentation: {}", index);
                current_selection.set(Some(index));
            }
        }
    }
}

#[component]
pub fn PresentationViewer(
    presentation: RunningPresentation,
    width: usize,
    title: Option<String>,
    selected: Option<bool>,
    onclick: Option<EventHandler<MouseEvent>>,
    /// Whether the preview can be paged through.
    ///
    /// Off by default: most previews stand for a *thing* — the design in the
    /// list, the example beside a setting — and a control on them would invite
    /// a click that means nothing. Where the point is to look at more than the
    /// first slide, the list of designs, it is on.
    #[props(default)]
    navigable: bool,
) -> Element {
    // Rendered at the presentation's own resolution and scaled down to the
    // width that was asked for.
    //
    // The scaling is a `transform` and not `zoom`, which is what it used to be.
    // `zoom` was the neater of the two: it changes an element's layout size, so
    // the box simply became the scaled size and nothing around it had to know.
    // But WebKitGTK — the engine behind the Linux build — re-lays-out a zoomed
    // subtree as the page scrolls, and each of these is a whole presentation.
    // That is what made the settings pages stall and then jump while scrolling,
    // and it is why a preview left at `zoom: 1` scrolled smoothly.
    //
    // A `transform` never touches layout, so the cost is paid once. In exchange
    // the scaled box no longer occupies the space it appears to, and the frame
    // around it has to state that size — which is what `frame_w`/`frame_h` are.
    let (native_w, native_h) = presentation.presentation_resolution;
    let scale = width as f64 / native_w as f64;
    let frame_w = width;
    let frame_h = (native_h as f64 * scale).round() as usize;
    let css_class = selected.map_or("rounded-corners-inactive", |s| {
        if s {
            "rounded-corners-active"
        } else {
            "rounded-corners-inactive"
        }
    });

    let mut presentation_signal = use_signal(|| presentation.clone());
    // Only the *content* is taken from the prop. Where the preview stands
    // within it belongs to the preview: comparing the whole thing put a
    // preview that had been paged forward back to its first slide the next
    // time anything redrew this — see [`SelectedItemPreview`], which had to
    // learn the same thing.
    if presentation_signal.peek().presentation != presentation.presentation {
        presentation_signal.set(presentation.clone());
    }

    rsx! {
        // The frame is what the page sees: an empty box of the finished size.
        // It carries nothing but that size, because everything else has to be
        // *inside* the scaling to look as it did — the selection border is
        // 18 pixels at presentation scale, which came out at four and a half.
        // Left on the frame it would be four times too heavy.
        div {
            class: "inline-div",
            // `content-visibility: auto` lets the engine skip laying out and
            // painting a tile that is off screen. Each of these is a whole
            // presentation, rendered at the *target screen's* resolution and
            // shrunk with a transform — so on a 4K projector every tile is a
            // 3840 x 2160 subtree, and a list of designs is that many. The cost
            // of scrolling was measured in the area of the screen the service
            // will be shown on, which is why a big projector made this page
            // crawl and a small one did not.
            //
            // This was tried once before and taken out again, because it sat on
            // the wrapper around this box — an element with no size of its own,
            // which therefore collapsed to nothing while it was skipped and
            // changed the page height as it came back into view. Here it is on
            // the frame, whose width and height are written out in pixels just
            // below: skipping the contents cannot change the size of a box that
            // states its own, whatever shape the design is.
            style: format!(
                "position: relative; width: {frame_w}px; height: {frame_h}px;                  content-visibility: auto;                  contain-intrinsic-size: {frame_w}px {frame_h}px;"
            ),
            onclick: move |event| if let Some(onclick_event) = onclick { onclick_event.call(event) },
            div {
                // Everything in here is laid out in the presentation's own
                // pixels, exactly as before — only the way it is shrunk has
                // changed.
                class: format!("{} presentation-preview", css_class),
                style: format!(
                    "position: absolute; top: 0; left: 0; width: {native_w}px; \
                     height: {native_h}px; transform: scale({scale}); \
                     transform-origin: top left;"
                ),
            PresentationRendererComponent {
                running_presentation: presentation_signal,
                role: PresentationRole::Follower,
            }
            if let Some(title) = title {
                div {
                    class: "presentation-title",
                    style: "position: absolute; top: 0; right: 0; display: flex; align-items: center; justify-content: center; font-size: 30pt; background-color: black; color: white; z-index: 99;",
                    { title }
                }
            }
            if navigable {
                // Across the lower third of the preview, and only there while
                // the pointer is on it — see `.preview-navigation`. Everything
                // here is sized in the presentation's own pixels, because the
                // preview around it is scaled down: a 24-pixel button would
                // come out at five.
                div { class: "preview-navigation",
                    button {
                        r#type: "button",
                        class: "preview-navigation-button",
                        aria_label: t!("settings.design_preview.previous").to_string(),
                        onclick: move |event: Event<MouseData>| {
                            // The preview as a whole selects the design; a
                            // click on the arrow only turns the page.
                            event.stop_propagation();
                            presentation_signal.write().previous_slide();
                        },
                        "‹"
                    }
                    button {
                        r#type: "button",
                        class: "preview-navigation-button",
                        aria_label: t!("settings.design_preview.next").to_string(),
                        onclick: move |event: Event<MouseData>| {
                            event.stop_propagation();
                            presentation_signal.write().next_slide();
                        },
                        "›"
                    }
                }
            }
            }
        }
    }
}

/// Displays an example presentation in 16:9 format scaled to a fixed width.
#[component]
pub fn ExamplePresentationViewer(
    presentation_design: PresentationDesign,
    song_slide_settings: Option<Signal<SlideSettings>>,
    width: usize,
    increase_font_size_in_percent: Option<usize>,
) -> Element {
    let presentation = create_amazing_grace_presentation(
        &presentation_design,
        &song_slide_settings.map_or(SlideSettings::default(), |s| s()),
    );

    rsx! {
        PresentationViewer {
            presentation,
            width,
        }
    }
}

/// Displays a live preview of the currently selected item with its actual slides,
/// transition effects, and countdown timer bar. Click advances to the next slide.
#[component]
pub fn SelectedItemPreview(
    selected_item: SelectedItemRepresentation,
    default_presentation_design: PresentationDesign,
    default_slide_settings: SlideSettings,
    width: usize,
) -> Element {
    let timer_seconds = selected_item
        .timer_settings_option
        .as_ref()
        .map(|t| t.timer_seconds);

    // The preview is what the audience will see, so it reads the song by
    // the same rules the presentation will — tag mappings included.
    let settings = use_settings();
    let presentation = create_single_item_presentation(
        &selected_item,
        &default_presentation_design,
        &default_slide_settings,
        &settings.read().tag_mappings,
    );

    let mut presentation_signal = use_signal(|| presentation.clone());
    // Only reset when slide content/settings change, not when position changes due to clicks
    if presentation_signal.peek().presentation != presentation.presentation {
        presentation_signal.set(presentation.clone());
    }

    let current_slide_number = use_memo(move || {
        presentation_signal
            .read()
            .position
            .as_ref()
            .map(|p| p.slide_total())
            .unwrap_or(0)
    });

    let total_slides = use_memo(move || presentation_signal.read().total_slides());

    // The frame at its finished size, with the full-size slide scaled down
    // inside it. See `PresentationViewer` for why this is a `transform` and
    // not `zoom`.
    let (native_w, native_h) = presentation.presentation_resolution;
    let scale = width as f64 / native_w as f64;
    let frame_w = width;
    let frame_h = (native_h as f64 * scale).round() as usize;

    rsx! {
        div {
            // The rounding sits on the frame rather than inside the scaling.
            // It used to be scaled along with everything else, which turned a
            // requested 8 pixels into two — the corners were as good as square.
            class: "presentation-preview",
            style: format!(
                "position: relative; width: {frame_w}px; height: {frame_h}px; \
                 cursor: pointer; overflow: hidden; border-radius: 8px;"
            ),
            div {
                style: format!(
                    "position: absolute; top: 0; left: 0; width: {native_w}px; \
                     height: {native_h}px; transform: scale({scale}); \
                     transform-origin: top left;"
                ),
            PresentationRendererComponent {
                running_presentation: presentation_signal,
                // It shows what a slide timer does, so it has to run —
                // but it is a preview on the selection page, not the
                // screen the audience is looking at.
                role: PresentationRole::SelfRunning,
            }
            // Countdown timer bar at the bottom
            if let Some(seconds) = timer_seconds {
                div {
                    key: "{current_slide_number()}",
                    style: format!(
                        "position: absolute; bottom: 0; left: 0; height: 6px; width: 100%; background: rgba(255, 255, 255, 0.7); z-index: 100; animation: countdownBar {}s linear forwards;",
                        seconds
                    ),
                }
            }
            // Slide counter overlay
            if total_slides() == 0 {
                div {
                    style: "position: absolute; bottom: 8px; right: 8px; background: rgba(0, 0, 0, 0.6); color: white; padding: 2px 8px; border-radius: 4px; font-size: 20px; z-index: 100;",
                    { "0 / 0" }
                }
            } else {
                div {
                    style: "position: absolute; bottom: 8px; right: 8px; background: rgba(0, 0, 0, 0.6); color: white; padding: 2px 8px; border-radius: 4px; font-size: 20px; z-index: 100;",
                    { format!("{} / {}", current_slide_number() + 1, total_slides()) }
                }
            }
            }
        }
    }
}

/// Puts a file wherever the platform puts files.
///
/// `Ok(true)` when it was written, `Ok(false)` when the user closed the file
/// dialog — which is not a failure and needs no message.
///
/// The desktop asks where it should go and writes it itself; every other build
/// hands it to the platform as a download. Bytes rather than text, because
/// some of what Cantara writes is a ZIP archive.
#[cfg(feature = "desktop")]
pub fn save_file(name: &str, bytes: &[u8]) -> Result<bool, String> {
    let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file() else {
        return Ok(false);
    };
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(true)
}

/// Without a file dialog the file travels into the page as base64 and leaves
/// it as a download.
#[cfg(not(feature = "desktop"))]
pub fn save_file(name: &str, bytes: &[u8]) -> Result<bool, String> {
    use base64::Engine as _;

    let name = serde_json::to_string(name).map_err(|error| error.to_string())?;
    let data = serde_json::to_string(&base64::engine::general_purpose::STANDARD.encode(bytes))
        .map_err(|error| error.to_string())?;

    spawn(async move {
        let js = format!(
            r#"
            (function() {{
                const raw = atob({data});
                const bytes = new Uint8Array(raw.length);
                for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
                const blob = new Blob([bytes], {{ type: 'application/octet-stream' }});
                const url = URL.createObjectURL(blob);
                const link = document.createElement('a');
                link.href = url;
                link.download = {name};
                document.body.appendChild(link);
                link.click();
                document.body.removeChild(link);
                URL.revokeObjectURL(url);
            }})();
            "#
        );
        let _ = document::eval(&js).await;
    });

    Ok(true)
}

/// A translated message whose parameters are only known at runtime.
///
/// `t!` needs its parameter names spelled out at compile time, which is fine
/// for a message written next to the code that shows it — and no use at all
/// for one built somewhere that has no business knowing about languages: an
/// error from a file reader, a line of a summary. Those carry their parameters
/// as data, and this is where the two meet.
pub fn translate(key: &str, parameters: &[(&str, String)]) -> String {
    let mut message = rust_i18n::t!(key).to_string();
    for (name, value) in parameters {
        message = message.replace(&format!("%{{{name}}}"), value);
    }
    message
}

// The dialogs used to be built here, as pieces of JavaScript handed to the web
// view's `alert()`, `confirm()` and `prompt()`. They are Dioxus components now;
// see [`crate::components::dialogs`].

#[component]
pub fn NumberedValidatedLengthInput(
    value: CssSize,
    placeholder: String,
    onchange: EventHandler<CssSize>,
) -> Element {
    let mut value_signal = use_signal(|| value);
    rsx! {
        input {
            placeholder,
            value: value_signal.read().get_float(),
            inputmode: "numeric",
            onchange: move |event| {
                value_signal.write().set_float(event.value().parse().unwrap_or(0.0));
                onchange.call(value_signal());
            }
        }
        select {
            name: "unit",
            required: true,
            onchange: move |event: Event<FormData>| {
                match event.value().as_str() {
                    "px" => value_signal.set(CssSize::Px(value_signal().get_float())),
                    "pt" => value_signal.set(CssSize::Pt(value_signal().get_float())),
                    "em" => value_signal.set(CssSize::Em(value_signal().get_float())),
                    "%"  => value_signal.set(CssSize::Percentage(value_signal().get_float())),
                    _    => value_signal.set(CssSize::Px(value_signal().get_float()))
                };
                onchange.call(value_signal());
            },
            option {
                key: "px",
                selected: matches!(value_signal(), CssSize::Px(_)) || value_signal() == CssSize::Null,
                "px"
            }
            option {
                key: "pt",
                selected: matches!(value_signal(), CssSize::Pt(_)),
                "pt"
            }
            option {
                key: "em",
                selected: matches!(value_signal(), CssSize::Em(_)),
                "em"
            }
            option {
                key: "%",
                selected: matches!(value_signal(), CssSize::Percentage(_)),
                "%"
            }
        }
    }
}
