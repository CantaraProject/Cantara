//! Checking the projection window, which no browser can reach.
//!
//! Stage 5 of `docs/specs/0004-testing-playwright.md`.
//!
//! # The gap this fills
//!
//! Playwright drives browsers. Cantara's projection is a `wry` window —
//! WebKitGTK on Linux, WebView2 on Windows, WKWebView on macOS — and Playwright
//! cannot attach to any of them. That window is where a service actually
//! happens, so leaving it to "somebody will notice" is leaving the most
//! important surface uncovered while the suite looks complete.
//!
//! This opens the real window, puts a real service in it, and measures what
//! came out. Not a mock of the window: the same `DesignedPresentation` the
//! projection draws, in a real web view, laid out by a real engine.
//!
//! # Why geometry and not screenshots
//!
//! The obvious way to check a window is to photograph it and compare. It is
//! also the way that goes red every time a font is updated, a driver changes
//! how it antialiases, or the picture is taken on a machine with a different
//! DPI — and a suite that goes red for those reasons is a suite whose failures
//! stop being read.
//!
//! Geometry is stable across all of that and still catches the failures that
//! matter here. Look at what actually went wrong while the monitor views were
//! being built:
//!
//! * a black strip and a white screen where the view should have been;
//! * a slide scaled to nothing, so the screen was empty;
//! * a PDF page at its own size in the middle of the design;
//! * a slide overflowing the box it was scaled into.
//!
//! Every one of those is a number. None of them needs a photograph, and a
//! photograph would have reported all four as "the picture changed", which is
//! what it reports when a heading moves two pixels as well.
//!
//! **What geometry cannot say** is whether anything was *painted*. An element
//! of the right size, in the right place, in black on black, measures
//! perfectly. That limitation is real and is why this does not replace a
//! person looking at the screen before a release — see the spec.

use dioxus::prelude::*;

use crate::logic::fixtures;
use crate::logic::settings::PresentationDesign;

/// The window the measurements are taken in.
///
/// Fixed, because every number below is relative to it and a window whose size
/// the desktop chose would make the results depend on the machine.
const WINDOW: (f64, f64) = (1280.0, 720.0);

/// How much of the window a slide has to occupy to count as being on it.
///
/// Not "all of it": a design has padding, and a slide of one short line does
/// not fill a wide screen vertically. A tenth is far below anything a real
/// design produces and far above the failures — a slide scaled to nothing, or
/// a stage a black strip high.
const ENOUGH_OF_THE_WINDOW: f64 = 0.1;

/// What one measurement of one design came to.
#[derive(serde::Deserialize, Debug)]
struct Measured {
    /// The stage the slide is drawn on: `.presentation`, or the monitor view.
    stage: Box2,
    /// The slide's own frame, where a monitor view scales one.
    frame: Option<Box2>,
    /// The scaled slide inside that frame.
    scaled: Option<Box2>,
    /// The largest piece of text on the stage, if there is any.
    text: Option<Box2>,
    /// The words in it, so that "there is a heading" and "the heading says
    /// something" are not the same observation.
    words: String,
}

#[derive(serde::Deserialize, Debug, Clone, Copy)]
struct Box2 {
    width: f64,
    height: f64,
}

impl Box2 {
    fn drawn(&self) -> bool {
        self.width > 1.0 && self.height > 1.0
    }
}

/// Measures the page, once it has settled.
///
/// Two animation frames before reading anything: the slide's size is set by a
/// script that measures its box, and reading in the same frame it was written
/// gives the size before the fitting rather than after it. This is the same
/// wait `stream_viewer.html` makes before engraving a staff, and for the same
/// reason.
const MEASURE_JS: &str = r#"
return await new Promise(function (resolve) {
  requestAnimationFrame(function () {
    requestAnimationFrame(function () {
      setTimeout(function () {
        function box(element) {
          if (!element) return null;
          var r = element.getBoundingClientRect();
          return { width: r.width, height: r.height };
        }

        var stage = document.querySelector('.monitor-view') ||
                    document.querySelector('.presentation');
        var frame = document.querySelector('.monitor-slide-frame');
        var scaled = document.querySelector('.monitor-slide-stage');

        // The largest run of text on the stage, which is what a person in the
        // back row is reading.
        var biggest = null;
        var widest = 0;
        var candidates = (stage || document).querySelectorAll(
          '.headline, .slide-text-content, .complex-slide-row, .presenter-slide-item, p'
        );
        candidates.forEach(function (element) {
          var r = element.getBoundingClientRect();
          if (r.width * r.height > widest && element.textContent.trim().length) {
            widest = r.width * r.height;
            biggest = element;
          }
        });

        resolve(JSON.stringify({
          stage: box(stage) || { width: 0, height: 0 },
          frame: box(frame),
          scaled: box(scaled),
          text: box(biggest),
          words: biggest ? biggest.textContent.trim().slice(0, 80) : ''
        }));
      }, 400);
    });
  });
});
"#;

