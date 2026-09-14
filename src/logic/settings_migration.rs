//! Reading a settings file written by an older Cantara.
//!
//! # Why this is the test that matters most
//!
//! A presentation that fails can be restarted. Somebody notices, presses the
//! key again, and the service goes on. **A configuration that has been
//! rewritten wrongly is gone** — the file is overwritten with the new shape on
//! the next save, and whatever the user had set up is not recoverable from it.
//!
//! [`Settings::bring_up_to_date`] runs on every start, for every user, and
//! [`Settings::ensure_views`] inside it rewrites the shape of the whole output
//! configuration. That happened once, to everybody, when they upgraded to 3.0.
//! There is no second chance at it and no way to notice it went wrong except by
//! opening the settings and finding them changed.
//!
//! So this is stage 2 of `docs/specs/0004-testing-playwright.md`, and it comes
//! before the network tests and before Playwright despite being much smaller
//! than either.
//!
//! # What the fixtures are
//!
//! `fixtures/settings/` holds one document per shape worth reading. They are
//! **built**, not collected — nobody's real settings file is in this
//! repository, and one would carry paths and passwords that do not belong in
//! it. Each is the shape a version actually wrote, with a configuration in it
//! that somebody would recognise: repositories with names, a second design, a
//! screen chosen, a stream set up.
//!
//! A real file is welcome here. Drop it in with the private parts replaced and
//! [`every_stored_settings_file_survives_being_read`] covers it, because that
//! test walks the directory rather than naming the files.
//!
//! # What is asserted, and what cannot be
//!
//! Two kinds of thing:
//!
//! * **Invariants that hold for every document**, checked by walking the
//!   directory. These are the ones that keep working when somebody adds a
//!   fixture.
//! * **What a particular version's configuration should become**, named one at
//!   a time. Those cannot be generic: the whole question is whether *this*
//!   file's stream design ends up on *that* view.
//!
//! What no test here can say is whether the migration matches what the user
//! meant. It can only say that what the file described still comes out the
//! other side. That is the difference between a migration being correct and a
//! migration being lossless, and only the second is testable.

#![cfg(test)]

use crate::logic::settings::{RepositoryType, Settings, ViewOutput};

/// Where the fixtures live.
fn settings_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("settings")
}

/// Every fixture, as (file name, contents).
fn every_stored_file() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = std::fs::read_dir(settings_dir())
        .expect("the settings fixtures are in the repository")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .map(|path| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{name} could not be read: {error}"));
            (name, content)
        })
        .collect();

    // Sorted, so that a failure names the same file in the same place on every
    // machine.
    files.sort_by(|(a, _), (b, _)| a.cmp(b));

    assert!(
        !files.is_empty(),
        "there are no settings fixtures, so this whole module asserts nothing"
    );
    files
}

/// The one that would be a disaster: a stored document that cannot be read at
/// all falls back to the defaults, and the defaults have no repositories in
/// them.
///
/// Every fixture has to survive being read. A version whose file stopped
/// parsing would take every repository, design and font its users had
/// configured with it, silently, on the first start after the update.
#[test]
fn every_stored_settings_file_survives_being_read() {
    for (name, content) in every_stored_file() {
        let settings = Settings::from_stored(&content)
            .unwrap_or_else(|| panic!("{name} could not be read at all, so every user of that version would lose their whole configuration"));

        assert!(
            !settings.repositories.is_empty(),
            "{name} came back with no repositories, which is what a fall back to \
             the defaults looks like"
        );
        assert!(
            !settings.presentation_designs.is_empty(),
            "{name} came back with no designs"
        );
    }
}

/// Every document comes out describing a service that can actually be given.
///
/// The invariants a running Cantara leans on without checking: there is
/// somewhere to project, the reference view is one of the views, and every
/// index names something that exists. An index past the end is the failure
/// mode this program has had before, and it surfaces during a service.
#[test]
fn every_stored_settings_file_describes_a_usable_service() {
    for (name, content) in every_stored_file() {
        let settings = Settings::from_stored(&content).expect("reads");

        assert!(
            !settings.views.is_empty(),
            "{name} has nothing to project onto"
        );
        assert!(
            settings.reference_view_index < settings.views.len(),
            "{name}: the reference view is at {} of {} views",
            settings.reference_view_index,
            settings.views.len()
        );
        assert!(
            settings.default_design_index < settings.presentation_designs.len(),
            "{name}: the default design names one that is not there"
        );
        assert!(
            settings.default_slide_settings_index < settings.song_slide_settings.len(),
            "{name}: the default slide division names one that is not there"
        );

        for view in &settings.views {
            if let Some(index) = view.design_index {
                assert!(
                    index < settings.presentation_designs.len(),
                    "{name}: view {:?} is set to design {index}, which is not there",
                    view.name
                );
            }
            if let Some(index) = view.slide_settings_index {
                assert!(
                    index < settings.song_slide_settings.len(),
                    "{name}: view {:?} is set to division {index}, which is not there",
                    view.name
                );
            }
        }
    }
}

