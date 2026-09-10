//! The screen the people making the service happen are looking at.
//!
//! A projection is for the congregation: one slide, as large as the wall
//! allows, and nothing else on it. The people on the platform need the
//! opposite — what is up, what is coming, and how long this has been going on.
//! That is the same presentation read differently, and this module is the
//! different reading.
//!
//! Nothing here controls anything. A monitor view receives the presentation
//! and draws it; the one place a service is driven from is the presenter
//! console. A stage monitor that could be leant on would be a second one, and
//! two people who can both press "next" is a bug report waiting in a service.
//!
//! See `docs/specs/0003-add-monitor-view.md`.

use dioxus::prelude::*;
use rust_i18n::t;

use crate::components::presentation_components::StaticSlideRendererComponent;
use crate::components::presenter_console_components::SlideList;
use crate::logic::settings::{
    MonitorDesign, MonitorLayout, MonitorWidget, PresentationDesign, SpeakerNextPosition,
    WidgetKind, WidgetPlacement,
};
use crate::logic::states::RunningPresentation;
use cantara_songlib::slides::Slide;
use crate::logic::timer::Timestamp;

rust_i18n::i18n!("locales", fallback = "en");

/// What a monitor view looks like.
const MONITOR_VIEW_CSS: Asset = asset!("/assets/monitor_view.css");

/// Fits a slide into its box. See the file itself for why it is measured
/// rather than expressed in CSS, and why it is one file rather than two.
pub(crate) const SLIDE_SCALE_JS: &str = include_str!("../../assets/monitor_slide_scale.js");

/// How often the widgets are redrawn.
///
/// A clock showing minutes and a timer showing seconds both need a second at
/// most; anything faster would be redrawing a screen nobody is looking at that
/// closely, on a machine that is also driving a projection.
const WIDGET_TICK: std::time::Duration = std::time::Duration::from_millis(1000);

/// A presentation, drawn the way its design says it should be.
///
/// The one place that decides between the audience renderer and a monitor
/// view. That decision was made in three places and only two of them knew
/// about monitor designs: the presentation window did, the design editor's
/// live preview did after it was fixed, and the cards in the settings did not
/// — so a monitor design sat in the list looking like a single slide, which is
/// the one thing it is not.
///
/// Everything that shows a presentation goes through here now, so a fourth
/// place that shows one cannot get it wrong by being written later.
#[component]
pub fn DesignedPresentation(
    running_presentation: Signal<RunningPresentation>,
    /// The design that decides how this is drawn.
    ///
    /// `None` asks the presentation itself — the design the chapter that is up
    /// carries. That is what a card in the settings and a preview of an
    /// element want. A surface that is shown in a design of its *own* — a view
    /// with a design set, the editor previewing the design being edited —
    /// names it here instead.
    #[props(default)]
    design: Option<PresentationDesign>,
    /// What an audience rendering is for. Means nothing for a monitor view,
    /// which is never the window the audience looks at.
    #[props(default)]
    role: crate::components::presentation_components::PresentationRole,
    /// Whether this is drawn inside a box rather than filling a window.
    #[props(default)]
    contained: bool,
) -> Element {
    let design = design
        .unwrap_or_else(|| running_presentation.read().get_current_presentation_design());

    match design.presentation_design_settings.monitor() {
        Some(monitor) => rsx! {
            MonitorViewComponent {
                running_presentation,
                monitor_design: monitor.clone(),
                slide_design: design.clone(),
                contained,
            }
        },
        None => rsx! {
            crate::components::presentation_components::PresentationRendererComponent {
                running_presentation,
                role,
            }
        },
    }
}

