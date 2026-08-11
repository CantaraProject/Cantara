//! The second reading of the same service: what the network stream shows when
//! it is not simply showing the projection.
//!
//! A projector and a phone are not the same surface. A design built for a wall
//! ten metres away — a photograph behind it, forty-point type, two lines to a
//! slide — is not the one to read from a pew, and a congregation following on
//! their own screens can perfectly well be given the whole verse at once while
//! the wall goes two lines at a time. So both the design and the slide settings
//! may differ between the two, generally and song by song.
//!
//! The projection stays the reference. It is what the presenter console counts
//! in, what the moderator presses the button for, and what a slide number
//! means. The stream is a *second* set of slides over the same song, and every
//! slide of the projection is mapped to the one on the phone that shows it —
//! see [`map_slides`].
//!
//! That mapping is only well defined if a projection slide never straddles two
//! stream slides, which is what [`stream_slide_settings`] is for: a viewer's
//! slide has to hold a whole number of the projection's, or none of the rules
//! above hold. Everything here is pure and tested without a socket, a window or
//! a song file in sight.

use cantara_songlib::slides::{Slide, SlideContent, SlideSettings};

use crate::logic::settings::PresentationDesign;

/// What an element of the service falls back on for the stream when it names
/// nothing of its own.
///
/// Both halves are optional and independent: a service may well want a lighter
/// design on the phones and the very same slide division, or the other way
/// round. `None` in either means "whatever the projection does" — which is the
/// ordinary case and costs nothing, since the stream then shows the
/// projection's own slides rather than a second set of them.
#[derive(Clone, PartialEq, Default)]
pub struct StreamDefaults {
    pub design: Option<PresentationDesign>,
    pub slide_settings: Option<SlideSettings>,
}

impl StreamDefaults {
    /// What the service has been set up to send, generally.
    ///
    /// The two choices are kept as indices into the lists the user maintains,
    /// so that editing a design reaches the phones the same way it reaches the
    /// wall. An index pointing past the end of its list — a design deleted
    /// since it was chosen — is read as "no choice" rather than as a reason to
    /// fall over in the middle of a service.
    pub fn of(settings: &crate::logic::settings::Settings) -> StreamDefaults {
        StreamDefaults {
            design: settings
                .stream
                .design_index
                .and_then(|index| settings.presentation_designs.get(index).cloned()),
            slide_settings: settings
                .stream
                .slide_settings_index
                .and_then(|index| {
                    settings
                        .song_slide_settings
                        .get(index)
                        .map(|named| named.settings.clone())
                }),
        }
    }

}

/// The slide settings the stream may actually use, given what the projection
/// uses.
///
/// Everything is taken as asked for except the line wrap, which is not the
/// user's alone to choose: the projection is the reference, and a stream slide
/// that held one and a half of its slides would leave a slide change on the
/// wall landing in the middle of a slide on the phones. So the wrap is either
/// a whole multiple of the projection's or it is off entirely, and a value
/// between two multiples is rounded up to the next one rather than refused —
/// the user asked for "roughly this much at a time", and the nearest amount
/// that works is a better answer than none.
///
/// Off entirely means the whole verse, which always contains whatever the
/// projection is showing and is therefore always safe.
pub fn stream_slide_settings(
    projection: &SlideSettings,
    wanted: &SlideSettings,
) -> SlideSettings {
    let mut settings = wanted.clone();
    settings.max_lines = reconcile_max_lines(projection.max_lines, wanted.max_lines);
    settings
}

/// The wrap the stream ends up with. Split out from
/// [`stream_slide_settings`] so the user interface can say what a chosen value
/// will actually come to before it is used.
pub fn reconcile_max_lines(projection: Option<usize>, wanted: Option<usize>) -> Option<usize> {
    match (projection, wanted) {
        // The whole verse. Contains whatever the projection is showing,
        // whatever that is.
        (_, None) => None,
        // The projection shows whole verses, so there is nothing to be a
        // multiple *of* — and any wrap at all would cut one of its slides in
        // half. The whole verse is the only division that works.
        (None, Some(_)) => None,
        (Some(0), Some(_)) => None,
        (Some(reference), Some(wanted)) => {
            // Never below the projection's own wrap: a viewer's slide showing
            // less than the wall does is the one thing this must not produce.
            let steps = wanted.div_ceil(reference).max(1);
            Some(reference * steps)
        }
    }
}

/// For every slide of the projection, the slide of the stream that shows it.
///
/// The two sets are generated independently from the same song, so they are
/// matched on what they *say* rather than on arithmetic over their lengths.
/// That holds up where counting would not: a stream whose settings also drop
/// the title slide, or leave out the spoiler, has a different number of slides
/// for reasons no ratio describes.
///
/// The walk only ever moves forward. A song repeats itself — a refrain is the
/// same words three times over — and matching by content alone would send the
/// phones back to the first chorus every time the wall reached the second.
///
/// Where a projection slide has no counterpart at all (the title slide, when
/// the stream is set up without one) it maps to wherever the walk had got to,
/// so a viewer stays on the slide before rather than being thrown to the end.
pub fn map_slides(projection: &[Slide], stream: &[Slide]) -> Vec<usize> {
    if stream.is_empty() {
        return vec![0; projection.len()];
    }

    let shown: Vec<Shows> = stream.iter().map(Shows::of).collect();
    let last = stream.len() - 1;
    let mut cursor = 0usize;

    projection
        .iter()
        .map(|slide| {
            let wanted = Shows::of(slide);
            let found = (cursor..stream.len()).find(|&index| shown[index].contains(&wanted));
            cursor = found.unwrap_or(cursor).min(last);
            cursor
        })
        .collect()
}

