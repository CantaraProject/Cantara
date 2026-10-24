//! Talking to Java from Cantara's Android build.
//!
//! What the program needs from Android that Rust cannot ask by itself — the
//! storage permission ([`super::storage_access`]), the dark mode
//! ([`super::system_theme`]) — goes through the activity Dioxus' Android
//! runtime created, which `ndk-context` hands out.

// `::` because `dioxus::prelude::*` also brings a `jni` into scope; see
// `get_android_files_dir` in `logic::settings`.
use ::jni::errors::Error as JniError;
use ::jni::objects::JObject;
use ::jni::{Env, JavaVM};
use log::error;

/// Runs `f` with the activity, on the current thread attached to the VM.
///
/// `what` completes "Could not …" in the log, if it fails. A Java exception
/// the calls leave behind is cleared here: a pending exception makes every
/// later call into Java fail, in code that has nothing to do with this.
pub(crate) fn with_activity<T>(
    what: &str,
    f: impl FnOnce(&mut Env, &JObject) -> Result<T, JniError>,
) -> Option<T> {
    let ctx = ndk_context::android_context();

    // Safety: the VM pointer from ndk-context is valid for the app's lifetime.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };

    let result: Result<T, JniError> = vm.attach_current_thread(|env| {
        // Safety: the context pointer is a valid Activity jobject managed by
        // android-activity.
        let activity = unsafe { JObject::from_raw(env, ctx.context().cast()) };
        let result = f(env, &activity);
        if env.exception_check() {
            env.exception_describe();
            env.exception_clear();
        }
        result
    });

    match result {
        Ok(value) => Some(value),
        Err(err) => {
            error!("Could not {what}: {err}");
            None
        }
    }
}