/// A whole monitor view: the layout, and the widgets over it.
#[component]
pub fn MonitorViewComponent(
    running_presentation: Signal<RunningPresentation>,
    /// The design being drawn. Its `base` supplies the colours and fonts, so a
    /// monitor view looks like the design it was set up with rather than like
    /// Cantara's defaults.
    monitor_design: MonitorDesign,
    /// The design slides are drawn with, where the layout draws whole slides.
    ///
    /// The monitor's own design, wrapped back into a [`PresentationDesign`] by
    /// the caller — the slide renderer takes one of those, and this is what
    /// makes a slide on the stage monitor come out in the monitor's colours.
    slide_design: PresentationDesign,
    /// Whether this is drawn inside a box rather than filling a window.
    ///
    /// The design editor's live preview draws one in a canvas of exactly the
    /// presentation's size, which is then scaled down. Left alone, the view's
    /// `position: fixed` would take it out of that canvas and lay it over the
    /// whole editor.
    #[props(default)]
    contained: bool,
) -> Element {
    let background = monitor_design.base.get_background_color_as_hex_string();

    rsx! {
        // The slide list is the presenter console's component, so this window
        // needs the console's stylesheet as well as the monitor's own. The
        // monitor stylesheet builds on those classes rather than replacing
        // them — see `assets/monitor_view.css`.
        document::Link {
            rel: "stylesheet",
            href: crate::components::presenter_console_components::PRESENTER_CONSOLE_CSS,
        }
        document::Link { rel: "stylesheet", href: MONITOR_VIEW_CSS }

        div {
            class: if contained { "monitor-view monitor-view-contained" } else { "monitor-view" },
            style: "background-color: {background};",

            div { class: "monitor-view-stage",
                match monitor_design.layout.clone() {
                    MonitorLayout::SlideList { context } => rsx! {
                        // The presenter console's own list, without the
                        // ability to press it. Shared rather than copied —
                        // see [`SlideList`].
                        SlideList {
                            running_presentation,
                            interactive: false,
                            context,
                        }
                    },
                    MonitorLayout::Speaker {
                        next_slide_share,
                        next_position,
                    } => rsx! {
                        SpeakerLayout {
                            running_presentation,
                            slide_design: slide_design.clone(),
                            next_slide_share,
                            next_position,
                        }
                    },
                }
            }

            MonitorWidgets {
                running_presentation,
                widgets: monitor_design.widgets.clone(),
            }
        }
    }
}

/// The current slide large, the next one small beside or below it.
///
/// For whoever is speaking: what they are saying now, and what comes after it.
#[component]
fn SpeakerLayout(
    running_presentation: Signal<RunningPresentation>,
    slide_design: PresentationDesign,
    next_slide_share: f64,
    next_position: SpeakerNextPosition,
) -> Element {
    let current = running_presentation.read().get_current_slide();
    let next = running_presentation.read().peek_next_slide();
    // What the presentation is actually laid out at, so the slides here break
    // their lines where the wall breaks them.
    let slide_size = running_presentation.read().layout_size();

    // Kept inside the range that makes the layout what it is called: a next
    // slide taking nine tenths would leave the speaker reading the wrong one.
    // See [`MonitorLayout::speaker_share`].
    let share = MonitorLayout::speaker_share(next_slide_share) * 100.0;
    let current_share = 100.0 - share;

    // The share is of whichever direction the two are stacked in, so that
    // moving the next slide from below to beside keeps its proportion.
    let (direction, current_size, next_size) = match next_position {
        SpeakerNextPosition::Below => (
            "column",
            format!("height: {current_share}%; width: 100%;"),
            format!("height: {share}%; width: 100%;"),
        ),
        SpeakerNextPosition::Right => (
            "row",
            format!("width: {current_share}%; height: 100%;"),
            format!("width: {share}%; height: 100%;"),
        ),
    };

    rsx! {
        div {
            class: "monitor-speaker",
            style: "flex-direction: {direction};",

            div { class: "monitor-speaker-current", style: "{current_size}",
                if let Some(slide) = current {
                    SlideAtSlideSize { slide, design: slide_design.clone(), slide_size }
                }
            }
            div { class: "monitor-speaker-next", style: "{next_size}",
                // Labelled, because a speaker glancing at two slides has to
                // know at once which of them is the one they are on. Without
                // it the layout is two slides and a guess.
                span { class: "monitor-speaker-next-label",
                    {t!("presentation.monitor_next").to_string()}
                }
                if let Some(slide) = next {
                    SlideAtSlideSize { slide, design: slide_design.clone(), slide_size }
                } else {
                    span { class: "monitor-speaker-next-end",
                        {t!("presentation.monitor_end").to_string()}
                    }
                }
            }
        }
    }
}