/// What a slide puts on the screen, reduced to what two slides can be compared
/// on.
#[derive(PartialEq, Debug)]
enum Shows {
    /// Deliberately nothing — the gap between two elements of a service.
    Nothing,
    /// A picture or a page of a document, named by where it came from. Two
    /// slide settings cannot divide a PDF differently, so these always line up
    /// one to one.
    Picture(String),
    /// Words, one entry per line.
    Words(Vec<String>),
}

impl Shows {
    fn of(slide: &Slide) -> Shows {
        match &slide.slide_content {
            SlideContent::Empty(_) => Shows::Nothing,
            SlideContent::SimplePicture(picture) => {
                Shows::Picture(crate::logic::presentation::get_picture_path(picture))
            }
            SlideContent::PdfPage(page) => {
                Shows::Picture(format!("{}#page={}", page.pdf_path, page.page_number))
            }
            SlideContent::Title(title) => Shows::Words(lines(&title.title_text)),
            SlideContent::SingleLanguageMainContent(main) => {
                Shows::Words(lines(&main.clone().main_text()))
            }
            SlideContent::MultiLanguageMainContent(multi) => Shows::Words(
                multi
                    .main_text_list
                    .iter()
                    .flat_map(|text| lines(text))
                    .collect(),
            ),
            // Including the rows the notation repeats: they are the words the
            // slide shows, whether they are set as text or engraved.
            SlideContent::Complex(complex) => Shows::Words(
                complex
                    .rows
                    .iter()
                    .filter(|row| !row.is_notation())
                    .flat_map(|row| lines(&row.content))
                    .collect(),
            ),
        }
    }

    /// Whether this slide shows everything `other` shows.
    ///
    /// Deliberately not equality: the whole point of a separate stream division
    /// is that one of its slides may hold several of the projection's. What it
    /// may never do is hold *part* of one, which is what
    /// [`reconcile_max_lines`] guarantees.
    fn contains(&self, other: &Shows) -> bool {
        match (self, other) {
            (Shows::Nothing, Shows::Nothing) => true,
            (Shows::Picture(here), Shows::Picture(there)) => here == there,
            // A slide with nothing readable on it matches nothing: it would
            // otherwise match every slide there is.
            (Shows::Words(_), Shows::Words(there)) if there.is_empty() => false,
            (Shows::Words(here), Shows::Words(there)) => {
                there.iter().all(|line| here.contains(line))
            }
            _ => false,
        }
    }
}

