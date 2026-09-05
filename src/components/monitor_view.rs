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
    MonitorDesign, MonitorLayout, MonitorWidget, PresentationDesign, WidgetKind, WidgetPlacement,
};
use crate::logic::states::RunningPresentation;
use crate::logic::timer::Timestamp;

rust_i18n::i18n!("locales", fallback = "en");

/// What a monitor view looks like.
const MONITOR_VIEW_CSS: Asset = asset!("/assets/monitor_view.css");

/// How often the widgets are redrawn.
///
/// A clock showing minutes and a timer showing seconds both need a second at
/// most; anything faster would be redrawing a screen nobody is looking at that
/// closely, on a machine that is also driving a projection.
const WIDGET_TICK: std::time::Duration = std::time::Duration::from_millis(1000);

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
                    MonitorLayout::Speaker { next_slide_share } => rsx! {
                        SpeakerLayout {
                            running_presentation,
                            slide_design: slide_design.clone(),
                            next_slide_share,
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

/// The current slide large, the next one small underneath it.
///
/// For whoever is speaking: what they are saying now, and what comes after it.
#[component]
fn SpeakerLayout(
    running_presentation: Signal<RunningPresentation>,
    slide_design: PresentationDesign,
    next_slide_share: f64,
) -> Element {
    let current = running_presentation.read().get_current_slide();
    let next = running_presentation.read().peek_next_slide();

    // Kept inside the range that makes the layout what it is called: a next
    // slide taking nine tenths of the height would leave the speaker reading
    // the wrong one. See [`MonitorLayout::speaker_share`].
    let share = MonitorLayout::speaker_share(next_slide_share) * 100.0;
    let current_share = 100.0 - share;

    rsx! {
        div { class: "monitor-speaker",
            div {
                class: "monitor-speaker-current",
                style: "height: {current_share}%;",
                if let Some(slide) = current {
                    StaticSlideRendererComponent {
                        slide,
                        presentation_design: slide_design.clone(),
                    }
                }
            }
            div {
                class: "monitor-speaker-next",
                style: "height: {share}%;",
                // Labelled, because a speaker glancing at two slides has to
                // know at once which of them is the one they are on. Without
                // it the layout is two slides and a guess.
                span { class: "monitor-speaker-next-label",
                    {t!("presentation.monitor_next").to_string()}
                }
                if let Some(slide) = next {
                    StaticSlideRendererComponent {
                        slide,
                        presentation_design: slide_design.clone(),
                    }
                } else {
                    span { class: "monitor-speaker-next-end",
                        {t!("presentation.monitor_end").to_string()}
                    }
                }
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
    let has_widgets = !widgets.is_empty();

    use_future(move || async move {
        if !has_widgets {
            return;
        }
        loop {
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
