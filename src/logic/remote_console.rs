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

        assert!(!drain(&mut presentations), "an empty channel changes nothing");
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
