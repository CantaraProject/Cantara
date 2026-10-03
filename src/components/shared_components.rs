//! Shared components reusable across different parts of the program.

use crate::components::presentation_components::{
    PresentationRendererComponent, PresentationRole,
};
use crate::components::reorder::{GRIP_STYLE, ReorderDrag, refuse_native_drag, use_reorder_drag};
use crate::logic::presentation::{create_amazing_grace_presentation, create_single_item_presentation};
use crate::logic::reorder::Layout;
use crate::logic::settings::{CssSize, PresentationDesign, use_settings};
use crate::logic::states::{RunningPresentation, SelectedItemRepresentation};
use cantara_songlib::slides::SlideSettings;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::{
    FaFileCode, FaFilePdf, FaFilm, FaGripVertical, FaImage, FaMusic, FaPenToSquare,
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
    /// A design was dragged: the one at the first position, into the gap at
    /// the second — see [`crate::logic::reorder`]. The caller changes the
    /// list; the selector only says what was asked for.
    on_move: EventHandler<(usize, usize)>,
) -> Element {
    let song_slide_settings = use_signal(|| song_slide_settings.unwrap_or_default());
    let len = presentation_designs.read().len();
    let drag = use_reorder_drag(Layout::Grid, DESIGN_LIST_ID, len, move |gap| on_move.call(gap));

    rsx! {
        div {
            id: DESIGN_LIST_ID,
            class: "presentation-design-selector {drag.container_class()}",
            onpointermove: move |event: Event<PointerData>| drag.moved(&event.data()),
            onpointerup: move |_| drag.released(),
            onpointercancel: move |_| drag.cancelled(),
            onpointerenter: move |event: Event<PointerData>| drag.entered(&event.data()),
            ondragstart: refuse_native_drag,

            for (index, design) in presentation_designs.read().iter().enumerate() {
                DesignTile {
                    key: "{index}",
                    design: design.clone(),
                    index,
                    song_slide_settings: song_slide_settings(),
                    viewer_width,
                    active_item,
                    drag,
                }
            }
        }
    }
}

/// One design in the list: its preview, and the grip to carry it by.
///
/// A component of its own so that a drag redraws only what it changes. The
/// carried tile follows the pointer, and if the list itself read where the
/// pointer is, every move would build every preview again.
#[component]
fn DesignTile(
    design: PresentationDesign,
    index: usize,
    song_slide_settings: SlideSettings,
    viewer_width: usize,
    active_item: Signal<Option<usize>>,
    drag: ReorderDrag,
) -> Element {
    // The preview is built from the design, and only built again when the
    // design changes — not each time the tile is drawn somewhere else.
    let build = |design: &PresentationDesign, settings: &SlideSettings| {
        (design.clone(), settings.clone(), create_amazing_grace_presentation(design, settings))
    };
    let mut preview = use_signal(|| build(&design, &song_slide_settings));
    let stale = {
        let current = preview.peek();
        current.0 != design || current.1 != song_slide_settings
    };
    if stale {
        preview.set(build(&design, &song_slide_settings));
    }
    let presentation = preview.peek().2.clone();

    rsx! {
        span {
            class: if active_item() == Some(index) {
                "presentation-design-selector-item active {drag.item_class(index)}"
            } else {
                "presentation-design-selector-item {drag.item_class(index)}"
            },
            style: drag.item_style(index),
            // Every tile in the order of the page. It used to be the
            // tile's position, which made every tile after the first a
            // *positive* tab index — reached before anything else on
            // the page.
            tabindex: 0,
            "data-reorder-index": "{index}",
            onmounted: move |event: Event<MountedData>| drag.mounted(index, event),
            // A mouse or a pen picks the tile up anywhere on it; a
            // finger only by the grip, so that the page still scrolls
            // under a finger on the tile.
            onpointerdown: move |event: Event<PointerData>| drag.press(index, &event.data(), false),
            onkeydown: move |event: Event<KeyboardData>| {
                drag.keydown(index, &event, false);
            },
            // The grip a finger drags the tile by. Shown always where
            // the pointer is a finger, and on hover elsewhere — see
            // `.presentation-design-grip` in `assets/main.css`.
            span {
                class: "presentation-design-grip reorder-grip",
                style: GRIP_STYLE,
                aria_label: t!("selection.reorder_handle").to_string(),
                title: t!("selection.reorder_hint").to_string(),
                onpointerdown: move |event: Event<PointerData>| {
                    // The tile's own handler would otherwise start the
                    // same drag a second time.
                    event.stop_propagation();
                    drag.press(index, &event.data(), true);
                },
                // Pressing the grip is not choosing the design.
                onclick: move |event: Event<MouseData>| event.stop_propagation(),
                Icon { icon: FaGripVertical }
            }
            // `content-visibility` used to sit here and was taken out
            // again: this element has no size of its own, so a skipped
            // tile collapsed and the page height changed as it scrolled
            // into view. It now sits one level in, on the frame that
            // states its size in pixels — see [`PresentationViewer`],
            // where the same idea works because the box cannot
            // collapse. Do not put it back here.
            SelectablePresentationViewer {
                presentation,
                width: viewer_width,
                title: design.name.clone(),
                index,
                current_selection: active_item,
                drag,
            }
        }
    }
}

