//! The presenter console as a browser on the network sees it.
//!
//! There is no second console here. This is the adapter around the one in
//! [`crate::components::presenter_console_components`]: it puts the contexts
//! that console expects in front of it, and it keeps the presentation it works
//! on in step with the program over the bridge in
//! [`crate::logic::remote_console`].
//!
//! The console itself cannot tell the difference, and that is the point. What
//! it *can* tell is which host it is running under — it is given
//! [`ConsoleHost::Remote`], which is how it knows not to take the machine's
//! audio and not to write the machine's settings file.

use dioxus::prelude::*;

use crate::logic::console_host::ConsoleHost;
use crate::logic::remote_console::{self, Agreement, ConsoleCommand};
use crate::logic::settings::Settings;
use crate::logic::states::{RunningPresentation, RuntimeInformation};

use super::presenter_console_components::PresenterConsolePage;

/// The root of a remote console's `VirtualDom`.
///
/// One of these per connected browser. Built on `dioxus_liveview`'s own
/// thread, which is why everything it needs is made here rather than handed
/// in: a `Signal` belongs to the runtime that made it.
#[component]
pub fn RemoteConsoleRoot() -> Element {
    // What the console reads and writes. In the main window this is *the*
    // presentation; here it is this connection's copy of it, and the two
    // loops below are what make it a copy rather than a fork.
    let mut presentations: Signal<Vec<RunningPresentation>> =
        use_context_provider(|| Signal::new(Vec::new()));

    // The console is one of three consoles, and this is which.
    use_context_provider(|| ConsoleHost::Remote);

    // A snapshot, not the program's own signal — see
    // [`ConsoleHost::persists_settings`]. What the remote operator changes
    // about their own view holds while their page is open and is then
    // forgotten; nothing here reaches the settings file.
    let settings: Signal<Settings> = use_signal(Settings::load);
    use_context_provider(|| settings);

    // The program's own language. The remote page is the operator's console,
    // not a visitor's page, and it says what the machine says.
    use_context_provider(|| RuntimeInformation {
        language: rust_i18n::locale().to_string(),
    });

    // What the program and this console last agreed the presentation to be.
    // Both loops below write it, and nothing else reads it: it is the one
    // thing keeping a change that travels in both directions from being
    // answered by the other end. See [`Agreement`] for what goes wrong when it
    // only remembers one of them.
    let mut agreement: Signal<Agreement> = use_signal(Agreement::default);

    // program → this console.
    use_future(move || async move {
        let mut states = remote_console::subscribe();
        loop {
            // `borrow_and_update` marks this value seen, so the wait below is
            // for the *next* change and not for this one again.
            let current = states.borrow_and_update().clone();

            // Cloned out before the agreement is written, so that no read of
            // one signal is open while the other is being written.
            let here = presentations.peek().first().cloned();

            if agreement
                .write()
                .program_published(current.as_ref(), here.as_ref())
            {
                presentations.set(current.into_iter().collect());
            }

            // The sender lives as long as the program does; an error here
            // means the program is going away, and so is this console.
            if states.changed().await.is_err() {
                return;
            }
        }
    });

    // this console → program.
    use_effect(move || {
        let here = presentations.read().first().cloned();
        let Some(here) = here else {
            // Emptied because the program said so, or because the operator
            // ended the presentation — in which case the `Quit` has already
            // been sent by the console itself.
            return;
        };

        // Only what the program has not already been told. Without this the
        // update that arrived from the program would be sent straight back,
        // and the two ends would keep answering each other.
        if !agreement.write().console_moved(&here) {
            return;
        }

        remote_console::send(ConsoleCommand::Update(Box::new(here)));
    });


    rsx! {
        PresenterConsolePage {}
    }
}
