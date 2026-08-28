//! The browser's local storage, as the three calls this program actually
//! makes.
//!
//! Every one of these used to be written out where it was needed:
//! `web_sys::window().and_then(|w| w.local_storage().ok().flatten())`, then a
//! `get_item` and a `serde_json::from_str` whose errors were dropped on the
//! floor. Dropping them is right — a browser with no storage, or with storage
//! full, is a browser the program still has to run in, and there is nothing
//! useful to say about it at any of these call sites. But it is one decision,
//! and it was being re-made eighteen times.
//!
//! Reading anything that is not there, or cannot be understood, gives `None`.
//! Writing anything that cannot be stored does nothing. A caller that needs to
//! know the difference — there is one, [`crate::logic::settings::Settings`]
//! saving itself — takes [`storage`] and says so in its own words.

use serde::de::DeserializeOwned;
use serde::Serialize;
use web_sys::Storage;

/// The window's local storage, where the browser offers one.
pub fn storage() -> Option<Storage> {
    web_sys::window().and_then(|window| window.local_storage().ok().flatten())
}

/// What is stored under a key, as it was written.
pub fn text(key: &str) -> Option<String> {
    storage().and_then(|storage| storage.get_item(key).ok().flatten())
}

/// What is stored under a key, read back as `T`.
///
/// `None` where nothing is stored *and* where what is stored is not a `T` — a
/// value written by an older version of Cantara is not readable and is not a
/// value, and both mean the same thing to every caller here.
pub fn read<T: DeserializeOwned>(key: &str) -> Option<T> {
    text(key).and_then(|json| serde_json::from_str(&json).ok())
}

/// Whether a key holds the flag `"true"`.
///
/// The keys in [`crate::logic::sync`] that mark a state rather than carry one
/// are written as this string; anything else, the key's absence included, is
/// `false`.
pub fn flag(key: &str) -> bool {
    text(key).is_some_and(|value| value == "true")
}

/// Stores a string under a key.
pub fn write_text(key: &str, value: &str) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(key, value);
    }
}

/// Stores `value` under a key as JSON.
pub fn write<T: Serialize>(key: &str, value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        write_text(key, &json);
    }
}

/// Forgets a key.
pub fn remove(key: &str) {
    if let Some(storage) = storage() {
        let _ = storage.remove_item(key);
    }
}

/// Forgets several keys.
///
/// Ending a synced session clears a handful at once, and a list reads better
/// there than a column of calls.
pub fn remove_all(keys: &[&str]) {
    if let Some(storage) = storage() {
        for key in keys {
            let _ = storage.remove_item(key);
        }
    }
}