/// The `id` of the list of designs, which the drag finds its scrolling area
/// from.
const DESIGN_LIST_ID: &str = "presentation-design-list";

/// A wrapper component around PresentationViewer that allows selecting it.
#[component]
fn SelectablePresentationViewer(
    presentation: RunningPresentation,
    width: usize,
    title: String,
    index: usize,
    current_selection: Signal<Option<usize>>,
    drag: ReorderDrag,
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
                // The release at the end of a drag is a click as far as the
                // browser is concerned. It is not choosing a design.
                if drag.swallows_click() {
                    return;
                }
                tracing::debug!("Selected Presentation: {}", index);
                current_selection.set(Some(index));
            }
        }
    }
}

/// What an entry in a settings list is called and what it is for.
///
/// A presentation design and a slide division are different things, but a list
/// of either is unusable without these two fields, and they were written out
/// once for each. `on_changed` is given both every time — the two are edited
/// together and stored together, and a caller that has to remember which of
/// the pair it just received is a caller waiting to write the wrong one.
#[component]
pub fn MetadataFieldset(
    name: String,
    description: String,
    /// Shown in the name field while it is empty. The name may be left empty —
    /// the list then falls back to the entry's position — so this is where a
    /// list says what it will call the entry until it is named.
    #[props(default)]
    name_placeholder: String,
    /// Anything else that belongs among an entry's meta information, drawn
    /// under the description.
    ///
    /// Only a presentation design has a third field here — what kind of view
    /// it describes — and a slide division has no use for one. Passed in
    /// rather than added to this component so that the two lists that share it
    /// do not both grow a field only one of them means.
    #[props(default)]
    extra: Option<Element>,
    on_changed: EventHandler<(String, String)>,
) -> Element {
    rsx! {
        h3 { {t!("general.meta_information").to_string()} }
        form {
            fieldset {
                label {
                    {t!("general.name").to_string()}
                    input {
                        value: name.clone(),
                        placeholder: name_placeholder,
                        onchange: {
                            let description = description.clone();
                            move |event: Event<FormData>| {
                                on_changed.call((event.value(), description.clone()));
                            }
                        },
                    }
                }

                label {
                    {t!("general.description").to_string()}
                    input {
                        value: description.clone(),
                        onchange: {
                            let name = name.clone();
                            move |event: Event<FormData>| {
                                on_changed.call((name.clone(), event.value()));
                            }
                        },
                    }
                }

                if let Some(extra) = extra {
                    {extra}
                }
            }
        }
    }
}

