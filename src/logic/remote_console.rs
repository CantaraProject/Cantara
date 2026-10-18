//! The bridge between the program and a presenter console running in a
//! browser on the network.
//!
//! # Why there is a bridge at all
//!
//! A console in its own desktop window is handed the very signal the rest of
//! the program uses — `with_root_context(*running_presentations)` in
//! [`crate::components::selection_components`]. That works because every
//! desktop window's `VirtualDom` is polled on the same thread, and a `Signal`
//! belongs to the thread that made it.
//!
//! A remote console does not. `dioxus_liveview` builds each connection's
//! `VirtualDom` with `spawn_pinned`, on a thread out of its own pool, so the
//! signal cannot go with it. The remote console gets a signal of its own and
//! this module keeps the two in step: the presentation as it stands goes out
//! over a [`watch`] channel, and what the remote console makes of it comes
//! back over an [`mpsc`] one.
//!
//! # Why whole presentations rather than "next slide"
//!
//! Because that is what a console produces. Every control in
//! [`crate::components::presenter_console_components`] — the arrows, the jump
//! sidebar, the black screen, the video transport — works by writing to its
//! own `RunningPresentation`, and the desktop's two windows already keep in
//! step by copying that value between them. Sending intents instead would mean
//! rewriting every one of those call sites for the remote case, which is the
//! opposite of running the same console.
//!
//! It is also no wider a door. The browser at the other end does not speak
//! this protocol and cannot reach it: with LiveView it sends *events* — a
//! click, a key — and the console component turns them into the same writes a
//! click in the window would. Nothing crosses the network but DOM patches one
//! way and events the other. What travels here has never left the process.
//!
//! The one thing that is not a state of the presentation is ending it, which
//! is why [`ConsoleCommand`] has a second variant.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use tokio::sync::{mpsc, watch};

use super::states::RunningPresentation;

/// What a remote console has to say.
///
/// Not [`Debug`]: a [`RunningPresentation`] is not, and printing a whole
/// service would be no use if it were.
#[derive(Clone)]
pub enum ConsoleCommand {
    /// The presentation as the remote console now has it — after a slide
    /// change, a jump, the black screen, a video control.
    ///
    /// Boxed because a [`RunningPresentation`] is a large value and this enum
    /// is otherwise a word wide.
    Update(Box<RunningPresentation>),

    /// End the presentation, for the room and for everyone watching.
    ///
    /// Not an [`Update`](Self::Update): there is no presentation left to
    /// describe afterwards.
    Quit,
}

/// The presentation as the program has it, for whoever is connected.
///
/// `None` between services: the remote page stays open and says that nothing
/// is running, exactly as the stream viewer's page does.
type StateChannel = (
    watch::Sender<Option<RunningPresentation>>,
    watch::Receiver<Option<RunningPresentation>>,
);

fn state() -> &'static StateChannel {
    static STATE: OnceLock<StateChannel> = OnceLock::new();
    STATE.get_or_init(|| watch::channel(None))
}

type CommandChannel = (
    mpsc::UnboundedSender<ConsoleCommand>,
    Mutex<Option<mpsc::UnboundedReceiver<ConsoleCommand>>>,
);

fn commands() -> &'static CommandChannel {
    static COMMANDS: OnceLock<CommandChannel> = OnceLock::new();
    COMMANDS.get_or_init(|| {
        let (sender, receiver) = mpsc::unbounded_channel();
        (sender, Mutex::new(Some(receiver)))
    })
}

/// How many browsers are driving a console right now.
static CONNECTED: AtomicUsize = AtomicUsize::new(0);

/// Tells every connected console where the presentation now stands.
///
/// Cheap when nobody is connected — a `watch` send with no receivers is a
/// store and nothing else — so the caller may publish on every change without
/// asking whether the feature is even switched on.
pub fn publish(presentation: Option<RunningPresentation>) {
    let channel = state();
    // Held rather than compared away: the console's own adapter decides what
    // counts as a change, using the same `eq_ignoring_scroll` the desktop
    // windows use. Comparing here as well would only mean two answers to the
    // same question.
    let _ = channel.0.send(presentation);
}

