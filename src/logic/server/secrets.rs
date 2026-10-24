//! Where the token a server hands this program is kept.
//!
//! In the system's keyring where there is one — Keychain, Credential Manager,
//! the Secret Service of the Linux desktop — under the service `Cantara` and
//! the instance's uuid. The settings file is readable by everything running
//! as the user and ends up in backups; a keyring is neither.
//!
//! Where there is none (mobile, the browser, a Linux without a Secret
//! Service running) [`store`] says so and the caller keeps the token in the
//! settings file instead: a login that works is worth more than one that is
//! refused for want of a keyring.

use uuid::Uuid;

#[cfg(any(target_os = "macos", target_os = "windows", all(target_os = "linux", not(target_arch = "wasm32"))))]
mod keyring_store {
    use uuid::Uuid;

    const SERVICE: &str = "Cantara";

    // The Secret Service is reached over D-Bus through zbus, which brings its
    // own executor. Called from inside the program's async runtime it would
    // try to block on that runtime's thread, so every call runs on a thread of
    // its own and is waited for.
    fn on_own_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        std::thread::spawn(work).join().ok()
    }

    pub fn store(instance: Uuid, token: &str) -> bool {
        let token = token.to_string();
        on_own_thread(move || {
            keyring::Entry::new(SERVICE, &instance.to_string())
                .and_then(|entry| entry.set_password(&token))
                .map_err(|error| log::warn!("the token cannot be kept in the keyring: {error}"))
                .is_ok()
        })
        .unwrap_or(false)
    }

    pub fn load(instance: Uuid) -> Option<String> {
        on_own_thread(move || {
            keyring::Entry::new(SERVICE, &instance.to_string())
                .and_then(|entry| entry.get_password())
                .ok()
        })
        .flatten()
    }

    pub fn delete(instance: Uuid) {
        let _ = on_own_thread(move || {
            let _ = keyring::Entry::new(SERVICE, &instance.to_string()).and_then(|entry| entry.delete_credential());
        });
    }
}

/// Keeps `token` for `instance`. `false` where there is no keyring to keep it
/// in.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn store(instance: Uuid, token: &str) -> bool {
    #[cfg(any(target_os = "macos", target_os = "windows", all(target_os = "linux", not(target_arch = "wasm32"))))]
    {
        keyring_store::store(instance, token)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", all(target_os = "linux", not(target_arch = "wasm32")))))]
    {
        let _ = (instance, token);
        false
    }
}

/// The token kept for `instance`, if the keyring has one.
pub fn load(instance: Uuid) -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "windows", all(target_os = "linux", not(target_arch = "wasm32"))))]
    {
        keyring_store::load(instance)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", all(target_os = "linux", not(target_arch = "wasm32")))))]
    {
        let _ = instance;
        None
    }
}

/// Forgets the token kept for `instance`.
pub fn delete(instance: Uuid) {
    #[cfg(any(target_os = "macos", target_os = "windows", all(target_os = "linux", not(target_arch = "wasm32"))))]
    keyring_store::delete(instance);
    #[cfg(not(any(target_os = "macos", target_os = "windows", all(target_os = "linux", not(target_arch = "wasm32")))))]
    let _ = instance;
}