/// A slider with the value it stands for written beside its name.
///
/// Every setting that is a number within known bounds is offered this way, and
/// each one used to answer for itself what an unreadable event means - one
/// snapped to `1.0`, the next to `100.0`, a third to the field's default. The
/// answer that is right everywhere is *nothing*: a value that cannot be read
/// is not a value the user asked for, so `onchange` is simply not called and
/// the setting keeps what it had.
///
/// The value goes in and comes out as `f64`; a setting stored as something
/// narrower converts at the call site, where the field being written is in
/// view.
#[component]
pub fn RangeInput(
    /// What the setting is called. The current value is appended to it.
    label: String,
    /// What the value is measured in, written straight after it. Empty for a
    /// bare number.
    #[props(default)]
    unit: String,
    min: f64,
    max: f64,
    step: f64,
    value: f64,
    onchange: EventHandler<f64>,
) -> Element {
    rsx! {
        label {
            {format!("{label}: {value}{unit}")}
            input {
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                oninput: move |event: Event<FormData>| {
                    if let Ok(value) = event.value().parse::<f64>() {
                        onchange.call(value);
                    }
                },
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
            // Drawn the way the design says, which for a monitor design is
            // its layout rather than a single slide. The card in the settings
            // used to show one slide for every design, including the ones
            // that do not describe one — see [`DesignedPresentation`].
            crate::components::monitor_view::DesignedPresentation {
                running_presentation: presentation_signal,
                role: PresentationRole::Follower,
                // Inside a box that is scaled down, not filling a window.
                contained: true,
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
                        // Turning the page is not picking the tile up.
                        onpointerdown: move |event: Event<PointerData>| event.stop_propagation(),
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
                        onpointerdown: move |event: Event<PointerData>| event.stop_propagation(),
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

    // Which slide of how many, counted the way every other counter in the
    // program counts — see [`crate::logic::states::RunningPresentation::counter_in`].
    let counter = use_memo(move || {
        presentation_signal
            .read()
            .counter_in(crate::logic::states::Division::Projection)
            .unwrap_or((0, 0))
    });
    let current_slide_number = use_memo(move || counter().0);
    let total_slides = use_memo(move || counter().1);

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
                    { format!("{} / {}", current_slide_number(), total_slides()) }
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

/// A length: a number with a stepper, and the unit it is in.
///
/// The number is an ordinary `type="number"` field, which is where its up and
/// down arrows come from — the same field, and so the same arrows, as
/// "maximum lines per slide" and every other number in the settings. A pair of
/// buttons of its own was tried and looked exactly like what it was: a control
/// this program does not otherwise have.
///
/// `min` is what keeps the arrows from running past nothing. There is no type
/// of a negative size and no margin that pushes text off its own slide, and an
/// arrow that produces one has to be undone by hand.
///
/// The value is read straight from the prop rather than copied into a signal
/// on the first render. A copy would freeze at whatever the design said when
/// the field first appeared, so anything that changed the length elsewhere —
/// importing a design, an edit to another block — would leave this showing the
/// old number. It is the same mistake that stopped the design preview
/// following the font settings; see the note in
/// [`crate::components::presentation_components`].
#[component]
pub fn NumberedValidatedLengthInput(
    value: CssSize,
    placeholder: String,
    onchange: EventHandler<CssSize>,
) -> Element {
    // Each handler needs its own, since a closure cannot borrow from the render.
    let for_input = value.clone();
    let for_unit = value.clone();

    rsx! {
        input {
            r#type: "number",
            min: "0",
            // One press, one point or one pixel.
            step: "1",
            placeholder,
            value: value.get_float(),
            onchange: move |event: Event<FormData>| {
                let mut updated = for_input.clone();
                updated.set_float(event.value().parse().unwrap_or(0.0));
                onchange.call(updated);
            }
        }

        select {
            name: "unit",
            required: true,
            onchange: move |event: Event<FormData>| {
                let number = for_unit.get_float();
                onchange.call(match event.value().as_str() {
                    "pt" => CssSize::Pt(number),
                    "em" => CssSize::Em(number),
                    "%" => CssSize::Percentage(number),
                    // `px` and anything unrecognised.
                    _ => CssSize::Px(number),
                });
            },
            option {
                key: "px",
                selected: matches!(value, CssSize::Px(_)) || value == CssSize::Null,
                "px"
            }
            option {
                key: "pt",
                selected: matches!(value, CssSize::Pt(_)),
                "pt"
            }
            option {
                key: "em",
                selected: matches!(value, CssSize::Em(_)),
                "em"
            }
            option {
                key: "%",
                selected: matches!(value, CssSize::Percentage(_)),
                "%"
            }
        }
    }
}

#[cfg(test)]
mod length_field_tests {
    use super::*;

    /// The number is a `type="number"` field that steps by one and stops at
    /// nothing — which is where its up and down arrows come from, and why they
    /// are the same arrows as everywhere else in the settings rather than a
    /// pair of buttons this program has nowhere else.
    #[test]
    fn the_field_is_an_ordinary_number_field_with_a_step_of_one() {
        #[component]
        fn Harness() -> Element {
            rsx! {
                NumberedValidatedLengthInput {
                    value: CssSize::Pt(31.5),
                    placeholder: "",
                    onchange: |_| {},
                }
            }
        }

        let mut dom = VirtualDom::new(Harness);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);

        assert!(html.contains(r#"type="number""#), "got {html}");
        assert!(html.contains(r#"step="1""#), "one press is one unit: {html}");
        // No arrow may take a size below nothing.
        assert!(html.contains(r#"min="0""#), "got {html}");
        assert!(html.contains("31.5"), "the number is not shown: {html}");
        // The unit it is in is the one that comes up selected.
        assert!(html.contains("<option selected=true>pt"), "got {html}");
    }

    /// The field follows the value it is given rather than the one it was
    /// first given. It used to copy the prop into a signal on the first
    /// render, which froze it: editing one font block would leave every other
    /// field showing its original number until the page was left and reopened.
    #[test]
    fn the_field_follows_a_value_that_changes_underneath_it() {
        use std::cell::RefCell;

        thread_local! {
            static VALUE: RefCell<CssSize> = const { RefCell::new(CssSize::Pt(31.5)) };
        }

        #[component]
        fn Harness() -> Element {
            rsx! {
                NumberedValidatedLengthInput {
                    value: VALUE.with(|value| value.borrow().clone()),
                    placeholder: "",
                    onchange: |_| {},
                }
            }
        }

        let mut dom = VirtualDom::new(Harness);
        dom.rebuild_in_place();
        assert!(dioxus_ssr::render(&dom).contains("31.5"));

        VALUE.with(|value| *value.borrow_mut() = CssSize::Px(32.5));
        dom.mark_dirty(ScopeId::APP);
        dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);

        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains("32.5") && !html.contains("31.5"),
            "the field kept the number it was first given: {html}"
        );
        // …and the unit follows with it.
        assert!(html.contains("<option selected=true>px"), "got {html}");
    }
}

#[cfg(test)]
mod design_selector_tests {
    use super::*;

    /// Three designs, the second one chosen, as the settings page draws them.
    fn rendered() -> String {
        #[component]
        fn Harness() -> Element {
            let designs = use_signal(|| {
                (0..3)
                    .map(|number| PresentationDesign {
                        name: format!("design {number}"),
                        ..PresentationDesign::default()
                    })
                    .collect::<Vec<_>>()
            });
            let active = use_signal(|| Some(1));
            rsx! {
                PresentationDesignSelector {
                    presentation_designs: designs,
                    viewer_width: 400,
                    active_item: active,
                    on_move: move |_| {},
                }
            }
        }

        let mut dom = VirtualDom::new(Harness);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// The tiles one after another, each with its position, and each a single
    /// stop in the order of the page. Every tile after the first used to carry
    /// its position as a *positive* tab index, which put it before everything
    /// else on the page.
    #[test]
    fn test_every_tile_is_one_stop_in_the_order_of_the_page() {
        let html = rendered();

        assert!(html.contains(r#"id="presentation-design-list""#), "{html}");
        for index in 0..3 {
            assert!(html.contains(&format!(r#"data-reorder-index="{index}""#)), "{html}");
        }
        assert!(!html.contains(r#"tabindex="1""#), "{html}");
        assert!(!html.contains(r#"tabindex="2""#), "{html}");
    }

    /// Every tile has a grip that a finger can drag it by, and it is the one
    /// part of the tile that claims a touch — the rest of the page must
    /// scroll under a finger on a tile.
    #[test]
    fn test_every_tile_has_a_grip() {
        let html = rendered();

        assert_eq!(html.matches("presentation-design-grip").count(), 3, "{html}");
        assert_eq!(html.matches("touch-action: none").count(), 3, "{html}");
    }

    /// The chosen design is marked, and only that one.
    #[test]
    fn test_the_chosen_tile_is_marked() {
        let html = rendered();

        assert_eq!(html.matches("presentation-design-selector-item active").count(), 1, "{html}");
    }

    /// Outside a drag there is nothing to show for one.
    #[test]
    fn test_no_drag_is_drawn_when_none_is_under_way() {
        let html = rendered();

        assert!(!html.contains("reorder-marker"), "{html}");
        assert!(!html.contains("reorder-dragging"), "{html}");
        assert!(!html.contains("reorder-active"), "{html}");
        assert!(!html.contains("translate("), "{html}");
    }
}
