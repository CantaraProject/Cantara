//! What every kind of slide, in every kind of design, comes out as.
//!
//! # The class of failure this is for
//!
//! Building the monitor views (`docs/specs/0003-add-monitor-view.md`) produced
//! nine defects. Not one was caught by the 735 tests that existed; every one
//! was found by starting the program and looking at it. Five of the nine were
//! the same mistake in different clothes — **markup that depends on a browser
//! event to become itself**:
//!
//! * the slide was hidden until `onmounted` fired, so a rendering that never
//!   mounts had no slide in it;
//! * the tune a staff is engraved from lived only inside the mount handler,
//!   so the staff was an empty box anywhere else;
//! * a PDF page was a `<canvas>` that said nothing about which page it was;
//! * a picture's cell had no height, which never showed because pdf.js sizes
//!   its own canvas;
//! * nothing provided a `Document`, so every render logged an error.
//!
//! Every one of those is visible in the HTML, with no browser involved. That
//! is what these tests read.
//!
//! # Why properties and not a golden file
//!
//! The obvious way to test rendering is to record the output and compare
//! against it. It is also the way that stops working: generated markup changes
//! whenever a style is edited, so the file goes red for reasons nobody cares
//! about, and the fix is always to re-record it. After the third time, nobody
//! reads what changed — and the one run where the slide vanished is re-recorded
//! along with the rest.
//!
//! So each test asserts a property, and each property is something a *broken*
//! rendering would not have. See [`fixtures::SlideKind::drawn`] for what makes
//! a mark worth asserting on.
//!
//! # Why through `render_presentation`
//!
//! [`DesignedPresentation`](crate::components::monitor_view::DesignedPresentation)
//! is the one place that decides between the audience renderer and a monitor
//! view, and [`render_presentation`] is the shortest way to reach it without a
//! window. Testing the components below it one at a time would test the parts
//! and miss the decision — which is exactly where two of the nine defects were.

#[cfg(test)]
mod tests {
    use crate::components::stream_render::render_presentation;
    use crate::logic::fixtures;

    /// Every kind of slide, in every design, produces a stage with something
    /// on it.
    ///
    /// The broadest thing worth saying, and the one that would have caught the
    /// mount gate: a rendering that came out as the design's background and an
    /// empty box passes nothing here.
    #[test]
    fn every_slide_in_every_design_renders_a_stage() {
        for (design_name, design) in fixtures::every_design() {
            for kind in fixtures::every_slide_kind() {
                let html = render_presentation(&kind.running, Some(design.clone()));

                assert!(
                    !html.is_empty(),
                    "{design_name} / {kind} rendered nothing at all",
                    kind = kind.name
                );
                assert!(
                    html.contains("presentation") || html.contains("monitor-view"),
                    "{design_name} / {kind} rendered no stage: {html}",
                    kind = kind.name
                );
            }
        }
    }

    /// A design that draws slides draws *this* slide.
    ///
    /// The list layout is left out on purpose — it shows a written account of
    /// the service rather than the slide, so asking it for a picture's bytes
    /// would be asking it for something it is not for. What it owes is
    /// asserted separately, below.
    #[test]
    fn every_kind_of_slide_is_drawn_and_not_merely_framed() {
        for (design_name, design) in fixtures::designs_that_draw_slides() {
            for kind in fixtures::every_slide_kind() {
                let html = render_presentation(&kind.running, Some(design.clone()));

                assert!(
                    html.contains(&kind.drawn),
                    "{design_name} drew a {kind} slide without {mark} in it, so \
                     whatever is on that stage is not the slide: {html}",
                    kind = kind.name,
                    mark = kind.drawn
                );
            }
        }
    }

