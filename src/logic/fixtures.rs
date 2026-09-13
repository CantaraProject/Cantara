//! Services to test against.
//!
//! Every test that renders something needs a presentation to render, and a
//! presentation is not a one-liner: slides, a chapter, a source file, and a
//! position to be at. Written out at each call site it is fifteen lines of
//! noise around one line of assertion, and — worse — fifteen lines that drift.
//! The moment two tests build "a song" slightly differently, a failure in one
//! stops meaning anything about the other.
//!
//! So the services live here, once. A test says which shape of service it
//! wants and gets a presentation standing on its first slide.
//!
//! # Why a module and not a `mod tests` helper
//!
//! The markup tests in [`crate::components::slide_markup`] and the rendering
//! tests in [`crate::components::stream_render`] need the same songs, and
//! [`crate::logic::network_host`] wants them too. A helper inside one test
//! module can be reached from nowhere else, which is how the same fixture came
//! to be written twice before this existed.
//!
//! It is also compiled — outside `cfg(test)` — into the build the browser
//! tests drive; see [`crate::logic::harness`]. A Playwright test asking for
//! "the service with a video in it" and a Rust test asserting on one have to
//! mean the same service, or a browser failure says nothing about the markup
//! test that passed.
//!
//! # Why the values are what they are
//!
//! Nothing here is arbitrary, and where something looks it, it is not:
//!
//! * The song is **Amazing Grace**, two slides, because a service with one
//!   slide cannot tell "shows the current slide" from "shows every slide" —
//!   which is exactly what a slide list has to be distinguishable by.
//! * A picture and a PDF name **files that exist**, under `testfiles/`. That
//!   looks like unnecessary care until you see what a made-up path renders as:
//!   a picture is inlined by reading it, so a slide naming a file that is not
//!   there comes out as `<div style="width: 100%; height: 100%;"></div>` — an
//!   empty box with nothing in the markup to say what it was meant to hold. A
//!   test written against that fixture would pass while asserting nothing.
//! * A **video** names a file that does not exist, and that is correct: a
//!   video slide renders to a `<source>` pointing at Cantara's asset handler
//!   and never opens the file. Giving it a real one would say the test needed
//!   something it does not.
//! * The ids are fixed ([`uuid::Uuid::from_u128`]) rather than fresh. A test
//!   that names a view has to be able to name the same view twice.

// A library of fixtures, not a set of call sites. Which of these is used
// depends on the build: the browser harness wants the services and none of the
// marks, the markup matrix wants both, and a test added tomorrow will want
// something that nothing uses today. Warning about the rest would mean either
// deleting fixtures that are about to be needed or annotating them one at a
// time, and neither is worth doing to a file whose whole job is to offer more
// than any one caller uses.
#![allow(dead_code, reason = "a fixture library is used differently by each build")]
// The rule against `expect` outside tests is right and this is the one place
// it should not apply. These functions build the *fixtures themselves*: a
// failure means the song this file contains no longer parses, or the picture
// beside it is not a picture. There is nothing to recover to — a test that
// carried on with no slides would assert against an empty stage and pass, and
// the browser harness must not serve a service it could not build. Failing
// loudly, at the first call, naming the fixture, is the behaviour wanted.
//
// The module was `cfg(test)` when this was written, so the lint did not see
// it; it does now that the harness build compiles it too. The reasoning has
// not changed, only who can read it.
#![allow(
    clippy::expect_used,
    reason = "a fixture that cannot be built has no fallback worth having"
)]

use cantara_songlib::slides::{
    Slide, SlideContent, SlideRow, SlideSettings, SimplePictureSlide,
};
use uuid::Uuid;

use crate::logic::settings::{
    MonitorDesign, MonitorLayout, MonitorWidget, PresentationDesign, PresentationDesignSettings,
    SpeakerNextPosition, WidgetKind, WidgetPlacement,
};
use crate::logic::sourcefiles::{SourceFile, SourceFileType};
use crate::logic::states::{RunningPresentation, SlideChapter};