/// A view of the presentation for one console to follow.
pub fn subscribe() -> watch::Receiver<Option<RunningPresentation>> {
    state().1.clone()
}

/// Sends what a remote console did back to the program.
pub fn send(command: ConsoleCommand) {
    let _ = commands().0.send(command);
}

/// The receiving end of the command channel, once.
///
/// For the helper process, which waits on it. The program itself uses
/// [`drain`] instead — see there for why it does not wait.
pub fn take_commands() -> Option<mpsc::UnboundedReceiver<ConsoleCommand>> {
    commands().1.lock().ok().and_then(|mut held| held.take())
}

/// Applies everything a remote console has done since this was last called,
/// and says whether any of it changed anything.
///
/// Called from a polling loop rather than woken by the channel, and that is
/// the point of it. Awaiting the receiver in the main window looked right and
/// did nothing: a message sent from the thread that reads the helper wakes a
/// `Waker`, and whether that reaches a `VirtualDom` driven by a window's event
/// loop is the very question every other cross-window path in this program
/// answers by polling. What it looked like from the pew was a remote console
/// that showed everything and changed nothing.
///
/// A burst — somebody pressing *next* three times while the projection catches
/// up — is drained in one go, so it costs one write to the signal rather than
/// three.
pub fn drain(presentations: &mut Vec<RunningPresentation>) -> bool {
    let Ok(mut held) = commands().1.lock() else {
        return false;
    };
    let Some(receiver) = held.as_mut() else {
        // The helper process took the receiver: this is that process, and
        // there is nothing here for it to apply.
        return false;
    };

    let mut changed = false;
    while let Ok(command) = receiver.try_recv() {
        changed |= apply(command, presentations);
    }
    changed
}

/// Applies what a remote console did to the running presentations.
///
/// Returns whether anything actually changed, so the caller can leave the
/// signal alone when it did not — writing a signal wakes everything that reads
/// it, and a console that reports the same state twice must not redraw the
/// projection.
///
/// The comparison ignores the markdown scroll position for the reason
/// [`RunningPresentation::eq_ignoring_scroll`] gives: it is reported several
/// times a second by whoever is scrolling and is not a command to anybody.
pub fn apply(command: ConsoleCommand, presentations: &mut Vec<RunningPresentation>) -> bool {
    match command {
        ConsoleCommand::Quit => {
            if presentations.is_empty() {
                return false;
            }
            presentations.clear();
            true
        }
        ConsoleCommand::Update(updated) => {
            let Some(current) = presentations.first_mut() else {
                // Nothing is running any more. A command that arrives just
                // after the presentation ended is not an error and is not a
                // reason to start one.
                return false;
            };
            if current.eq_ignoring_scroll(&updated) {
                return false;
            }
            *current = *updated;
            true
        }
    }
}

/// What a remote console and the program last agreed the presentation to be.
///
/// A remote console has to send everything the operator does and send back
/// nothing the program said, or the two ends spend the service answering each
/// other. Both of those are decided here, and both directions write: this is
/// not "the last thing the program said" but "the last state the two ends are
/// known to share".
///
/// Remembering only what the program said is what went wrong before. The
/// program answers a console's change by publishing it, which is no news to
/// the console that made it and so changes nothing on its screen — and a note
/// written only when something changed was therefore left holding the
/// presentation as it stood *before*. The first thing the program ever
/// publishes is the opening slide, which for a song is its title slide, so
/// what stood there was usually that. A jump back onto the title slide from
/// the first verse then looked precisely like a state the program had sent,
/// was dropped as an echo, and the room stayed on the verse. Reloading the
/// page cured it because a new connection agrees afresh, which is exactly how
/// it was reported.
///
/// Both methods below write, and either write on its own would have been
/// enough to cure that jump — which is why the test for it only fails when
/// neither does. They are both kept because they answer different moments: an
/// echo confirms an agreement that the program has acted on, and a send
/// records one it has not been told of yet. Only the second of those closes
/// the round trip, during which the shared signal is written again by
/// every scroll report.
#[derive(Default)]
pub struct Agreement {
    state: Option<RunningPresentation>,
}