    /// The list layout has an entry for every kind of slide, and none of them
    /// is blank.
    ///
    /// What a list owes: a stage monitor showing the service as a list must
    /// not have a gap in it where the video was. A person reading it has to be
    /// able to count the items and find where they are.
    #[test]
    fn the_slide_list_says_something_about_every_kind_of_slide() {
        for kind in fixtures::every_slide_kind() {
            let html = render_presentation(&kind.running, Some(fixtures::slide_list_design()));

            assert!(
                html.contains("presenter-slide-item"),
                "the list has no entry for a {kind} slide: {html}",
                kind = kind.name
            );
            // An entry that renders as nothing is a gap in the list, which is
            // worse than an entry saying "picture": the reader cannot tell
            // whether something is missing or whether they have miscounted.
            assert!(
                !html.contains("<div class=\"presenter-slide-item active\"></div>"),
                "the list's entry for a {kind} slide is empty: {html}",
                kind = kind.name
            );
        }
    }

    /// Rendering the same thing twice gives the same bytes, for every
    /// combination.
    ///
    /// The stream compares what it is about to send against what it last sent,
    /// so that a state which has not changed is not pushed to every phone
    /// again. A rendering that differed run to run — an id counted up per
    /// render, say — would make every republish look like a change, and every
    /// phone in the building would rebuild its page on a timer.
    #[test]
    fn every_combination_renders_the_same_way_twice() {
        for (design_name, design) in fixtures::every_design() {
            for kind in fixtures::every_slide_kind() {
                assert_eq!(
                    render_presentation(&kind.running, Some(design.clone())),
                    render_presentation(&kind.running, Some(design.clone())),
                    "{design_name} / {kind} renders differently each time",
                    kind = kind.name
                );
            }
        }
    }

    /// An element's identifier is the same in two renderings of the same
    /// slide.
    ///
    /// Found by [`every_combination_renders_the_same_way_twice`] and worth its
    /// own name, because the identifier is the *reason* and the consequence is
    /// nowhere near it. A notation staff and a PDF canvas each carry an id for
    /// the script that draws into them; both were a fresh `Uuid` per render.
    ///
    /// So every rendering of such a slide differed from the last, and the
    /// stream's "this has not changed, do not push it again" check never held.
    /// A notation slide on screen meant every phone in the building rebuilding
    /// its page once a second — re-engraving the staff, restarting any video —
    /// for as long as the slide was up. Nothing about that looks like an
    /// identifier when you are watching it happen.
    #[test]
    fn an_elements_identifier_does_not_change_between_renderings() {
        for (name, running) in [
            ("notation", fixtures::notation_service()),
            ("pdf page", fixtures::pdf_service()),
        ] {
            let design = Some(fixtures::audience_design());

            assert_eq!(
                render_presentation(&running, design.clone()),
                render_presentation(&running, design),
                "a {name} slide's element is identified differently each time it \
                 is rendered, so nothing downstream can tell it has not changed"
            );
        }
    }

    /// A list shows the whole service, not only the slide that is up.
    ///
    /// The point of the layout, and something a single-slide renderer cannot
    /// express — which is why the stream used to show a monitor design as a
    /// wall of text.
    #[test]
    fn the_slide_list_shows_slides_the_service_has_not_reached() {
        let html = render_presentation(
            &fixtures::song_service(),
            Some(fixtures::slide_list_design()),
        );

        assert!(
            html.contains(fixtures::FIRST_LINE),
            "the slide that is up is missing from the list: {html}"
        );
        assert!(
            html.contains(fixtures::SECOND_LINE),
            "the list stops at the current slide, so it is not a list: {html}"
        );
    }

    /// Every widget reaches the corner it was given.
    ///
    /// Two at once rather than one at a time: a single widget cannot show that
    /// a second one went somewhere else, and "somewhere else" is the failure —
    /// a clock and a timer stacked in the same corner are two overlapping
    /// widgets, not two widgets.
    #[test]
    fn each_widget_is_rendered_in_the_corner_it_was_given() {
        let html = render_presentation(
            &fixtures::song_service(),
            Some(fixtures::monitor_design(
                crate::logic::settings::MonitorLayout::SlideList { context: None },
                fixtures::both_widgets(),
            )),
        );

        assert!(html.contains("monitor-clock"), "the clock is missing: {html}");
        assert!(html.contains("monitor-timer"), "the timer is missing: {html}");
        assert!(
            html.contains("monitor-widget-top-right"),
            "the clock did not reach the corner it was given: {html}"
        );
        assert!(
            html.contains("monitor-widget-bottom-left"),
            "the timer did not reach the corner it was given: {html}"
        );
    }

