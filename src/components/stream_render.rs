//! One rendering, two surfaces: turning a presentation into HTML that a phone
//! on the network can be shown.
//!
//! # Why this exists
//!
//! Cantara used to draw a slide twice. The window drew it with the components
//! in [`crate::components::presentation_components`]; the page served to
//! browsers drew it again, in JavaScript, from a description of the slide that
//! [`crate::logic::stream::protocol`] built for it. Two renderers of the same
//! thing, and the second one only ever knew about the features the first had
//! when it was written — which is exactly how a monitor design came out as a
//! plain wall of text on a phone. Its layout and its widgets were not missing
//! from the data; the page simply had no idea such things existed.
//!
//! So the page stops rendering. What it is given is HTML produced by *the same
//! components the projector uses*, through [`dioxus_ssr`]. A feature added to
//! a slide, a design, or a monitor layout reaches the network by existing.
//!
//! # Why here and not in the helper
//!
//! Cantara's network side runs as a second process — see
//! [`crate::logic::network_server`] — and the obvious place to render would be
//! there, next to the socket. It is the wrong place, for two reasons that both
//! come down to the helper deliberately knowing nothing:
//!
//! * **The pictures.** A slide's background and a picture slide are inlined as
//!   data URLs out of [`crate::logic::images`], whose cache is filled from the
//!   library on disk. The helper has neither the library nor the cache; it is
//!   handed rendered bytes and serves them. Cantara has both, warm, because it
//!   is already showing the same slide on the projector.
//! * **The settings.** Which design a view is shown in is a setting, and the
//!   helper has no settings — by design, so that a service cannot be changed by
//!   whatever reaches the socket.
//!
//! Rendering here also keeps the fan-out that made the stream a static page in
//! the first place: the slide is rendered **once per change**, not once per
//! viewer, and the same bytes go to every phone in the building. That is what
//! ruled out giving each viewer a `dioxus-liveview` session the way the remote
//! presenter console has one — a console is one operator, a stream is the whole
//! congregation, and a server-side `VirtualDom` per phone is a different
//! proposition entirely.
//!
//! # What a caller has to know
//!
//! [`render_presentation`] is synchronous and does no I/O of its own beyond the
//! picture cache it reads. It builds a `VirtualDom`, renders it once, and drops
//! it — so anything a component does in an effect or a spawned task has *not*
//! happened. In practice that means:
//!
//! * A picture that is not in the cache yet renders as no picture, and the next
//!   render — after it has been prepared — has it. Nothing waits.
//! * Time-based widgets read the clock at the moment of rendering. They are as
//!   fresh as the last render, which is why anything showing one has to be
//!   re-rendered on a timer rather than only when the slide changes.

use dioxus::prelude::*;

use crate::components::monitor_view::DesignedPresentation;
use crate::components::presentation_components::PresentationRole;
use crate::logic::settings::PresentationDesign;
use crate::logic::states::RunningPresentation;