impl Agreement {
    /// Takes in what the program has published, and says whether this console
    /// has to be moved to it.
    ///
    /// `here` is what the console has now; the answer is no when the program
    /// is only echoing that back. The agreement is noted either way, which is
    /// the whole point of the type.
    pub fn program_published(
        &mut self,
        theirs: Option<&RunningPresentation>,
        here: Option<&RunningPresentation>,
    ) -> bool {
        self.state = theirs.cloned();

        match (theirs, here) {
            (Some(theirs), Some(here)) => !theirs.eq_ignoring_scroll(here),
            // Nothing running at either end: between services.
            (None, None) => false,
            // A presentation has begun, or has ended.
            _ => true,
        }
    }

    /// Takes in where the console now stands, and says whether that is news to
    /// the program.
    ///
    /// Noted as it is sent rather than when the program answers: the answer
    /// arrives as a publication like any other, and until it does this console
    /// must not offer the same state a second time.
    pub fn console_moved(&mut self, here: &RunningPresentation) -> bool {
        if self
            .state
            .as_ref()
            .is_some_and(|agreed| agreed.eq_ignoring_scroll(here))
        {
            return false;
        }
        self.state = Some(here.clone());
        true
    }
}

/// Counts one connected console for as long as it is held.
///
/// A guard rather than a pair of calls, because the count has to come back
/// down when a connection is dropped — and a connection is most often dropped
/// by a phone going to sleep in the middle of a service, not by anyone tidying
/// up after themselves.
pub struct Connection;

impl Connection {
    pub fn open() -> Self {
        CONNECTED.fetch_add(1, Ordering::Relaxed);
        Connection
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        CONNECTED.fetch_sub(1, Ordering::Relaxed);
    }
}