/// Says something, now.
///
/// `println!` alone was not enough here and the reason cost half an hour:
/// stdout is **block-buffered when it is not a terminal**, so everything this
/// mode reported sat in a buffer until the process ended — and when a run was
/// cut short, the output was simply gone. A mode that appeared to hang and a
/// mode that was working normally looked identical from the outside.
///
/// A diagnostic that is only delivered on a clean exit is no diagnostic at all:
/// the runs worth reading are exactly the ones that do not get there.
fn say(line: &str) {
    use std::io::Write;
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// One thing that has to be true of the window.
struct Check {
    /// What is being drawn.
    what: &'static str,
    /// The service, and the design it is shown in.
    service: fn() -> crate::logic::states::RunningPresentation,
    design: fn() -> Option<PresentationDesign>,
}

/// Everything measured, in order.
fn checks() -> Vec<Check> {
    vec![
        Check {
            what: "a song on the projection",
            service: fixtures::song_service_on_the_words,
            design: || None,
        },
        Check {
            what: "a picture on the projection",
            service: fixtures::picture_service,
            design: || None,
        },
        Check {
            what: "a song on a stage monitor showing the list",
            service: fixtures::song_service,
            design: || Some(fixtures::slide_list_design()),
        },
        Check {
            what: "a song on a speaker's monitor",
            service: fixtures::song_service,
            design: || {
                Some(fixtures::speaker_design(
                    crate::logic::settings::SpeakerNextPosition::Right,
                ))
            },
        },
        Check {
            what: "a pdf page on a speaker's monitor",
            service: fixtures::pdf_service,
            design: || {
                Some(fixtures::speaker_design(
                    crate::logic::settings::SpeakerNextPosition::Below,
                ))
            },
        },
    ]
}

/// What a measurement has to satisfy, and why.
///
/// Returns the complaints. Empty is a pass.
fn faults(check: &Check, measured: &Measured) -> Vec<String> {
    let mut wrong = Vec::new();
    let (window_width, window_height) = WINDOW;

    if !measured.stage.drawn() {
        wrong.push(format!(
            "the stage is {}×{}, so there is nothing on the screen at all",
            measured.stage.width, measured.stage.height
        ));
        // Nothing below can mean anything without a stage.
        return wrong;
    }

    // A black strip and a white screen: the reported shape of the very first
    // monitor view. The stage was there and occupied almost none of the window.
    if measured.stage.width < window_width * ENOUGH_OF_THE_WINDOW
        || measured.stage.height < window_height * ENOUGH_OF_THE_WINDOW
    {
        wrong.push(format!(
            "the stage is {}×{} in a {window_width}×{window_height} window, which \
             is a strip rather than a projection",
            measured.stage.width, measured.stage.height
        ));
    }

    match (measured.frame, measured.scaled) {
        (Some(frame), Some(scaled)) => {
            // "Now there is no content on the monitor slide at all." A default
            // of `scale(0)` hid everything when the measuring did not run, and
            // a stage of nothing passes every check that only asks whether an
            // element is present.
            if !scaled.drawn() {
                wrong.push(format!(
                    "the slide was scaled to {}×{}, so the monitor is blank",
                    scaled.width, scaled.height
                ));
            }
            // The other direction: a slide laid out at 1920 wide, dropped into
            // a smaller box without being scaled, overflows it. A point is an
            // absolute length, so this does not fix itself.
            if scaled.width > frame.width + 1.0 || scaled.height > frame.height + 1.0 {
                wrong.push(format!(
                    "the slide is {}×{} in a frame of {}×{}, so it overflows it",
                    scaled.width, scaled.height, frame.width, frame.height
                ));
            }
        }
        (Some(_), None) => wrong.push("there is a slide frame with no slide in it".to_string()),
        _ => {}
    }

    // A design that shows words has to have words on it, of a readable size.
    // Nothing here can say they are legible — see the module documentation on
    // what geometry cannot see — but "there is text and it has area" rules out
    // the failure where the stage is drawn and empty.
    if check.what.starts_with("a song")
        && let Some(text) = measured.text
    {
        if !text.drawn() {
            wrong.push("the words have no size, so the slide is blank".to_string());
        }
        if measured.words.is_empty() {
            wrong.push("the largest thing on the slide has no words in it".to_string());
        }
    } else if check.what.starts_with("a song") {
        wrong.push("there is no text on a slide that is all text".to_string());
    }

    wrong
}

/// Opens the window, measures every check, and says whether the projection is
/// right.
///
/// Exits the process: the Dioxus desktop launcher owns the main loop and does
/// not hand it back.
pub fn run() -> ! {
    say(&format!("measuring the projection window in {}×{}", WINDOW.0, WINDOW.1));
    // The same preparation `launch_app` does. Without it this window came up
    // empty on Linux and measured nothing — a failure that looked exactly like
    // the defect it exists to find, which is the worst way for scaffolding to
    // be wrong.
    crate::logic::window_platform::prepare();

    let window = dioxus::desktop::tao::window::WindowBuilder::new()
        .with_title("Cantara — measuring")
        .with_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(
            WINDOW.0, WINDOW.1,
        ))
        .with_resizable(false);

    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(window)
                .with_menu(None),
        )
        .launch(Measuring);

    // `launch` returns when the window closes, which happens after the report
    // below has already decided the exit code.
    std::process::exit(0)
}

