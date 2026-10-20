//! The windows of a running service, drawn as part of the main window's tree.
//!
//! Dioxus 0.8 has one `VirtualDom` for the whole program. A window is no
//! longer a `VirtualDom` of its own that is handed to the desktop; it is a
//! [`Window`] component, and what it contains is rendered into a native window
//! instead of into the place it stands. The consequence that matters here is
//! that the contexts the windows used to be given by hand
//! (`with_root_context(settings)` and the like) are now simply inherited from
//! [`App`](crate::App), and that opening a window is no longer a call but a
//! state: a request in [`OpenWindows`], which [`WindowHost`] turns into a
//! `Window`.
//!
//! Closing is the same in reverse. A page that wants its window gone calls
//! `window().close()` as before; the desktop hands the request to the
//! component, [`WindowHost`] drops the request in `onclose`, and the native
//! window goes with the component.

use crate::components::presentation_components::PresentationPage;
use crate::components::presenter_console_components::PresenterConsolePage;
use crate::components::selection_components::ShownView;
use crate::logic::console_host::ConsoleHost;
use crate::logic::live_windows::WindowTarget;
use dioxus::desktop::tao::window::WindowBuilder;
use dioxus::desktop::{Window, WindowConfig};
use dioxus::prelude::*;

/// What a requested window shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WindowPage {
    /// One of the settings' views, by its position — see [`ShownView`].
    View(usize),
    /// The presenter console, in a window of its own.
    Console,
}

/// A window that has been asked for and not yet closed.
#[derive(Clone)]
pub struct WindowRequest {
    id: u64,
    pub target: WindowTarget,
    pub page: WindowPage,
    pub builder: WindowBuilder,
    /// The screen it was *put* on, as a [`screen_key`](crate::logic::screens::screen_key).
    /// Told to [`live_windows::remember`](crate::logic::live_windows::remember)
    /// by the window itself, as against the screen it happens to be over now.
    pub on: Option<String>,
}

impl WindowRequest {
    pub fn new(
        target: WindowTarget,
        page: WindowPage,
        builder: WindowBuilder,
        on: Option<String>,
    ) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            target,
            page,
            builder,
            on,
        }
    }
}

/// The windows that should be open. Provided by [`App`](crate::App).
#[derive(Clone, Copy)]
pub struct OpenWindows(pub Signal<Vec<WindowRequest>>);

impl OpenWindows {
    /// Asks for a window. One that is already open for the same
    /// [`WindowTarget`] is replaced, which is what a second start of the
    /// service during the first one's last seconds should mean.
    pub fn open(mut self, request: WindowRequest) {
        let mut windows = self.0.write();
        windows.retain(|open| open.target != request.target);
        windows.push(request);
    }
}

/// Draws a [`Window`] for every request in [`OpenWindows`]. Mounted once, at
/// the root, and never unmounted.
#[component]
pub fn WindowHost() -> Element {
    let mut windows = use_context::<OpenWindows>().0;

    rsx! {
        for request in windows.read().iter().cloned() {
            Window {
                key: "{request.id}",
                config: WindowConfig::new()
                    .with_menu(None)
                    .with_disable_drag_drop_handler(true)
                    .with_window(request.builder.clone()),
                onclose: {
                    let id = request.id;
                    move |_| windows.write().retain(|open| open.id != id)
                },
                match request.page {
                    WindowPage::View(index) => rsx! {
                        ViewWindow { index, target: request.target, on: request.on.clone() }
                    },
                    WindowPage::Console => rsx! {
                        ConsoleWindow { target: request.target, on: request.on.clone() }
                    },
                }
            }
        }
    }
}

/// Puts the stylesheets and scripts every one of these windows needs into *its*
/// head.
///
/// `document::Link` and `document::Script` look like they do this, and in a
/// window of the old kind they did. In Dioxus 0.8 they remember the hrefs they
/// have inserted in a context of the whole `VirtualDom`, not of the window —
/// so everything [`App`](crate::App) registered for the main window counts as
/// done, the window's own `Link` is skipped, and the window comes up without
/// its stylesheet: no background, the wrong font, the web view's default
/// margin. The window's own document is asked directly instead, which has no
/// such memory.
fn head_for_this_window() {
    let document = use_hook(dioxus::document::document);
    // After the render, as the document's own `create_link` does: the web view
    // is not there to be told anything until then.
    use_effect(move || {
        for href in [
            crate::MAIN_CSS,
            crate::components::presentation_components::PRESENTATION_CSS,
            crate::components::presenter_console_components::PRESENTER_CONSOLE_CSS,
        ] {
            document.create_head_element(
                "link",
                &[("rel", "stylesheet".to_string()), ("href", href.to_string())],
                None,
            );
        }
        for src in [
            crate::components::presentation_components::PRESENTATION_JS,
            crate::components::presentation_components::MORPH_JS,
            crate::logic::pdf::PDF_VIEWER_JS,
        ] {
            document.create_head_element("script", &[("src", src.to_string())], None);
        }
    });
}

/// Remembers the window this is drawn in, so that it can be moved later — see
/// [`crate::logic::live_windows`] and D5 of spec 0007.
fn remember_this_window(target: WindowTarget, on: Option<String>) {
    use_hook(|| crate::logic::live_windows::remember(target, &dioxus::desktop::window(), on));
}

#[component]
fn ViewWindow(index: usize, target: WindowTarget, on: Option<String>) -> Element {
    // Which of the settings' views this window is drawing.
    use_context_provider(|| ShownView(index));
    head_for_this_window();
    remember_this_window(target, on);
    rsx! { PresentationPage {} }
}

#[component]
fn ConsoleWindow(target: WindowTarget, on: Option<String>) -> Element {
    // Which console this is. Without it the console would take itself for the
    // routed one and navigate a router that is not there. See
    // [`crate::logic::console_host`].
    use_context_provider(|| ConsoleHost::SeparateWindow);
    head_for_this_window();
    remember_this_window(target, on);
    rsx! { PresenterConsolePage {} }
}