    /// A design with no widgets renders none.
    ///
    /// The other half of the same property. A view whose design asks for
    /// nothing must not get a clock by default — and the widget ticker must not
    /// start, which is what puts a second's work on a machine that is also
    /// driving a projection.
    #[test]
    fn a_design_without_widgets_renders_none() {
        let html = render_presentation(
            &fixtures::song_service(),
            Some(fixtures::slide_list_design()),
        );

        assert!(
            !html.contains("monitor-widget"),
            "a design that asked for no widgets got one: {html}"
        );
    }

    // ── What a browser would otherwise have had to supply ────────────────

    /// The tune is in the markup, not only in a handler that runs on mount.
    ///
    /// This is the notation defect exactly: the ABC source was read in
    /// `onmounted` and handed to the engraver from there, so a rendering
    /// without a mount carried an empty container. On the network that is a
    /// blank rectangle where the melody should be, and the operator sees the
    /// staff perfectly well on their own screen.
    #[test]
    fn a_notation_slide_carries_the_tune_it_is_engraved_from() {
        let html = render_presentation(
            &fixtures::notation_service(),
            Some(fixtures::audience_design()),
        );

        assert!(
            html.contains("data-abc="),
            "the staff has nothing to engrave from: {html}"
        );
        assert!(
            html.contains("K:F"),
            "the tune itself is missing, so the container is empty: {html}"
        );
    }