/// Draws one check at a time, measuring each before moving on.
#[component]
fn Measuring() -> Element {
    let mut at = use_signal(|| 0usize);
    let mut faulty: Signal<Vec<String>> = use_signal(Vec::new);

    let all = checks();
    let total = all.len();
    let index = at();
    let current = all.get(index);

    let running = current.map(|check| (check.service)());
    let design = current.and_then(|check| (check.design)());

    use_effect(move || {
        say(&format!("  … measuring check {}", at() + 1));
        // Reading the position is what makes this run again for the next
        // check. Without it the effect fires once and the window sits on the
        // first slide for ever.
        let index = at();
        let all = checks();

        spawn(async move {
            let Some(check) = all.get(index) else {
                return;
            };

            let measured = match document::eval(MEASURE_JS).await {
                Ok(value) => value
                    .as_str()
                    .and_then(|json| serde_json::from_str::<Measured>(json).ok()),
                Err(error) => {
                    say(&format!("  {}: could not be measured: {error:?}", check.what));
                    None
                }
            };

            match measured {
                Some(measured) => {
                    let wrong = faults(check, &measured);
                    if wrong.is_empty() {
                        say(&format!(
                            "  ok    {} — stage {}×{}",
                            check.what, measured.stage.width, measured.stage.height
                        ));
                    } else {
                        say(&format!("  FAIL  {}", check.what));
                        for complaint in &wrong {
                            say(&format!("          {complaint}"));
                            faulty.write().push(format!("{}: {complaint}", check.what));
                        }
                    }
                }
                None => {
                    let complaint = format!("{}: nothing could be measured", check.what);
                    say(&format!("  FAIL  {complaint}"));
                    faulty.write().push(complaint);
                }
            }

            if index + 1 < total {
                at.set(index + 1);
                return;
            }

            let wrong = faulty.read().clone();
            if wrong.is_empty() {
                say("\nthe projection window draws every design correctly");
                std::process::exit(0);
            }
            say(&format!("\n{} thing(s) wrong with the projection window", wrong.len()));
            std::process::exit(1);
        });
    });

    rsx! {
        if let Some(running) = running {
            MeasuredPresentation { running, design }
        }
    }
}