/// Reading a document twice gives the same configuration as reading it once.
///
/// `bring_up_to_date` runs on **every** start, including starts on a file it
/// has already migrated. A step that is not safe to repeat would add a view
/// per start, or reset a choice the user made after the first migration — and
/// it would do that quietly, so the person it happened to would find their
/// settings drifting with no event to blame it on.
#[test]
fn reading_a_stored_file_twice_gives_the_same_configuration() {
    for (name, content) in every_stored_file() {
        let once = Settings::from_stored(&content).expect("reads");
        let written = serde_json::to_string(&once).expect("serialises");
        let twice = Settings::from_stored(&written).expect("reads back");

        assert_eq!(
            serde_json::to_value(&once).expect("comparable"),
            serde_json::to_value(&twice).expect("comparable"),
            "{name} changes every time it is read, so the user's settings drift \
             with each start"
        );
    }
}

/// Nothing the user set up is dropped on the way through.
///
/// Deliberately about the things a person would notice missing rather than
/// about the shape of the document: their libraries, their designs, their
/// slide divisions — by name, so that a migration which kept the *count* while
/// replacing the contents does not pass.
#[test]
fn a_users_own_configuration_comes_through_untouched() {
    for (name, content) in every_stored_file() {
        let stored: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
        let settings = Settings::from_stored(&content).expect("reads");

        let names_in = |key: &str| -> Vec<String> {
            stored
                .get(key)
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("name"))
                        .filter_map(|value| value.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };

        assert_eq!(
            settings
                .repositories
                .iter()
                .map(|repo| repo.name.clone())
                .collect::<Vec<_>>(),
            names_in("repositories"),
            "{name} lost or renamed a repository"
        );
        assert_eq!(
            settings
                .presentation_designs
                .iter()
                .map(|design| design.name.clone())
                .collect::<Vec<_>>(),
            names_in("presentation_designs"),
            "{name} lost or renamed a design"
        );
    }
}

// ── What a particular version's configuration becomes ───────────────────