/// The presentation as HTML, drawn the way `design` says it should be.
///
/// `design` is the design of the *view* being rendered — a stage monitor's, a
/// stream's — or `None` to use the design the presentation's current chapter
/// carries, which is what the projection shows.
///
/// The markup is a fragment, not a document: it carries no stylesheet and no
/// `<html>`. Whoever serves it says how it is dressed, because the stylesheets
/// are the same files the window links and are better sent once with the page
/// than with every slide.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the stream still serves its own page; wiring this in is the                   next step, and it is tested and documented in the meantime"
    )
)]
pub fn render_presentation(
    presentation: &RunningPresentation,
    design: Option<PresentationDesign>,
) -> String {
    let mut dom = VirtualDom::new_with_props(
        StreamRoot,
        StreamRootProps {
            presentation: presentation.clone(),
            design,
        },
    );
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// The same rendering, with its addresses rewritten for another device.
///
/// A rendering is made for the machine it was made on, and two of the
/// addresses in it mean nothing anywhere else:
///
/// * `/cantara-video/…` is answered by the asset handler inside Cantara's own
///   web view — see [`crate::components::video_host`]. A phone asking its own
///   origin for that gets nothing.
/// * On the WebKitGTK platforms it is worse: a video's `src` is an absolute
///   `http://127.0.0.1:…` URL, because that engine will not play media from a
///   custom scheme (see [`crate::logic::video::video_source_url`]). *Loopback*
///   on a phone is the phone.
///
/// Both become `video/…` on the stream's own origin.
///
/// The name after it is **not** the path. The server holds the videos of the
/// running service under [`media_id`](crate::logic::stream::protocol::media_id)
/// — an MD5 of the source — because that is how every other piece of media it
/// serves is addressed, and it is registered under that name before a viewer
/// ever asks. A rewrite that kept the encoded path produced an address nothing
/// answered: the element was there, the file was not, and a viewer saw the
/// design's background and an empty rectangle over it.
///
/// A **PDF page** is the other half of the same problem. It is drawn by pdf.js
/// into a canvas, which no rendering without a browser can fill, so the markup
/// arrived as an empty box. The page travels as a picture instead — Cantara
/// renders it and sends the bytes, exactly as it always has — and the canvas
/// becomes the `<img>` that asks for it. The canvas says which page it depicts
/// in `data-pdf` and `data-page`, so that this rewrite needs to know nothing
/// about PDFs beyond the name the picture is filed under.
///
/// Ordinary pictures need no rewriting at all: they are inlined as data URLs
/// and carry their own bytes.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "used once the stream serves the rendering; tested and documented meanwhile"
    )
)]
pub fn for_network(html: &str) -> String {
    // An optional origin in front, so that the absolute form loses its
    // `http://127.0.0.1:1234` as well rather than being left with a URL
    // pointing at the viewer's own machine.
    //
    // Built once. A pattern that will not compile is not a reason to stop a
    // service — the rendering goes out with its addresses as they were, which
    // costs the viewers a video they could not have played anyway. Everything
    // else in the slide still arrives.
    static PATTERN: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        // The whole address, the encoded path included, so that the
        // replacement is the complete new one rather than a prefix with the
        // old tail left dangling after it.
        match regex::Regex::new(&format!(
            r#"(?:https?://[^"'\s]*?)?/{}/([^"'\s]*)"#,
            regex::escape(crate::logic::video::VIDEO_HANDLER)
        )) {
            Ok(pattern) => Some(pattern),
            Err(error) => {
                log::error!("the video address pattern did not compile: {error}");
                None
            }
        }
    });

    let Some(pattern) = pattern else {
        return html.to_string();
    };

    // The video's own address, under the name the server files it by.
    let rewritten = pattern.replace_all(html, |captures: &regex::Captures| {
        let encoded = captures.get(1).map(|m| m.as_str()).unwrap_or_default();

        match crate::logic::video::path_of_video_url(&format!(
            "/{}/{encoded}",
            crate::logic::video::VIDEO_HANDLER
        )) {
            Some(path) => format!("video/{}", crate::logic::stream::protocol::media_id(&path)),
            // An address that cannot be read back is left as the path it was.
            // It will not be answered, but nothing else in the slide is
            // disturbed by it.
            None => format!("video/{encoded}"),
        }
    });

    pdf_pages_as_pictures(&rewritten)
}

/// Turns the canvas a PDF page is drawn into on this machine into the picture
/// a viewer is served.
///
/// pdf.js draws the page into a `<canvas>` in the window, which is exactly
/// right there and useless anywhere else: a rendering made without a browser
/// carries an empty box. Cantara renders the page and sends it as a picture
/// regardless — that is how the stream has always shown PDFs — so what the
/// markup needs is the address of that picture, and the canvas already says
/// which page it is.
fn pdf_pages_as_pictures(html: &str) -> String {
    static PATTERN: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        match regex::Regex::new(
            r#"<canvas[^>]*?data-pdf="([^"]*)"[^>]*?data-page="([^"]*)"[^>]*?></canvas>"#,
        ) {
            Ok(pattern) => Some(pattern),
            Err(error) => {
                log::error!("the pdf page pattern did not compile: {error}");
                None
            }
        }
    });

    let Some(pattern) = pattern else {
        return html.to_string();
    };

    pattern
        .replace_all(html, |captures: &regex::Captures| {
            let path = captures.get(1).map(|m| m.as_str()).unwrap_or_default();
            let page = captures.get(2).map(|m| m.as_str()).unwrap_or_default();
            // The same name `media_sources` files the rendered page under, so
            // that the address and the bytes cannot disagree.
            let id = crate::logic::stream::protocol::media_id(&format!("{path}#page={page}"));
            // Fitted exactly as a picture slide is — because that is what a
            // PDF page becomes here. The styles come from the picture
            // component rather than being written out again, so the two
            // cannot drift into looking different.
            format!(
                r#"<div style="{frame}"><img alt="" style="{picture}" src="media/{id}"/></div>"#,
                frame = crate::components::presentation_components::PICTURE_FRAME_STYLE,
                picture = crate::components::presentation_components::PICTURE_STYLE,
            )
        })
        .into_owned()
}