/// The presentation, exactly as the projection draws it.
///
/// Through [`DesignedPresentation`](crate::components::monitor_view::DesignedPresentation)
/// — the one place that decides between the audience renderer and a monitor
/// view — because a measuring mode that drew the slide its own way would be
/// measuring itself.
#[component]
fn MeasuredPresentation(
    running: crate::logic::states::RunningPresentation,
    design: Option<PresentationDesign>,
) -> Element {
    let running_presentation = use_signal(|| running);

    rsx! {
        // The same stylesheet and the same wrapper the projection window has.
        // Both matter to the numbers: `main.css` is what gives the page a
        // height at all, and the wrapper is what passes it on. Measuring
        // without them measures a page that does not exist.
        crate::components::monitor_view::DesignedPresentation {
            running_presentation,
            design,
            // The audience's role, because that is the window being checked:
            // it is what publishes the layout size that every slide beside it
            // is fitted to, and measuring anything else would measure a
            // follower of a projection that does not exist.
            role: crate::components::presentation_components::PresentationRole::Audience,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_song() -> Check {
        Check {
            what: "a song on the projection",
            service: fixtures::song_service_on_the_words,
            design: || None,
        }
    }

    fn measured(stage: (f64, f64)) -> Measured {
        Measured {
            stage: Box2 {
                width: stage.0,
                height: stage.1,
            },
            frame: None,
            scaled: None,
            text: Some(Box2 {
                width: 600.0,
                height: 80.0,
            }),
            words: "Amazing grace how sweet the sound".to_string(),
        }
    }

    /// An ordinary projection passes.
    ///
    /// First, because every rule below is a way of failing, and a set of rules
    /// that rejects everything satisfies all of them.
    #[test]
    fn a_window_that_is_right_has_nothing_wrong_with_it() {
        assert_eq!(faults(&a_song(), &measured(WINDOW)), Vec::<String>::new());
    }

    /// A stage of nothing is the failure where the screen was empty.
    #[test]
    fn a_stage_of_no_size_is_reported() {
        let wrong = faults(&a_song(), &measured((0.0, 0.0)));

        assert_eq!(wrong.len(), 1, "one complaint, not a cascade: {wrong:?}");
        assert!(wrong[0].contains("nothing on the screen"));
    }

    /// A black strip is the failure where the monitor view was a sliver at the
    /// top of a white screen.
    #[test]
    fn a_stage_that_is_a_strip_is_reported() {
        let wrong = faults(&a_song(), &measured((1280.0, 20.0)));

        assert!(
            wrong.iter().any(|said| said.contains("strip")),
            "got {wrong:?}"
        );
    }

    /// A slide scaled to nothing is the failure where the monitor went blank
    /// after the scaling was made to work.
    #[test]
    fn a_slide_scaled_to_nothing_is_reported() {
        let mut reading = measured(WINDOW);
        reading.frame = Some(Box2 {
            width: 900.0,
            height: 600.0,
        });
        reading.scaled = Some(Box2 {
            width: 0.0,
            height: 0.0,
        });

        let wrong = faults(&a_song(), &reading);

        assert!(
            wrong.iter().any(|said| said.contains("blank")),
            "got {wrong:?}"
        );
    }

    /// A slide larger than the box it was put in is the failure a point being
    /// an absolute length causes.
    #[test]
    fn a_slide_that_overflows_its_frame_is_reported() {
        let mut reading = measured(WINDOW);
        reading.frame = Some(Box2 {
            width: 900.0,
            height: 600.0,
        });
        reading.scaled = Some(Box2 {
            width: 1920.0,
            height: 1080.0,
        });

        let wrong = faults(&a_song(), &reading);

        assert!(
            wrong.iter().any(|said| said.contains("overflows")),
            "got {wrong:?}"
        );
    }

    /// A slide of words with no words on it.
    #[test]
    fn a_text_slide_with_no_text_is_reported() {
        let mut reading = measured(WINDOW);
        reading.text = None;

        let wrong = faults(&a_song(), &reading);

        assert!(
            wrong.iter().any(|said| said.contains("no text")),
            "got {wrong:?}"
        );
    }

    /// A slide fitted exactly to its frame is fitted, not overflowing.
    ///
    /// The boundary the tolerance exists for: a scale computed in the browser
    /// lands a fraction of a pixel either side of exact, and a rule without
    /// slack would fail on a correct window depending on the rounding.
    #[test]
    fn a_slide_fitted_exactly_to_its_frame_is_allowed() {
        let mut reading = measured(WINDOW);
        reading.frame = Some(Box2 {
            width: 900.0,
            height: 600.0,
        });
        reading.scaled = Some(Box2 {
            width: 900.4,
            height: 600.2,
        });

        assert_eq!(faults(&a_song(), &reading), Vec::<String>::new());
    }

    /// Every check names a service that can be built.
    ///
    /// The same guard the harness has: a check naming a fixture that no longer
    /// exists would fail in a window, minutes later, as "nothing could be
    /// measured".
    #[test]
    fn every_check_names_a_service_that_exists() {
        for check in checks() {
            let running = (check.service)();
            assert!(
                running.get_current_slide().is_some(),
                "{} has no slide to measure",
                check.what
            );
        }
    }
}