/// How many browsers have a console open, for the panel with the switch in it.
pub fn connected() -> usize {
    CONNECTED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::states::SlideChapter;
    use cantara_songlib::slides::Slide;

    fn presentation_of(slides: usize) -> RunningPresentation {
        let chapter = SlideChapter::new(
            (0..slides).map(|_| Slide::new_empty_slide(false)).collect(),
            crate::logic::sourcefiles::SourceFile {
                name: "Test".to_string(),
                path: std::path::PathBuf::from("testfiles/Test.song"),
                file_type: crate::logic::sourcefiles::SourceFileType::Song,
                md5_hash: None,
                relative_path: None,
            },
            None,
            None,
        );
        RunningPresentation::new(vec![chapter])
    }

    #[test]
    fn a_command_that_changes_nothing_is_not_a_change() {
        let mut presentations = vec![presentation_of(3)];
        let same = presentations[0].clone();

        assert!(!apply(
            ConsoleCommand::Update(Box::new(same)),
            &mut presentations
        ));
    }

    #[test]
    fn a_slide_change_from_the_remote_console_arrives() {
        let mut presentations = vec![presentation_of(3)];
        let mut moved = presentations[0].clone();
        moved.next_slide();

        assert!(apply(
            ConsoleCommand::Update(Box::new(moved.clone())),
            &mut presentations
        ));
        assert_eq!(presentations[0].position, moved.position);
    }

    /// Scrolling a markdown slide is a report, not a command. It travels with
    /// the state and must not count as a change on its own — see
    /// [`RunningPresentation::eq_ignoring_scroll`].
    #[test]
    fn scrolling_alone_is_not_a_change() {
        let mut presentations = vec![presentation_of(3)];
        let mut scrolled = presentations[0].clone();
        scrolled.markdown_scroll_position = 120.0;

        assert!(!apply(
            ConsoleCommand::Update(Box::new(scrolled)),
            &mut presentations
        ));
    }

    #[test]
    fn quitting_ends_the_presentation() {
        let mut presentations = vec![presentation_of(3)];

        assert!(apply(ConsoleCommand::Quit, &mut presentations));
        assert!(presentations.is_empty());
    }

    /// A command that arrives just after the presentation ended is late, not
    /// wrong: it must not put a presentation back up.
    #[test]
    fn a_late_command_does_not_start_a_presentation() {
        let mut presentations: Vec<RunningPresentation> = vec![];

        assert!(!apply(
            ConsoleCommand::Update(Box::new(presentation_of(2))),
            &mut presentations
        ));
        assert!(presentations.is_empty());
        assert!(!apply(ConsoleCommand::Quit, &mut presentations));
    }

    /// What the program does with a remote console's commands, from the
    /// channel to the presentation — the half that cannot be seen from the
    /// helper's side of the socket.
    #[test]
    fn a_burst_of_commands_is_drained_into_one_change() {
        let mut presentations = vec![presentation_of(4)];

        let mut moved = presentations[0].clone();
        moved.next_slide();
        let mut moved_again = moved.clone();
        moved_again.next_slide();

        send(ConsoleCommand::Update(Box::new(moved)));
        send(ConsoleCommand::Update(Box::new(moved_again.clone())));

        assert!(drain(&mut presentations), "the commands were applied");
        assert_eq!(
            presentations[0].position, moved_again.position,
            "the last one wins, and the projection is written once"
        );

        assert!(
            !drain(&mut presentations),
            "an empty channel changes nothing"
        );
    }

    /// What the program publishes must not come straight back to it.
    #[test]
    fn what_the_program_published_is_not_sent_back() {
        let running = presentation_of(3);
        let mut agreement = Agreement::default();

        assert!(
            agreement.program_published(Some(&running), None),
            "a presentation that has begun reaches the console"
        );
        assert!(!agreement.console_moved(&running));
    }

    /// The regression this type exists for.
    ///
    /// The state the program published last is not forever a state the console
    /// may no longer ask for — and the opening slide of a song is its title
    /// slide, which is why the jump that went nowhere was the jump onto it.
    #[test]
    fn going_back_to_the_opening_slide_still_reaches_the_program() {
        let title = presentation_of(3);
        let mut first_verse = title.clone();
        first_verse.next_slide();

        let mut agreement = Agreement::default();

        // The presentation begins: the program publishes the opening slide.
        assert!(agreement.program_published(Some(&title), None));

        // The operator moves on, and the program is told.
        assert!(agreement.console_moved(&first_verse));

        // The program answers with what it was just told. Nothing changes on
        // the console, and that is not the same as nothing having happened.
        assert!(!agreement.program_published(Some(&first_verse), Some(&first_verse)));

        assert!(
            agreement.console_moved(&title),
            "a jump back onto the title slide has to reach the projection"
        );
    }

    /// Scrolling a markdown slide is a report, not a command — the same rule
    /// [`apply`] follows, applied to what is worth sending at all.
    #[test]
    fn scrolling_is_nothing_to_tell_the_program() {
        let running = presentation_of(3);
        let mut scrolled = running.clone();
        scrolled.markdown_scroll_position = 240.0;

        let mut agreement = Agreement::default();
        assert!(agreement.program_published(Some(&running), None));
        assert!(!agreement.console_moved(&scrolled));
    }

    #[test]
    fn between_services_there_is_nothing_to_agree_on() {
        let mut agreement = Agreement::default();
        assert!(!agreement.program_published(None, None));
    }

    /// The end of a presentation is a change like any other: the page has to
    /// hear about it, or it keeps the ended service on screen with its arrows.
    #[test]
    fn the_end_of_a_presentation_reaches_the_console() {
        let running = presentation_of(2);
        let mut agreement = Agreement::default();

        assert!(agreement.program_published(Some(&running), None));
        assert!(agreement.program_published(None, Some(&running)));
    }

    #[test]
    fn a_connection_counts_only_while_it_is_open() {
        let before = connected();
        {
            let _first = Connection::open();
            let _second = Connection::open();
            assert_eq!(connected(), before + 2);
        }
        assert_eq!(connected(), before);
    }
}