/// The song every text fixture is built from.
///
/// Two slides, so that a layout showing the whole service can be told apart
/// from one showing the slide that is up.
const AMAZING_GRACE: &str = "#title: Amazing Grace\n\nAmazing grace how sweet the sound\nThat saved a wretch like me\n\n---\n\nI once was lost but now am found\nWas blind but now I see\n";

/// A line from the first slide, for asserting that the current slide is drawn.
pub const FIRST_LINE: &str = "Amazing grace how sweet the sound";

/// A line from the *second* slide, for asserting that a layout shows more than
/// the slide that is up.
pub const SECOND_LINE: &str = "I once was lost but now am found";

/// A file under `testfiles/`, named absolutely.
///
/// Absolute because a slide carries the path it was built from and nothing
/// resolves it again later — a relative one would be read against whatever
/// directory the test runner happened to start in.
fn testfile(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testfiles")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// A real PDF of one page.
pub fn pdf_path() -> String {
    testfile("Example.pdf")
}

/// A real picture, small enough to inline without making a test slow.
pub fn picture_path() -> String {
    testfile("Banner.png")
}

/// A video that does not exist, and does not need to — see the module
/// documentation.
pub const VIDEO_PATH: &str = "/srv/fixtures/Clip.mp4";

/// A library entry naming `name`, of the kind given.
///
/// The path is `testfiles/<name>`, relative, because that is what a running
/// order built by [`crate::logic::presentation::build_presentation`] is handed
/// and it reads the file from there.
pub fn source_file(name: &str, kind: SourceFileType) -> SourceFile {
    SourceFile {
        name: name
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(name)
            .to_string(),
        path: std::path::PathBuf::from("testfiles").join(name),
        file_type: kind,
        md5_hash: None,
        relative_path: None,
    }
}

/// A running order of one chapter, standing on its first slide.
///
/// `kind` decides what the chapter is said to have come from, which is not
/// cosmetic: a source file's type is what the console and the jump sidebar
/// read to decide how to describe it.
pub fn service_of(slides: Vec<Slide>, name: &str, kind: SourceFileType) -> RunningPresentation {
    let chapter = SlideChapter::new(
        slides,
        SourceFile {
            name: name.to_string(),
            path: std::path::PathBuf::from(name),
            file_type: kind,
            md5_hash: None,
            relative_path: None,
        },
        None,
        None,
    );

    let mut running = RunningPresentation::new(vec![chapter]);
    // On the first slide rather than before it. A presentation that has not
    // started renders nothing much, and a test asserting on a slide would be
    // asserting on an empty stage without saying so.
    running.jump_to(0, 0);
    running
}

/// The ordinary service: one song, standing on its **title** slide.
///
/// The title is where a song begins, so this is where a presentation of one
/// stands when it starts. A test that wants the words has to move on — see
/// [`song_service_on_the_words`], and note that the difference is easy to miss:
/// a rendering standing on the title contains "Amazing Grace" and none of the
/// verse, which reads like a rendering that lost its text.
pub fn song_service() -> RunningPresentation {
    let slides = crate::logic::presentation::slides_from_song_content(
        AMAZING_GRACE,
        "Amazing Grace.song",
        &SlideSettings::default(),
        "Amazing Grace",
        &[],
    )
    .expect("the fixture song builds into slides");

    service_of(slides, "Amazing Grace.song", SourceFileType::Song)
}

/// The same song, moved on to the first slide of words.
pub fn song_service_on_the_words() -> RunningPresentation {
    let mut running = song_service();
    running.jump_to(0, 1);
    running
}

/// A service of one PDF page.
pub fn pdf_service() -> RunningPresentation {
    service_of(
        vec![Slide::new_pdf_page_slide(pdf_path(), 1)],
        "Example.pdf",
        SourceFileType::Pdf,
    )
}

/// A service of one video, set to start by itself.
pub fn video_service() -> RunningPresentation {
    service_of(
        vec![Slide::new_video_slide(VIDEO_PATH.to_string(), true, false)],
        "Clip.mp4",
        SourceFileType::Video,
    )
}

/// A service of one picture.
///
/// `SimplePictureSlide`'s field is private and the song library offers no
/// constructor, so it is built through serde — the same way
/// [`crate::logic::presentation`] builds one.
pub fn picture_service() -> RunningPresentation {
    let picture: SimplePictureSlide =
        serde_json::from_value(serde_json::json!({ "picture_path": picture_path() }))
            .expect("a picture slide is buildable from its own serialisation");

    service_of(
        vec![Slide {
            slide_content: SlideContent::SimplePicture(picture),
            linked_file: None,
        }],
        "Banner.png",
        SourceFileType::Image,
    )
}

/// A service of one blacked-out slide.
pub fn empty_service() -> RunningPresentation {
    service_of(
        vec![Slide::new_empty_slide(true)],
        "Empty",
        SourceFileType::Song,
    )
}

/// A service of one title slide.
pub fn title_service() -> RunningPresentation {
    service_of(
        vec![Slide::new_title_slide(
            "Amazing Grace".to_string(),
            Some("John Newton".to_string()),
        )],
        "Amazing Grace.song",
        SourceFileType::Song,
    )
}

/// A service of one slide carrying the same verse in two languages.
pub fn multi_language_service() -> RunningPresentation {
    service_of(
        vec![Slide::new_multi_language_content_slide(
            vec![FIRST_LINE.to_string(), "Oh teure Gnade wunderbar".to_string()],
            Vec::new(),
            None,
        )],
        "Amazing Grace.song",
        SourceFileType::Song,
    )
}

/// A service of one slide with notation over its words.
///
/// The notation is a complete ABC tune, because that is what the engraver in
/// the page is handed — a fragment renders as nothing, and a fixture that
/// rendered as nothing would make the notation tests pass for the wrong
/// reason.
pub fn notation_service() -> RunningPresentation {
    service_of(
        vec![Slide::new_complex_slide(
            vec![
                SlideRow::notation("X:1\nM:3/4\nL:1/4\nK:F\nC | F2 (A/F/) | A2 G |", 8),
                SlideRow::lyrics(Some("en".to_string()), FIRST_LINE),
            ],
            Vec::new(),
            None,
            1,
        )],
        "Amazing Grace.song",
        SourceFileType::Song,
    )
}

/// Cantara's own design: what the congregation is shown.
pub fn audience_design() -> PresentationDesign {
    PresentationDesign::default()
}

/// A monitor design with the layout and widgets asked for.
pub fn monitor_design(layout: MonitorLayout, widgets: Vec<MonitorWidget>) -> PresentationDesign {
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

/// A monitor showing the whole service as a list.
pub fn slide_list_design() -> PresentationDesign {
    monitor_design(MonitorLayout::SlideList { context: None }, Vec::new())
}

/// A monitor showing what is up and what comes next.
pub fn speaker_design(next_position: SpeakerNextPosition) -> PresentationDesign {
    monitor_design(
        MonitorLayout::Speaker {
            next_slide_share: 0.25,
            next_position,
        },
        Vec::new(),
    )
}

/// One of each widget, in two different corners.
///
/// Both kinds together rather than one at a time, because the thing worth
/// asserting about widgets is usually that *each* reached the corner it was
/// given — a single widget cannot show that a second one went somewhere else.
pub fn both_widgets() -> Vec<MonitorWidget> {
    vec![
        MonitorWidget {
            kind: WidgetKind::Clock { with_date: true },
            placement: WidgetPlacement::TopRight,
        },
        MonitorWidget {
            kind: WidgetKind::ChapterTimer {
                warn_after_seconds: None,
            },
            placement: WidgetPlacement::BottomLeft,
        },
    ]
}

/// Every design a presentation can be drawn in, named.
///
/// The point of a matrix test: a new layout added to [`MonitorLayout`] belongs
/// here, and then every test that walks this list covers it without being
/// touched. A list built at each call site is a list that gets one new entry
/// and not the other four.
pub fn every_design() -> Vec<(&'static str, PresentationDesign)> {
    vec![
        ("audience", audience_design()),
        ("slide list", slide_list_design()),
        ("speaker, next beside", speaker_design(SpeakerNextPosition::Right)),
        ("speaker, next below", speaker_design(SpeakerNextPosition::Below)),
    ]
}

/// A kind of slide, the service holding one, and how to recognise it.
pub struct SlideKind {
    /// What to call it when a test fails.
    pub name: &'static str,
    /// A service of exactly this kind of slide, standing on it.
    pub running: RunningPresentation,
    /// Something that must appear in the markup wherever this slide is
    /// **drawn** — as opposed to merely listed.
    ///
    /// This is the whole point of the matrix, so it is worth saying what makes
    /// a good one: it has to be something only a *faithful* rendering
    /// produces. `"presentation"` would be a bad mark — every rendering has
    /// that, including one that drew an empty stage. The path of the file, the
    /// words of the verse, the tune itself: those cannot appear by accident.
    pub drawn: String,
}

/// Every kind of slide a service can contain, one service each.
///
/// As [`every_design`]: a variant added to
/// [`SlideContent`](cantara_songlib::slides::SlideContent) belongs here, and
/// every test that walks this list covers it without being touched.
pub fn every_slide_kind() -> Vec<SlideKind> {
    let kind = |name, running, drawn: &str| SlideKind {
        name,
        running,
        drawn: drawn.to_string(),
    };

    vec![
        kind("words", song_service_on_the_words(), FIRST_LINE),
        kind("title", title_service(), "Amazing Grace"),
        kind("two languages", multi_language_service(), "Oh teure Gnade wunderbar"),
        // The tune itself, not the lyrics beside it: a notation slide that
        // rendered its words and lost its notes would otherwise pass.
        kind("notation", notation_service(), "K:F"),
        // The bytes. A picture travels inlined, so a rendering that names the
        // file but did not read it is not a rendering of the picture.
        kind("picture", picture_service(), "data:image/png;base64,"),
        kind("pdf page", pdf_service(), "data-pdf="),
        // The file, however its address is spelled — the path is percent-
        // encoded into the handler's URL, and the name survives that.
        kind("video", video_service(), "Clip.mp4"),
        kind("empty", empty_service(), "empty-content"),
    ]
}

/// The designs that draw whole slides, as opposed to listing them.
///
/// The audience renderer and both speaker layouts put the slide itself on the
/// stage. [`MonitorLayout::SlideList`] does not — it shows a written account of
/// the service — so a test asserting that a picture's bytes are in the markup
/// is asking the list for something it is not for. See
/// [`every_design`] for the whole set.
pub fn designs_that_draw_slides() -> Vec<(&'static str, PresentationDesign)> {
    vec![
        ("audience", audience_design()),
        ("speaker, next beside", speaker_design(SpeakerNextPosition::Right)),
        ("speaker, next below", speaker_design(SpeakerNextPosition::Below)),
    ]
}

/// A fixed view id, so that a test can name the same view twice.
pub fn view_id(which: u128) -> Uuid {
    Uuid::from_u128(which)
}

/// The same service, with `view` shown in a design of its own.
///
/// This is what `StreamDefaults` puts on a chapter when a view names a design
/// — a second reading of the same service, kept beside the projection's rather
/// than replacing it. Built here rather than at each call site because
/// assembling a [`ViewDivision`](crate::logic::states::ViewDivision) by hand is
/// four lines that say nothing about the test doing it.
pub fn shown_in(
    mut running: RunningPresentation,
    view: Uuid,
    design: PresentationDesign,
) -> RunningPresentation {
    for chapter in running.presentation.iter_mut() {
        chapter.view_slides.insert(
            view,
            crate::logic::states::ViewDivision {
                design: Some(design.clone()),
                ..crate::logic::states::ViewDivision::default()
            },
        );
    }
    running
}
