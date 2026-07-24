//! Constants for cross-tab presentation synchronization via localStorage.
//!
//! These keys are used on web targets to coordinate state between the presentation
//! tab and the presenter console tab. They live in a shared module to avoid
//! duplicating raw string literals across multiple component files.

/// localStorage key holding the serialized [`RunningPresentation`](super::states::RunningPresentation)
/// that is loaded by a newly-opened presentation tab.
pub const SYNC_KEY_PRESENTATION: &str = "cantara-sync-presentation";

/// localStorage key set to `"true"` while a synced presentation session is active.
pub const SYNC_KEY_ACTIVE: &str = "cantara-sync-active";

/// localStorage key set to `"true"` to signal that the presentation should quit.
pub const SYNC_KEY_QUIT: &str = "cantara-sync-quit";

/// localStorage key written by the presentation tab with its current position/state.
pub const SYNC_KEY_POSITION: &str = "cantara-sync-position";

/// localStorage key written by the presenter console with its current position/state.
pub const SYNC_KEY_POSITION_FROM_CONSOLE: &str = "cantara-sync-position-from-console";

/// localStorage key for synchronizing markdown scroll position across tabs.
pub const SYNC_KEY_SCROLL_POSITION: &str = "cantara-sync-scroll-position";

/// localStorage key holding base64-encoded VFS file data (e.g. PDFs) needed by
/// the presentation tab. The value is a JSON map of `{ path: base64_data }`.
pub const SYNC_KEY_FILES: &str = "cantara-sync-files";
