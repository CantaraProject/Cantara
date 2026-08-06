//! How large the main window was when it was last closed.
//!
//! Dioxus has no window-state facility of its own. The plugin under
//! `DioxusLabs/platform-apis/plugins/window-state` is `tauri-plugin-window-state`:
//! it registers with a `tauri::Builder` and acts on a `tauri::Window`. Cantara
//! is a Dioxus desktop application built straight on wry and tao and has
//! neither of those, so the same behaviour is kept here instead — a few dozen
//! lines, and no second UI runtime to carry for them.
//!
//! The size is deliberately *not* part of [`Settings`](crate::logic::settings::Settings).
//! It changes with every frame of a drag on the window edge, and the settings
//! live in a signal that the whole program reads — writing there would redraw
//! everything while the window is being resized. This is a small file of its
//! own, beside the settings.
//!
//! Only the size and whether the window was maximised are kept, not where it
//! stood. A remembered position is a promise the next session may not be able
//! to keep: a window put back onto a screen that has since been unplugged is
//! simply invisible, with nothing on screen to say why.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The geometry of the main window, as it is written to disk.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct WindowState {
    /// Width in logical pixels — the unit the window was asked for, so that a
    /// screen with a different scaling does not change what "the same size"
    /// means between two sessions.
    pub width: f64,
    /// Height in logical pixels.
    pub height: f64,
    /// Whether the window filled the screen.
    #[serde(default)]
    pub maximized: bool,
}

/// The smallest window worth restoring.
///
/// A minimised window reports a size of nothing at all on Windows, and there
/// is no reason to hand anyone back a window they could not use.
const MIN_EDGE: f64 = 200.0;

/// How often the size may be written while the window is being dragged.
///
/// The file is written properly when the window closes; this only bounds how
/// much is lost if the program never gets that far.
const WRITE_INTERVAL: Duration = Duration::from_secs(2);

/// The state as it currently stands, waiting to be written.
static PENDING: Mutex<Option<WindowState>> = Mutex::new(None);

/// When the file was last written, so a drag does not write it per frame.
static LAST_WRITE: Mutex<Option<Instant>> = Mutex::new(None);

impl WindowState {
    /// Whether this is a window anyone could work with.
    fn is_usable(&self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.width >= MIN_EDGE
            && self.height >= MIN_EDGE
    }
}

fn file() -> Option<PathBuf> {
    crate::logic::settings::get_settings_folder().map(|folder| folder.join("window.json"))
}

/// The size the main window had at the end of the last session.
///
/// Also becomes the starting point for what is saved during this one, so that
/// a session in which the window is only ever maximised still remembers the
/// size to come back to.
///
/// `None` when there is nothing to restore — the first start, a file that
/// cannot be read, or a size that would be unusable.
pub fn load() -> Option<WindowState> {
    let text = std::fs::read_to_string(file()?).ok()?;
    let state: WindowState = serde_json::from_str(&text).ok()?;
    if !state.is_usable() {
        return None;
    }
    if let Ok(mut pending) = PENDING.lock() {
        *pending = Some(state);
    }
    Some(state)
}

/// Takes note of the window's geometry, and writes it if it has been a while.
///
/// `size` is `None` while the window is maximised: what it reports then is the
/// screen it is on, and keeping that as the ordinary size would mean the
/// window never came back to the one the user chose.
pub fn record(size: Option<(f64, f64)>, maximized: bool) {
    let changed = {
        let Ok(mut pending) = PENDING.lock() else {
            return;
        };
        let updated = match (*pending, size) {
            (_, Some((width, height))) => WindowState {
                width,
                height,
                maximized,
            },
            (Some(previous), None) => WindowState {
                maximized,
                ..previous
            },
            // Maximised from the first moment, with no earlier size to keep:
            // there is nothing to write yet.
            (None, None) => return,
        };
        if !updated.is_usable() || *pending == Some(updated) {
            return;
        }
        *pending = Some(updated);
        true
    };

    let due = LAST_WRITE
        .lock()
        .map(|last| last.is_none_or(|at| at.elapsed() >= WRITE_INTERVAL))
        .unwrap_or(false);

    if changed && due {
        flush();
    }
}

/// Writes the geometry now. Called when the window is closing, where losing
/// the last resize would be exactly the one the user meant to keep.
pub fn flush() {
    let Some(state) = PENDING.lock().ok().and_then(|pending| *pending) else {
        return;
    };
    let Some(path) = file() else {
        return;
    };
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    match serde_json::to_string_pretty(&state) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&path, json) {
                log::warn!("could not remember the window size: {error}");
            }
        }
        Err(error) => log::warn!("could not remember the window size: {error}"),
    }
    if let Ok(mut last) = LAST_WRITE.lock() {
        *last = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        *PENDING.lock().unwrap() = None;
        *LAST_WRITE.lock().unwrap() = None;
    }

    fn pending() -> Option<WindowState> {
        *PENDING.lock().unwrap()
    }

    /// A window nobody could work with is not restored — a minimised window
    /// reports nothing at all, and handing that back on the next start would
    /// open Cantara as a sliver.
    #[test]
    fn an_unusable_size_is_not_kept() {
        reset();

        record(Some((0.0, 0.0)), false);
        assert_eq!(pending(), None);

        record(Some((f64::NAN, 800.0)), false);
        assert_eq!(pending(), None);

        record(Some((900.0, 800.0)), false);
        assert_eq!(pending().map(|state| state.width), Some(900.0));
    }

    /// While the window is maximised its size is the screen's, so the size the
    /// user chose has to survive being maximised and un-maximised.
    #[test]
    fn maximising_does_not_forget_the_chosen_size() {
        reset();

        record(Some((900.0, 800.0)), false);
        record(None, true);

        let kept = pending().expect("a size was recorded");
        assert_eq!((kept.width, kept.height), (900.0, 800.0));
        assert!(kept.maximized, "the window was maximised when it was closed");
    }

    /// Nothing to go on and nothing to write: a window that is maximised from
    /// the very first moment must not invent a size.
    #[test]
    fn a_window_maximised_from_the_start_writes_nothing() {
        reset();

        record(None, true);

        assert_eq!(pending(), None);
    }
}