/// The lines of a piece of text, trimmed, without the blank ones.
fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(text: &str) -> Slide {
        Slide::new_content_slide(text.to_string(), None, None)
    }

    // ── The line wrap ───────────────────────────────────────────────────────

    /// A viewer's slide holds a whole number of the projection's, so that a
    /// slide change on the wall always lands on a slide boundary on the phones.
    #[test]
    fn the_streams_wrap_is_a_multiple_of_the_projections() {
        assert_eq!(reconcile_max_lines(Some(2), Some(4)), Some(4));
        assert_eq!(reconcile_max_lines(Some(2), Some(6)), Some(6));
        assert_eq!(reconcile_max_lines(Some(3), Some(3)), Some(3));
    }

    /// A value between two multiples is rounded up rather than refused: the
    /// user asked for roughly that much, and the nearest amount that works is a
    /// better answer than none at all.
    #[test]
    fn a_wrap_between_two_multiples_is_rounded_up() {
        assert_eq!(reconcile_max_lines(Some(2), Some(5)), Some(6));
        assert_eq!(reconcile_max_lines(Some(4), Some(1)), Some(4));
        assert_eq!(reconcile_max_lines(Some(4), Some(7)), Some(8));
    }

    /// The whole verse always contains whatever the projection is showing, so
    /// it is safe whatever the projection does.
    #[test]
    fn no_wrap_at_all_is_always_allowed() {
        assert_eq!(reconcile_max_lines(Some(2), None), None);
        assert_eq!(reconcile_max_lines(None, None), None);
    }

    /// A projection that shows whole verses gives nothing to be a multiple of,
    /// and any wrap the stream chose would cut one of its slides in half.
    #[test]
    fn a_wrap_under_a_projection_without_one_is_dropped() {
        assert_eq!(reconcile_max_lines(None, Some(4)), None);
        assert_eq!(reconcile_max_lines(Some(0), Some(4)), None);
    }

    /// Everything but the wrap is the stream's own business.
    #[test]
    fn the_rest_of_the_settings_are_taken_as_asked_for() {
        let projection = SlideSettings {
            max_lines: Some(2),
            title_slide: true,
            show_spoiler: true,
            ..SlideSettings::default()
        };
        let wanted = SlideSettings {
            max_lines: Some(5),
            title_slide: false,
            show_spoiler: false,
            ..SlideSettings::default()
        };

        let settled = stream_slide_settings(&projection, &wanted);

        assert_eq!(settled.max_lines, Some(6), "rounded to a whole multiple");
        assert!(!settled.title_slide, "and the rest left alone");
        assert!(!settled.show_spoiler);
    }

    // ── The mapping ─────────────────────────────────────────────────────────

    /// The ordinary case: the wall goes two lines at a time, the phones four.
    /// Two slide changes on the wall are one on the phones.
    #[test]
    fn two_projection_slides_to_one_stream_slide() {
        let projection = vec![content("one\ntwo"), content("three\nfour")];
        let stream = vec![content("one\ntwo\nthree\nfour")];

        assert_eq!(map_slides(&projection, &stream), vec![0, 0]);
    }

    /// Four verses on the wall, two on the phones.
    #[test]
    fn a_longer_song_lines_up_all_the_way_through() {
        let projection = vec![
            content("a1\na2"),
            content("a3\na4"),
            content("b1\nb2"),
            content("b3\nb4"),
        ];
        let stream = vec![content("a1\na2\na3\na4"), content("b1\nb2\nb3\nb4")];

        assert_eq!(map_slides(&projection, &stream), vec![0, 0, 1, 1]);
    }

    /// Where the two divide the song the same way, every slide is its own.
    #[test]
    fn an_identical_division_maps_one_to_one() {
        let projection = vec![content("one"), content("two"), content("three")];
        let stream = projection.clone();

        assert_eq!(map_slides(&projection, &stream), vec![0, 1, 2]);
    }

    /// A refrain is the same words three times over. Matching on content alone
    /// would send every viewer back to the first chorus each time the wall
    /// reached a later one — so the walk only ever moves forward.
    #[test]
    fn a_repeated_refrain_does_not_send_the_viewers_backwards() {
        let projection = vec![
            content("verse one"),
            content("refrain"),
            content("verse two"),
            content("refrain"),
        ];
        let stream = projection.clone();

        assert_eq!(map_slides(&projection, &stream), vec![0, 1, 2, 3]);
    }

    /// A title slide the stream was set up without has nothing to map to. A
    /// viewer stays where the walk had got to rather than being thrown to the
    /// end of the song.
    #[test]
    fn a_slide_with_no_counterpart_leaves_the_viewers_where_they_are() {
        let projection = vec![
            Slide::new_title_slide("A Christian Home".to_string(), None),
            content("one\ntwo"),
            content("three\nfour"),
        ];
        let stream = vec![content("one\ntwo\nthree\nfour")];

        assert_eq!(map_slides(&projection, &stream), vec![0, 0, 0]);
    }

    /// Pages of a document cannot be divided differently by any slide setting,
    /// so they line up one to one and are told apart by their page number.
    #[test]
    fn the_pages_of_a_document_line_up_one_to_one() {
        let pages: Vec<Slide> = (1..=3)
            .map(|page| Slide::new_pdf_page_slide("handout.pdf".to_string(), page))
            .collect();

        assert_eq!(map_slides(&pages, &pages.clone()), vec![0, 1, 2]);
    }

    /// A stream that shows the same slides needs no map, and asking for one
    /// over nothing must not panic.
    #[test]
    fn a_stream_with_no_slides_maps_everything_to_the_first() {
        let projection = vec![content("one"), content("two")];

        assert_eq!(map_slides(&projection, &[]), vec![0, 0]);
    }

    /// The gap between two elements of a service is a slide of its own on both
    /// sides, and matches the other gap rather than the last verse.
    #[test]
    fn an_empty_slide_matches_the_other_empty_slide() {
        let projection = vec![content("one\ntwo"), content("three\nfour"), Slide::new_empty_slide(false)];
        let stream = vec![content("one\ntwo\nthree\nfour"), Slide::new_empty_slide(false)];

        assert_eq!(map_slides(&projection, &stream), vec![0, 0, 1]);
    }

    /// Nothing may map outside the stream: a viewer looking up a slide that is
    /// not there sees nothing at all.
    #[test]
    fn every_slide_maps_inside_the_stream() {
        let projection: Vec<Slide> = (1..=6).map(|n| content(&format!("line {n}"))).collect();
        let stream = vec![content("line 1"), content("line 2")];

        let map = map_slides(&projection, &stream);

        assert!(map.iter().all(|&index| index < stream.len()), "got: {map:?}");
    }

    /// The map never goes backwards, whatever the song does. Anything else
    /// would show a congregation a verse they have already sung.
    #[test]
    fn the_map_never_goes_backwards() {
        let projection = vec![
            content("a"),
            content("b"),
            content("a"),
            content("c"),
            content("b"),
        ];
        let stream = vec![content("a\nb"), content("a\nc"), content("b")];

        let map = map_slides(&projection, &stream);

        assert!(
            map.windows(2).all(|pair| pair[0] <= pair[1]),
            "got: {map:?}"
        );
    }
}