    /// A PDF page says which document and which page it depicts.
    ///
    /// pdf.js fills the canvas in the window and nowhere else, so the page has
    /// to travel as a picture instead — and the only way anything can look up
    /// *which* picture is if the canvas says so. Without these two attributes
    /// the rewrite in [`crate::components::stream_render`] has nothing to work
    /// from, and a viewer gets an empty box.
    #[test]
    fn a_pdf_page_says_which_page_it_is() {
        let html = render_presentation(&fixtures::pdf_service(), Some(fixtures::audience_design()));

        assert!(
            html.contains(&format!(r#"data-pdf="{}""#, fixtures::pdf_path())),
            "the canvas does not say which document it is: {html}"
        );
        assert!(
            html.contains(r#"data-page="1""#),
            "the canvas does not say which page it is: {html}"
        );
    }

    /// A picture travels as its own bytes.
    ///
    /// A file system path in `src` is not something a web view will fetch, and
    /// a phone on the network has no access to the operator's disk at all. The
    /// only form that works on both is the picture inlined.
    #[test]
    fn a_picture_travels_as_its_own_bytes() {
        let html = render_presentation(
            &fixtures::picture_service(),
            Some(fixtures::audience_design()),
        );

        assert!(
            html.contains("data:image/png;base64,"),
            "the picture is not in the markup, so only its absence travels: {html}"
        );
        assert!(
            !html.contains(&fixtures::picture_path()),
            "the rendering carries a path off this machine's disk: {html}"
        );
    }

    /// A picture, a video and a PDF page each get a cell with a height.
    ///
    /// `height: 100%` against a parent that has none is nothing. That is what
    /// left a PDF page sitting small in the middle of the design's background,
    /// and it never showed on the machine it was built on — pdf.js sizes its
    /// own canvas, so only the network rendering, where the page is an image,
    /// ever exposed it.
    #[test]
    fn media_slides_are_given_a_cell_to_fill() {
        for (name, running) in [
            ("picture", fixtures::picture_service()),
            ("video", fixtures::video_service()),
            ("pdf page", fixtures::pdf_service()),
        ] {
            let html = render_presentation(&running, Some(fixtures::audience_design()));

            assert!(
                html.contains(r#"class="slide-container"#) && html.contains("height: 100%"),
                "a {name} slide has no cell to fill, so it will sit at its own \
                 size in the middle: {html}"
            );
        }
    }

    /// A slide on a stage monitor carries the size it was laid out at.
    ///
    /// A slide's type is in points, so a slide put straight into a smaller box
    /// overflows it — the fitting is done by measuring the box in the browser
    /// and scaling. What the markup has to supply is the size to scale *from*,
    /// and it has to be the size the presentation is actually laid out at
    /// rather than a guess: line breaks depend on it, and a monitor breaking
    /// them somewhere else is not showing the speaker what the room sees.
    #[test]
    fn a_slide_on_a_monitor_carries_the_size_to_scale_from() {
        let html = render_presentation(
            &fixtures::song_service(),
            Some(fixtures::speaker_design(
                crate::logic::settings::SpeakerNextPosition::Right,
            )),
        );

        assert!(
            html.contains("--slide-width") && html.contains("--slide-height"),
            "the size to scale from is missing, so the slide cannot be fitted: {html}"
        );
        assert!(
            html.contains("monitor-slide-stage"),
            "the slide is not on a stage to be scaled: {html}"
        );
    }

    /// The projection plays the video; a monitor mirrors it.
    ///
    /// Reported from a real service: the video was black on the stage monitor.
    /// The element was there and nothing had ever started it — a monitor view
    /// shows, it does not control, so its `<video>` takes no commands of its
    /// own and follows the one the room is watching instead. Two things have to
    /// be true at once, and asserting either alone would miss the defect:
    /// the monitor's copy must be a mirror, and the projection's must not.
    #[test]
    fn a_video_plays_on_the_projection_and_is_mirrored_on_a_monitor() {
        let audience = render_presentation(
            &fixtures::video_service(),
            Some(fixtures::audience_design()),
        );
        let monitor = render_presentation(
            &fixtures::video_service(),
            Some(fixtures::speaker_design(
                crate::logic::settings::SpeakerNextPosition::Right,
            )),
        );

        assert!(
            audience.contains("slide-video-live"),
            "the projection's video is not the one that plays: {audience}"
        );
        assert!(
            monitor.contains("slide-video-mirror"),
            "the monitor's video does not follow the room's, so it will sit \
             black: {monitor}"
        );
        assert!(
            !monitor.contains("slide-video-live"),
            "the monitor is playing a second copy of the video: {monitor}"
        );
    }

    // ── The design is what decides ───────────────────────────────────────

    /// Two designs over the same service give two different renderings.
    ///
    /// Without this the whole matrix would be a very elaborate way of ignoring
    /// the design — which is what the old JavaScript renderer did, and why a
    /// monitor design reached a phone as a wall of text.
    #[test]
    fn no_two_designs_render_the_same_service_alike() {
        let running = fixtures::song_service();

        let mut seen: Vec<(&str, String)> = Vec::new();
        for (name, design) in fixtures::every_design() {
            let html = render_presentation(&running, Some(design));

            if let Some((other, _)) = seen.iter().find(|(_, previous)| previous == &html) {
                panic!("{name} and {other} render the same service identically, so one of them is not being applied");
            }
            seen.push((name, html));
        }
    }

    /// An audience design is not a monitor view, and a monitor design is.
    ///
    /// The decision itself, asserted directly. It was made in three places
    /// before it was made in one, and two of the three did not know monitor
    /// designs existed.
    #[test]
    fn the_kind_of_design_decides_which_renderer_draws_it() {
        let running = fixtures::song_service();

        assert!(
            !render_presentation(&running, Some(fixtures::audience_design()))
                .contains("monitor-view"),
            "an audience design was drawn as a monitor view"
        );
        assert!(
            render_presentation(&running, Some(fixtures::slide_list_design()))
                .contains("monitor-view"),
            "a monitor design was drawn as a plain slide"
        );
    }

    /// A service that has not started renders rather than failing.
    ///
    /// The address is open before the service begins, and whoever opens it
    /// early — which on a phone is everybody — must be handed a page and not a
    /// panic.
    #[test]
    fn a_service_that_has_not_started_renders_in_every_design() {
        let running = crate::logic::states::RunningPresentation::new(Vec::new());

        for (name, design) in fixtures::every_design() {
            let html = render_presentation(&running, Some(design));

            assert!(!html.is_empty(), "{name} rendered nothing before the service began");
        }
    }
}
