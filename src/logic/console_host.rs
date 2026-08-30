//! Where a presenter console is being shown.
//!
//! There is one console component. It is reached in four ways — as a route in
//! the main window, as a window of its own on the desktop, as a second browser
//! tab in the web build, and now from a browser somewhere else on the network
//! — and a handful of things it does depend on which of those it is.
//!
//! Those things used to be decided by `#[cfg]`. That worked while the answer
//! followed the build: a desktop build closed a window, a web build talked to
//! another tab. It stops working with the remote console, which is *in the
//! desktop build* and must behave like neither — so the distinction moves from
//! compile time to a value, provided when the console's `VirtualDom` is built.
//!
//! Nothing is provided for the routed console: [`ConsoleHost::current`]
//! answers [`ConsoleHost::MainWindow`] when nobody said otherwise, which is
//! what a route is.

use dioxus::prelude::*;

/// Which console this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleHost {
    /// A page in the main window, reached by the router. Leaving it means
    /// navigating back to the selection.
    #[default]
    MainWindow,

    /// A window of its own, on the desktop. Leaving it means closing that
    /// window.
    SeparateWindow,

    /// A browser on the network, over the remote-control server. Leaving it
    /// means saying so on the page: there is no window to close and no route
    /// to go back to.
    Remote,
}

impl ConsoleHost {
    /// What the console being drawn is hosted by.
    pub fn current() -> Self {
        try_consume_context::<ConsoleHost>().unwrap_or_default()
    }

    /// Whether this console takes the machine's audio while it is open.
    ///
    /// The rule it belongs to is in [`crate::logic::video::claim_audio`]: the
    /// console is where the operator is, so the projection mutes itself while
    /// one is open and the console makes the sound instead.
    ///
    /// A remote console is not where the machine's speakers are. Someone
    /// opening one from the back of the hall would otherwise silence the room
    /// — the projection would mute for a console whose sound comes out of a
    /// tablet nobody can hear.
    pub fn claims_audio(self) -> bool {
        !matches!(self, ConsoleHost::Remote)
    }

    /// Whether what this console changes about itself — the view mode, the
    /// size of the thumbnails — is written to the settings file.
    ///
    /// It is the operator's file, on the operator's machine. A remote console
    /// may be someone else entirely, and their preference for the grid view is
    /// not a change to how Cantara opens next Sunday. Their choices hold for
    /// as long as their page is open and are then forgotten.
    pub fn persists_settings(self) -> bool {
        !matches!(self, ConsoleHost::Remote)
    }

    /// Whether this console is driven over the remote bridge rather than by
    /// the signal the rest of the program shares.
    ///
    /// See [`crate::logic::remote_console`] for why a remote console cannot
    /// simply be handed the signal.
    pub fn is_remote(self) -> bool {
        matches!(self, ConsoleHost::Remote)
    }
}