/// The contents of a named fixture.
fn fixture(name: &str) -> String {
    std::fs::read_to_string(settings_dir().join(name))
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// A file from before views existed gets the two outputs Cantara had, and
/// they behave as the old fields did.
///
/// The whole of `ensure_views` against a real document rather than against
/// `Settings::default()`: the screen the user had chosen is on the projection,
/// the projection follows the default design instead of being pinned to
/// whichever one was default at the moment of migration, and the stream is
/// *off*.
///
/// That last one is the migration's one genuinely dangerous choice. Whether
/// streaming is on has never been remembered between sessions, so a migration
/// that enabled the stream view would begin putting services on the network
/// for people who had never switched it on and would have no reason to look.
#[test]
fn a_file_from_before_views_gets_the_two_outputs_it_always_had() {
    let settings = Settings::from_stored(&fixture("0.2-before-streaming.json")).expect("reads");

    assert_eq!(settings.views.len(), 2, "the projection and the stream");

    let projection = &settings.views[0];
    assert_eq!(
        projection.output,
        ViewOutput::Screen {
            monitor_name: Some("HDMI-2".to_string())
        },
        "the screen the user had chosen is not the one their projection uses"
    );
    assert!(projection.enabled, "the projection does not project");
    assert_eq!(
        projection.design_index, None,
        "the projection was pinned to a design instead of following the default, \
         so changing the default would silently stop reaching the wall"
    );
    assert_eq!(settings.reference_view_index, 0);

    let stream = &settings.views[1];
    assert!(
        matches!(stream.output, ViewOutput::Network { .. }),
        "the second view is not the stream"
    );
    assert!(
        !stream.enabled,
        "the migration switched streaming on for somebody who never asked for it"
    );
}

/// A 0.3 file's stream settings end up on the stream's view.
///
/// A lighter design for the phones and a shorter slide division are things
/// somebody chose deliberately. Losing them in the migration would be losing a
/// decision, and it would look like the stream had simply always been that way.
#[test]
fn what_was_set_up_for_the_phones_ends_up_on_the_stream_view() {
    let settings = Settings::from_stored(&fixture("0.3-with-a-stream.json")).expect("reads");

    let stream = settings
        .views
        .iter()
        .find(|view| matches!(view.output, ViewOutput::Network { .. }))
        .expect("there is a stream view");

    assert_eq!(
        stream.design_index,
        Some(1),
        "the design chosen for the phones did not reach their view"
    );
    assert_eq!(
        stream.slide_settings_index,
        Some(1),
        "the slide division chosen for the phones did not reach their view"
    );

    // The port and the passwords are not part of a view, and stay where they
    // were. Asserted because they are easy to lose sight of while the design
    // and the division are being moved out of the same section.
    assert_eq!(settings.stream.port, 9000);
    assert_eq!(settings.stream.password, "gnade");
    assert_eq!(settings.stream.remote_password, "leitung");
}

/// A repository stored as a GitHub zip URL becomes a GitHub repository.
///
/// The other migration in `bring_up_to_date`, and the fixture carries one
/// because a step that no document exercises is a step nothing is testing.
#[test]
fn a_github_zip_url_becomes_a_github_repository() {
    let settings = Settings::from_stored(&fixture("0.3-with-a-stream.json")).expect("reads");

    let repo = settings
        .repositories
        .iter()
        .find(|repo| repo.name == "cantara-songrepo")
        .expect("the fixture carries one");

    assert_eq!(
        repo.repository_type,
        RepositoryType::GitHub {
            owner: "reckel-jm".to_string(),
            repo: "cantara-songrepo".to_string(),
            token: None,
        },
        "a zip URL was left as a download rather than being recognised"
    );
}

/// A file that already has views is left exactly as it is.
///
/// A migration, not a repair. Somebody who has arranged three views — and
/// deleted one they did not want — must not find it back on the next start,
/// and a monitor design set on a stage view must still be set on it.
#[test]
fn a_configuration_that_already_has_views_is_not_migrated_again() {
    let content = fixture("3.0-monitor-views.json");
    let stored: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
    let settings = Settings::from_stored(&content).expect("reads");

    assert_eq!(
        serde_json::to_value(&settings.views).expect("comparable"),
        stored["views"],
        "the views were rewritten by a migration that had already been run"
    );

    let stage = &settings.views[1];
    assert_eq!(stage.design_index, Some(1));
    assert!(
        settings.presentation_designs[1]
            .presentation_design_settings
            .monitor()
            .is_some(),
        "the design the stage view is set to is no longer a monitor design"
    );
}

/// A first start — no file at all — comes up able to project.
///
/// The defaults on their own have **no views in them**, so a start that
/// skipped the fixups would come up with nothing to project onto. Worth its
/// own test because it is the one path where nothing is read and so nothing
/// above covers it.
#[test]
fn a_first_start_comes_up_with_somewhere_to_project() {
    // The real path, not an imitation of it: this is what `load` calls when
    // there is no file and nothing in local storage.
    let settings = Settings::started_fresh();

    assert!(
        !settings.views.is_empty(),
        "a fresh installation has nowhere to put the presentation"
    );
    assert!(
        settings.reference_view().is_some(),
        "a fresh installation has no reference view"
    );
}

/// A damaged file is not read, rather than half-read.
///
/// The distinction `load` depends on: something it cannot understand gives
/// `None`, and the caller falls back to a working configuration. A partial
/// read would be worse than either — a configuration that is *nearly* the
/// user's is the one nobody notices is wrong.
#[test]
fn a_document_that_cannot_be_understood_is_refused() {
    for broken in [
        "",
        "not json at all",
        "{\"repositories\": ",
        "[]",
        "\"a string\"",
        // Valid JSON, but not a settings document — a file that has been
        // truncated to nothing but its braces, which is what an interrupted
        // save leaves behind.
        "{}",
    ] {
        assert!(
            Settings::from_stored(broken).is_none(),
            "{broken:?} was read as a configuration"
        );
    }
}