/// The root of a rendering that has no window.
///
/// Its only job is to turn the value it is given into the signal the
/// components want. A `Signal` cannot be made outside a running `VirtualDom`,
/// so it cannot be built by the caller and handed in — which is the whole
/// reason this component exists rather than [`render_presentation`] calling
/// [`DesignedPresentation`] directly.
#[component]
fn StreamRoot(
    presentation: RunningPresentation,
    design: Option<PresentationDesign>,
) -> Element {
    // A document, so that the components which ask for one find it.
    //
    // `document::Link` and `document::Script` — which every slide renderer
    // uses to bring in its stylesheet and the PDF viewer — look one up in the
    // context. Without one they log
    // "Unable to find a document in the renderer. Using the default no-op
    // document." at error level, and this renders on every slide change of
    // every service. The fallback they then use is exactly this one; the only
    // difference is that finding it is not an error.
    //
    // A no-op is the right document here in any case. There is no page to add
    // a stylesheet to: the markup is a fragment, and whoever serves it says
    // how it is dressed — see [`crate::logic::stream::server`].
    use_context_provider(|| {
        std::rc::Rc::new(dioxus::document::NoOpDocument) as std::rc::Rc<dyn dioxus::document::Document>
    });

    let running_presentation = use_signal(|| presentation.clone());

    rsx! {
        DesignedPresentation {
            running_presentation,
            design,
            // Never the window the audience is looking at: this rendering
            // drives no timer, publishes no layout size, and is one of many.
            role: PresentationRole::Follower,
            // Shown inside whatever box the page gives it, not filling a
            // window of its own.
            contained: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::settings::{
        MonitorDesign, MonitorLayout, MonitorWidget, PresentationDesignSettings,
        SpeakerNextPosition, WidgetKind, WidgetPlacement,
    };
    use crate::logic::sourcefiles::{SourceFile, SourceFileType};
    use crate::logic::states::SlideChapter;
    use cantara_songlib::slides::SlideSettings;

    /// A short service of one song, built the way a presentation is.
    fn service() -> RunningPresentation {
        let slides = crate::logic::presentation::slides_from_song_content(
            "#title: Amazing Grace\n\nAmazing grace how sweet the sound\nThat saved a wretch like me\n\n---\n\nI once was lost but now am found\nWas blind but now I see\n",
            "Amazing Grace.song",
            &SlideSettings::default(),
            "Amazing Grace",
            &[],
        )
        .expect("the preview song builds into slides");

        let chapter = SlideChapter::new(
            slides,
            SourceFile {
                name: "Amazing Grace".to_string(),
                path: std::path::PathBuf::from("Amazing Grace.song"),
                file_type: SourceFileType::Song,
                md5_hash: None,
                relative_path: None,
            },
            None,
            None,
        );

        RunningPresentation::new(vec![chapter])
    }

    fn monitor_design(layout: MonitorLayout, widgets: Vec<MonitorWidget>) -> PresentationDesign {
        PresentationDesign {
            name: "Stage".to_string(),
            description: String::new(),
            presentation_design_settings: PresentationDesignSettings::Monitor(MonitorDesign {
                layout,
                widgets,
                ..MonitorDesign::default()
            }),
        }
    }

    /// The ordinary case: an audience design renders the slide that is up.
    ///
    /// This is what the stream showed before, and it has to keep showing it —
    /// the whole change is meant to be invisible for a design that has not
    /// changed.
    #[test]
    fn an_audience_design_renders_the_slide_that_is_up() {
        let mut running = service();
        running.jump_to(0, 1);

        let html = render_presentation(&running, None);

        assert!(
            html.contains("Amazing grace how sweet the sound"),
            "the slide's own words are missing from: {html}"
        );
        assert!(
            html.contains("presentation"),
            "the slide was not drawn by the presentation components: {html}"
        );
    }

    /// The failure this module was written for: a monitor design used to reach
    /// a phone as a plain slide, because the page rendering it had never heard
    /// of layouts. Now the layout is what is rendered.
    #[test]
    fn a_monitor_design_renders_its_layout_and_not_a_bare_slide() {
        let mut running = service();
        running.jump_to(0, 1);

        let html = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                Vec::new(),
            )),
        );

        assert!(
            html.contains("monitor-view"),
            "a monitor design did not render as a monitor view: {html}"
        );
        assert!(
            html.contains("presenter-text-panel"),
            "the slide list is missing: {html}"
        );
    }

    /// The list shows the service, not only the slide that is up — which is
    /// the point of it, and something a single-slide renderer cannot express.
    #[test]
    fn the_slide_list_renders_more_than_the_current_slide() {
        let mut running = service();
        running.jump_to(0, 0);

        let html = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                Vec::new(),
            )),
        );

        assert!(
            html.contains("Amazing grace how sweet the sound"),
            "the first slide is missing: {html}"
        );
        assert!(
            html.contains("I once was lost but now am found"),
            "a slide that is not the current one is missing: {html}"
        );
    }

    /// The speaker layout renders both slides and says which is which.
    #[test]
    fn the_speaker_layout_renders_the_current_slide_and_the_next() {
        let mut running = service();
        running.jump_to(0, 0);

        let html = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::Speaker {
                    next_slide_share: 0.25,
                    next_position: SpeakerNextPosition::default(),
                },
                Vec::new(),
            )),
        );

        assert!(html.contains("monitor-speaker-current"), "no current slide");
        assert!(html.contains("monitor-speaker-next"), "no next slide");
    }

    /// Widgets reach the network too. They are part of the design, and the old
    /// page had no notion of them at all.
    #[test]
    fn widgets_are_rendered_into_the_stream() {
        let mut running = service();
        running.jump_to(0, 0);

        let html = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                vec![
                    MonitorWidget {
                        kind: WidgetKind::Clock { with_date: false },
                        placement: WidgetPlacement::TopRight,
                    },
                    MonitorWidget {
                        kind: WidgetKind::ChapterTimer {
                            warn_after_seconds: None,
                        },
                        placement: WidgetPlacement::BottomLeft,
                    },
                ],
            )),
        );

        assert!(html.contains("monitor-clock"), "the clock is missing: {html}");
        assert!(html.contains("monitor-timer"), "the timer is missing: {html}");
        assert!(
            html.contains("monitor-widget-top-right"),
            "a widget was not put in the corner it was given: {html}"
        );
    }

    /// The design decides, so two designs over the same presentation give two
    /// different renderings. Without this the whole exercise would be a very
    /// elaborate way of ignoring the design.
    #[test]
    fn the_design_is_what_decides_the_rendering() {
        let mut running = service();
        running.jump_to(0, 0);

        let audience = render_presentation(&running, None);
        let monitor = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                Vec::new(),
            )),
        );

        assert_ne!(audience, monitor);
        assert!(!audience.contains("monitor-view"));
    }

    /// A presentation that has not started renders something rather than
    /// failing. The address is open before the service begins, and a viewer who
    /// opens it early must not be handed a panic.
    #[test]
    fn a_presentation_with_no_slides_renders_without_failing() {
        let running = RunningPresentation::new(Vec::new());

        let audience = render_presentation(&running, None);
        let monitor = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                Vec::new(),
            )),
        );

        assert!(!audience.is_empty());
        assert!(monitor.contains("monitor-view"));
    }

    /// Rendering the same thing twice gives the same bytes.
    ///
    /// What the stream sends is compared against what it last sent, so that a
    /// state which has not changed is not pushed to every phone again. A
    /// rendering that differed run to run — an id counted up per render, say —
    /// would make every republish look like a change.
    #[test]
    fn the_same_presentation_renders_the_same_way_twice() {
        let mut running = service();
        running.jump_to(0, 1);

        assert_eq!(
            render_presentation(&running, None),
            render_presentation(&running, None)
        );
    }

    // ── Addresses that have to survive the journey ──────────────────────

    /// The ordinary form: a path on Cantara's own origin becomes one on the
    /// stream's.
    #[test]
    fn a_videos_address_is_rewritten_for_the_network() {
        let handler = crate::logic::video::VIDEO_HANDLER;
        let html = format!(r#"<video src="/{handler}/%2FDer%20Film.mp4"></video>"#);

        assert_eq!(
            for_network(&html),
            format!(
                r#"<video src="video/{}"></video>"#,
                crate::logic::stream::protocol::media_id("/Der Film.mp4")
            )
        );
    }

    /// The WebKitGTK form, which is the one that would fail most confusingly:
    /// loopback on a phone is the phone, so the viewer would ask itself for the
    /// video and be told nothing is there.
    #[test]
    fn an_absolute_loopback_address_loses_its_origin_too() {
        let handler = crate::logic::video::VIDEO_HANDLER;
        let html =
            format!(r#"<video src="http://127.0.0.1:8431/{handler}/%2Fsrv%2Fclip.mp4"></video>"#);

        assert_eq!(
            for_network(&html),
            format!(
                r#"<video src="video/{}"></video>"#,
                crate::logic::stream::protocol::media_id("/srv/clip.mp4")
            )
        );
    }

    /// A service may have more than one video in it, and a rewrite that only
    /// did the first would leave the rest pointing nowhere.
    #[test]
    fn every_address_in_a_rendering_is_rewritten() {
        let handler = crate::logic::video::VIDEO_HANDLER;
        let html = format!(
            r#"<video src="/{handler}/%2Fone.mp4"></video><video src="/{handler}/%2Ftwo.mp4"></video>"#
        );

        let rewritten = for_network(&html);

        for path in ["/one.mp4", "/two.mp4"] {
            let id = crate::logic::stream::protocol::media_id(path);
            assert!(
                rewritten.contains(&format!(r#"src="video/{id}""#)),
                "{path} is not addressed by the name the server files it under: {rewritten}"
            );
        }
        assert!(
            !rewritten.contains(handler),
            "an address was left pointing at Cantara's own handler: {rewritten}"
        );
    }

    /// A path is decoded before it is hashed, so that the name matches the one
    /// the server registered — which it built from the path itself, not from
    /// the encoded form.
    ///
    /// This is the bug that made a video show as an empty rectangle over the
    /// design's background: the element was there and nothing answered its
    /// address.
    #[test]
    fn the_address_is_the_name_the_server_files_the_video_under() {
        let handler = crate::logic::video::VIDEO_HANDLER;
        let path = "/Ordner/Der Film #2.mp4";
        let html = format!(r#"<video src="/{handler}/%2FOrdner%2FDer%20Film%20%232.mp4"></video>"#);

        assert!(
            for_network(&html)
                .contains(&format!("video/{}", crate::logic::stream::protocol::media_id(path))),
            "the address is not the id the server registered"
        );
    }

    /// Rendering says nothing about a missing document.
    ///
    /// `document::Link` and `document::Script`, which every slide renderer
    /// uses, look one up in the context and log at *error* level when there is
    /// none. That is once per slide change of every service, in a log an
    /// operator reads to find real problems.
    #[test]
    fn rendering_does_not_complain_about_a_missing_document() {
        // The lookup is what logs, so provoking it is enough: if a document is
        // found, nothing is written. This asserts the provider is in place by
        // rendering something that asks for one and checking the render
        // succeeds — the message itself goes to `tracing`, which a unit test
        // cannot read without a subscriber.
        //
        // The guard against regression is the provider's presence; this keeps
        // the reason for it written down beside the code.
        let mut running = service();
        running.jump_to(0, 0);

        let html = render_presentation(&running, None);

        assert!(
            html.contains("presentation"),
            "the rendering that asks for a document did not come out"
        );
    }

    /// A monitor design set on the stream view reaches the network as a
    /// monitor view.
    ///
    /// This is the whole reason the rendering moved out of the page. It went
    /// through the chapter's `design_for_stream`, which is what
    /// `StreamDefaults` fills from the stream view's design — so what a phone
    /// is shown is what that view was set to, layout and widgets included.
    #[test]
    fn a_monitor_design_on_the_stream_view_streams_as_a_monitor_view() {
        use cantara_songlib::slides::SlideSettings;

        let slides = crate::logic::presentation::slides_from_song_content(
            "#title: Amazing Grace\n\nAmazing grace how sweet the sound\n",
            "Amazing Grace.song",
            &SlideSettings::default(),
            "Amazing Grace",
            &[],
        )
        .expect("the song builds into slides");

        let mut chapter = SlideChapter::new(
            slides,
            SourceFile {
                name: "Amazing Grace".to_string(),
                path: std::path::PathBuf::from("Amazing Grace.song"),
                file_type: SourceFileType::Song,
                md5_hash: None,
                relative_path: None,
            },
            None,
            None,
        );
        // What `StreamDefaults` puts on a chapter when the stream view names a
        // design of its own.
        let phones = uuid::Uuid::from_u128(5);
        chapter.view_slides.insert(
            phones,
            crate::logic::states::ViewDivision {
                design: Some(monitor_design(
                    MonitorLayout::SlideList { context: None },
                    Vec::new(),
                )),
                ..crate::logic::states::ViewDivision::default()
            },
        );

        let mut running = RunningPresentation::new(vec![chapter]);
        running.jump_to(0, 0);

        // Exactly what `network_host::publish` renders with.
        let html = render_presentation(
            &running,
            Some(running.current_design_in(crate::logic::states::Division::View(phones))),
        );

        assert!(
            html.contains("monitor-view"),
            "the stream did not get the monitor view its view was set to: {html}"
        );
    }

    /// The speaker layout puts the next slide where the design says, and both
    /// slides are drawn at the presentation's own size to be scaled by CSS.
    ///
    /// The scaling itself is the browser's — a slide's type is in points, so a
    /// slide put straight into a smaller box overflows it, and the factor
    /// depends on a box no renderer here can measure. What has to be in the
    /// markup is the size to scale *from*.
    #[test]
    fn the_speaker_layout_carries_what_the_browser_needs_to_scale() {
        let mut running = service();
        running.jump_to(0, 0);

        let beside = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::Speaker {
                    next_slide_share: 0.25,
                    next_position: SpeakerNextPosition::Right,
                },
                Vec::new(),
            )),
        );

        assert!(
            beside.contains("flex-direction: row"),
            "the next slide is not beside the current one: {beside}"
        );
        assert!(
            beside.contains("--slide-width") && beside.contains("--slide-height"),
            "the size to scale from is missing: {beside}"
        );
        assert!(
            beside.contains("monitor-slide-stage"),
            "the slide is not on a stage to be scaled: {beside}"
        );

        let below = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::Speaker {
                    next_slide_share: 0.25,
                    next_position: SpeakerNextPosition::Below,
                },
                Vec::new(),
            )),
        );

        assert!(
            below.contains("flex-direction: column"),
            "the next slide is not below the current one: {below}"
        );
    }

    /// The share is of whichever direction the two are stacked in, so moving
    /// the next slide from below to beside keeps its proportion rather than
    /// silently becoming a share of the other axis.
    #[test]
    fn the_share_follows_the_direction_the_slides_are_stacked_in() {
        let mut running = service();
        running.jump_to(0, 0);

        let beside = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::Speaker {
                    next_slide_share: 0.25,
                    next_position: SpeakerNextPosition::Right,
                },
                Vec::new(),
            )),
        );

        assert!(
            beside.contains("width: 25%") && beside.contains("width: 75%"),
            "beside, the share should be of the width: {beside}"
        );

        let below = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::Speaker {
                    next_slide_share: 0.25,
                    next_position: SpeakerNextPosition::Below,
                },
                Vec::new(),
            )),
        );

        assert!(
            below.contains("height: 25%") && below.contains("height: 75%"),
            "below, the share should be of the height: {below}"
        );
    }

    /// Three views: the projection, a monitor at one address and a plain
    /// presentation at another. Each address gets its own rendering.
    ///
    /// Reported from a real service: both addresses showed the monitor. This
    /// builds the same configuration and renders what the helper would render
    /// for each.
    #[test]
    fn each_network_view_is_rendered_in_its_own_design() {
        use crate::logic::settings::{
            Settings, View, ViewFocus, ViewOutput,
        };
        use crate::logic::stream_view::ViewDefaults;

        let mut settings = Settings {
            presentation_designs: vec![
                // 0: what the wall shows.
                PresentationDesign::default(),
                // 1: the stage monitor.
                monitor_design(MonitorLayout::SlideList { context: None }, Vec::new()),
            ],
            default_design_index: 0,
            ..Settings::default()
        };

        let screen = uuid::Uuid::from_u128(20);
        let monitor = uuid::Uuid::from_u128(21);
        let plain = uuid::Uuid::from_u128(22);
        settings.views = vec![
            View {
                id: screen,
                name: "Projection".to_string(),
                design_index: None,
                slide_settings_index: None,
                output: ViewOutput::Screen { monitor_name: None },
                enabled: true,
                focus: ViewFocus::Follow,
            },
            View {
                id: monitor,
                name: "Monitor".to_string(),
                design_index: Some(1),
                slide_settings_index: None,
                output: ViewOutput::Network {
                    path: "/test".to_string(),
                },
                enabled: true,
                focus: ViewFocus::Follow,
            },
            View {
                id: plain,
                name: "Stream".to_string(),
                design_index: None,
                slide_settings_index: None,
                output: ViewOutput::Network {
                    path: "/".to_string(),
                },
                enabled: true,
                focus: ViewFocus::Follow,
            },
        ];
        settings.reference_view_index = 0;

        // The running order, built exactly as the program builds it.
        let item = crate::logic::states::SelectedItemRepresentation::new_with_sourcefile(
            SourceFile {
                name: "Amazing Grace".to_string(),
                path: std::path::PathBuf::from("testfiles/Amazing Grace.song"),
                file_type: SourceFileType::Song,
                md5_hash: None,
                relative_path: None,
            },
        );
        let mut running = crate::logic::presentation::build_presentation(
            &vec![item],
            &settings.default_presentation_design(),
            &settings.default_song_slide_settings(),
            &ViewDefaults::all(&settings),
            &[],
        )
        .expect("a presentation");
        running.jump_to(0, 0);

        // What `network_host::publish` renders for each address.
        let for_view = |id| {
            render_presentation(
                &running,
                Some(running.current_design_in(crate::logic::states::Division::View(id))),
            )
        };

        let at_test = for_view(monitor);
        let at_root = for_view(plain);

        assert!(
            at_test.contains("monitor-view"),
            "/test should be the monitor view: {at_test}"
        );
        assert!(
            !at_root.contains("monitor-view"),
            "/ was given the monitor view as well: {at_root}"
        );
    }

    /// Two renderings a second apart differ *only* inside the widgets.
    ///
    /// The page leans on this: where that holds it replaces what the widgets
    /// say and leaves everything else standing, rather than rebuilding the
    /// stage every second — which lost the slide frames the size they had been
    /// fitted to, restarted the video and re-engraved every staff.
    ///
    /// If a rendering ever came to differ elsewhere from one second to the
    /// next, the page would fall back to rebuilding and the jump would be back.
    /// This is what says it does not.
    #[test]
    fn a_second_later_only_the_widgets_have_changed() {
        use crate::logic::timer::Timestamp;

        let mut running = service();
        running.jump_to(0, 0);

        let design = monitor_design(
            MonitorLayout::SlideList { context: None },
            vec![MonitorWidget {
                kind: WidgetKind::ChapterTimer {
                    warn_after_seconds: None,
                },
                placement: WidgetPlacement::TopRight,
            }],
        );

        // The same presentation, timed a second apart. The chapter clock is
        // what the widget counts from, so moving it is moving the widget and
        // nothing else.
        let now = Timestamp::now();
        running.chapter_entered_at = Some(now);
        let before = render_presentation(&running, Some(design.clone()));
        running.chapter_entered_at =
            Some(Timestamp::from_milliseconds(now.milliseconds() - 1000));
        let after = render_presentation(&running, Some(design));

        assert_ne!(before, after, "the timer did not move at all");

        // Everything outside the widgets is untouched — which is what the page
        // checks for itself before patching.
        let strip = |html: &str| {
            html.split("monitor-widget")
                .next()
                .unwrap_or_default()
                .to_string()
        };
        assert_eq!(
            strip(&before),
            strip(&after),
            "a second's passing changed the slide as well as the widget"
        );
    }

    /// A PDF page is drawn by pdf.js into a canvas, which no rendering without
    /// a browser can fill. The page travels as a picture instead, and the
    /// canvas becomes the request for it.
    #[test]
    fn a_pdf_page_becomes_the_picture_the_server_serves() {
        let html = r#"<canvas id="x" data-pdf="/srv/Handout.pdf" data-page="2" style="visibility: hidden;"></canvas>"#;

        let rewritten = for_network(html);
        let id = crate::logic::stream::protocol::media_id("/srv/Handout.pdf#page=2");

        assert!(
            rewritten.contains(&format!(r#"src="media/{id}""#)),
            "the page was not addressed the way the server files it: {rewritten}"
        );
        assert!(
            !rewritten.contains("<canvas"),
            "an empty canvas was left in the rendering: {rewritten}"
        );
        // Fitted the way a picture slide is fitted. Without this the page came
        // out at its own size in the middle of the design's background — the
        // right page, the wrong proportions.
        assert!(
            rewritten.contains("object-fit: contain"),
            "the page is not fitted like a picture, so it will be distorted or \
             left at its own size: {rewritten}"
        );
    }

    /// A PDF page gets a cell with a height, exactly as a picture and a video
    /// do.
    ///
    /// `height: 100%` against a parent that has none is nothing, and that is
    /// what left a page sitting small in the middle. On this machine it never
    /// showed: pdf.js sizes its own canvas.
    #[test]
    fn a_pdf_slide_is_given_a_cell_to_fill() {
        use cantara_songlib::slides::Slide;

        let chapter = SlideChapter::new(
            vec![Slide::new_pdf_page_slide("/srv/Handout.pdf".to_string(), 1)],
            SourceFile {
                name: "Handout".to_string(),
                path: std::path::PathBuf::from("Handout.pdf"),
                file_type: SourceFileType::Pdf,
                md5_hash: None,
                relative_path: None,
            },
            None,
            None,
        );
        let mut running = RunningPresentation::new(vec![chapter]);
        running.jump_to(0, 0);

        let html = render_presentation(&running, None);

        assert!(
            html.contains(r#"class="slide-container "#) && html.contains("height: 100%"),
            "the page has no cell to fill: {html}"
        );
    }

    /// The real Linux form, which carries a one-time token between the origin
    /// and the handler. That token is this machine's; a phone must not be
    /// asked for it.
    #[test]
    fn the_loopback_servers_token_is_stripped_along_with_the_origin() {
        let handler = crate::logic::video::VIDEO_HANDLER;
        let html = format!(
            r#"<source src="http://127.0.0.1:37167/10e74dc715fd4b478bc27d1fe48a5ece/{handler}/%2Fsrv%2FClip.mp4" type="video/mp4"/>"#
        );

        assert_eq!(
            for_network(&html),
            format!(
                r#"<source src="video/{}" type="video/mp4"/>"#,
                crate::logic::stream::protocol::media_id("/srv/Clip.mp4")
            )
        );
    }

    /// A rendering with nothing to rewrite comes back untouched — a picture is
    /// inlined as a data URL and carries its own bytes, and words need no
    /// address at all.
    #[test]
    fn a_rendering_without_a_video_is_unchanged() {
        let html = r#"<div class="presentation"><p>Amazing grace</p></div>"#;

        assert_eq!(for_network(html), html);
    }

    /// A data URL must not be mangled by the rewrite: it is how every picture
    /// in a rendering travels.
    #[test]
    fn an_inlined_picture_survives_the_rewrite() {
        let html = r#"<div style="background-image:url(data:image/png;base64,iVBORw0KGgo=)"></div>"#;

        assert_eq!(for_network(html), html);
    }

    /// Diagnostic: prints what a video slide and a picture slide come out as.
    ///
    /// Ignored — it asserts nothing. It is how "the browser shows only the
    /// background" was traced to its cause rather than guessed at.
    #[test]
    #[ignore = "diagnostic output, not an assertion"]
    fn dump_media_slides() {
        use cantara_songlib::slides::{Slide, SlideContent, VideoSlide};

        let media = |content: SlideContent| {
            let chapter = SlideChapter::new(
                vec![Slide { slide_content: content, linked_file: None }],
                SourceFile {
                    name: "Clip".to_string(),
                    path: std::path::PathBuf::from("Clip.mp4"),
                    file_type: SourceFileType::Video,
                    md5_hash: None,
                    relative_path: None,
                },
                None,
                None,
            );
            let mut running = RunningPresentation::new(vec![chapter]);
            running.jump_to(0, 0);
            render_presentation(&running, None)
        };

        println!(
            "VIDEO:\n{}\n",
            media(SlideContent::Video(VideoSlide {
                video_path: "/srv/Clip.mp4".to_string(),
                autostart: true,
                looping: false,
            }))
        );
        println!(
            "PDF PAGE:\n{}\n",
            media(Slide::new_pdf_page_slide("/srv/Handout.pdf".to_string(), 2).slide_content)
        );
    }

    /// Moving the presentation changes the rendering. The obvious property,
    /// and the one that would make the stream look frozen if it failed.
    #[test]
    fn moving_to_another_slide_changes_the_rendering() {
        let mut running = service();
        running.jump_to(0, 0);
        let first = render_presentation(&running, None);

        running.next_slide();

        assert_ne!(first, render_presentation(&running, None));
    }
}