/// A slide drawn at the presentation's own size and scaled to fit its box,
/// keeping its proportions.
///
/// # Why it is not simply told to fill the box
///
/// A slide's type is set in **points** — the design says "32pt", and a point
/// is an absolute length. Put the renderer straight into a small box and the
/// words come out at their full size and overflow it; the box gets smaller and
/// the text does not. That is why every other preview in Cantara draws the
/// slide at its native size and shrinks the whole thing with a `transform`,
/// and why this has to as well.
///
/// # Why the scale is CSS and not a number worked out in Rust
///
/// The other previews scale by a factor computed from a width they know — a
/// thumbnail is 400 pixels wide because something said so. This box has no such
/// width: it is a share of a window whose size is the operator's business, and
/// on the network it is a share of a phone nobody here can measure.
///
/// Measuring it in the browser would work in a window and nowhere else: the
/// network stream is served as markup with no Dioxus behind it, so there is no
/// `onmounted` to measure in — the same rule that has caught four other things
/// in this feature. So the scale is worked out by the browser, from the box
/// itself, with container query units: the frame is a size container and the
/// stage inside it is scaled by its width over the presentation's own.
#[component]
fn SlideAtSlideSize(
    slide: Slide,
    design: PresentationDesign,
    /// The size the presentation is laid out at, in CSS pixels.
    ///
    /// Not a guess at 1920×1080: a slide's line breaks depend on the width it
    /// is laid out at, and a preview that broke them somewhere else would not
    /// be a preview of what the room is seeing. This is the number the
    /// presentation window measured — see
    /// [`RunningPresentation::layout_size`].
    slide_size: (f64, f64),
) -> Element {
    let (width, height) = slide_size;

    // A video on this slide follows the one the room is watching.
    //
    // Without this a video slide came out as a `<video>` that nothing ever
    // started: paused at nothing, which on a stage monitor is a blank
    // rectangle where the film should be. The mechanism is the presenter
    // console's — its overview shows the same thing, a picture of a video that
    // moves with the video — and it is asked for here rather than written
    // again. See [`MirrorsTheVideo`] and
    // [`crate::logic::video::mirror_script`].
    //
    // A mirror takes no commands, makes no sound and reports nothing: the
    // projection stays the one that plays. That is the same rule as everywhere
    // else here — a monitor view shows, it does not control.
    use_context_provider(|| crate::components::presentation_components::MirrorsTheVideo);

    rsx! {
        div {
            class: "monitor-slide-frame",
            style: "--slide-width: {width}px; --slide-height: {height}px;",
            // Measured once the frame is on the page. A slide frame is created
            // when the presentation reaches this slide, so this is the moment
            // its size is knowable — and the script asks again on the next
            // frame in case the layout has not settled.
            onmounted: move |_| {
                spawn(async move {
                    if let Err(error) = document::eval(SLIDE_SCALE_JS).await {
                        log::error!("could not fit the slide to its box: {error:?}");
                    }
                });
            },
            div { class: "monitor-slide-stage",
                StaticSlideRendererComponent { slide, presentation_design: design }
            }
        }
    }
}

/// The widgets of a monitor view, in their corners.
#[component]
fn MonitorWidgets(
    running_presentation: Signal<RunningPresentation>,
    widgets: Vec<MonitorWidget>,
) -> Element {
    // What makes a clock a clock. Every widget reads this, so one timer
    // redraws all of them rather than each keeping its own — and a view with
    // no widgets starts no timer at all.
    let mut now = use_signal(Timestamp::now);
    // Read while rendering, so that a design gaining its first widget brings
    // the loop below round rather than leaving it asleep.
    let mut widget_count = use_signal(|| widgets.len());
    if *widget_count.peek() != widgets.len() {
        widget_count.set(widgets.len());
    }

    use_future(move || async move {
        loop {
            // Nothing to redraw: wait and look again rather than returning.
            // Returning is what left a clock frozen for good when the widget
            // was added to a design after the view was already open — the
            // future had ended and there was nothing to start it again.
            if widget_count() == 0 {
                crate::logic::timer::sleep(WIDGET_TICK).await;
                continue;
            }
            crate::logic::timer::sleep(WIDGET_TICK).await;
            now.set(Timestamp::now());
        }
    });

    rsx! {
        for (position , widget) in widgets.iter().cloned().enumerate() {
            div {
                key: "{position}",
                class: "monitor-widget monitor-widget-{corner_class(widget.placement)}",
                match widget.kind {
                    WidgetKind::Clock { with_date } => rsx! {
                        ClockWidget { now: now(), with_date }
                    },
                    WidgetKind::ChapterTimer { warn_after_seconds } => rsx! {
                        ChapterTimerWidget {
                            running_presentation,
                            now: now(),
                            warn_after_seconds,
                        }
                    },
                }
            }
        }
    }
}

/// The time, and optionally the date.
#[component]
fn ClockWidget(now: Timestamp, with_date: bool) -> Element {
    // Formatted by the platform rather than by a format string of Cantara's:
    // a German installation should get a German date without anybody
    // configuring one, and neither Rust's standard library nor this program
    // carries a calendar. See [`crate::logic::localisation::format_clock`].
    let (date, time) = crate::logic::localisation::format_clock(now);

    rsx! {
        div { class: "monitor-clock",
            span { class: "monitor-clock-time", {time} }
            if with_date {
                span { class: "monitor-clock-date", {date} }
            }
        }
    }
}

/// How long the service has been in the chapter it is in.
#[component]
fn ChapterTimerWidget(
    running_presentation: Signal<RunningPresentation>,
    now: Timestamp,
    warn_after_seconds: Option<u32>,
) -> Element {
    let entered = running_presentation.read().chapter_entered_at;

    let Some(entered) = entered else {
        // Nothing is up, so there is nothing to time — a timer counting from
        // the moment an empty running order was opened would be counting the
        // operator's preparation.
        return rsx! {
            div { class: "monitor-timer", span { class: "monitor-timer-value", "–:––" } }
        };
    };

    let elapsed = entered.elapsed_at(now);
    let over = warn_after_seconds
        .is_some_and(|warn| elapsed.as_secs() >= warn as u64);

    rsx! {
        div { class: if over { "monitor-timer over" } else { "monitor-timer" },
            span { class: "monitor-timer-value", {clock_face(elapsed.as_secs())} }
        }
    }
}

/// Seconds as a person reads a stopwatch.
///
/// Minutes and seconds until an hour has passed, and hours after that: a sermon
/// reading `93:41` is harder to take in at a glance than one reading `1:33:41`,
/// and a song never gets that far.
fn clock_face(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    match hours {
        0 => format!("{minutes}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

/// The class that puts a widget in its corner.
fn corner_class(placement: WidgetPlacement) -> &'static str {
    match placement {
        WidgetPlacement::TopLeft => "top-left",
        WidgetPlacement::TopRight => "top-right",
        WidgetPlacement::BottomLeft => "bottom-left",
        WidgetPlacement::BottomRight => "bottom-right",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a stopwatch looks like. The minute-and-second form is the one a
    /// song and a sermon both use; the hour only appears when it means
    /// something.
    #[test]
    fn a_timer_reads_the_way_a_stopwatch_does() {
        assert_eq!(clock_face(0), "0:00");
        assert_eq!(clock_face(9), "0:09");
        assert_eq!(clock_face(69), "1:09");
        assert_eq!(clock_face(1199), "19:59");
        assert_eq!(clock_face(3600), "1:00:00");
        assert_eq!(clock_face(5621), "1:33:41");
    }
}
