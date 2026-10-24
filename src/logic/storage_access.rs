//! Whether Cantara may read the files in a phone's shared storage, and asking
//! for it.
//!
//! Since Android 11, an app may list the folders of shared storage
//! (`/storage/emulated/0`) but read only the pictures, videos and music in
//! them — every other file is simply not there. Songs (`.song`, `.ccli`),
//! PDFs and Markdown files are exactly such other files, which is what users
//! saw: the directory browser showed the folders, and the library stayed
//! empty.
//!
//! Cantara reads its repositories through ordinary paths, on every platform,
//! with the same code. What keeps that working on Android is the permission
//! "access to all files" (`MANAGE_EXTERNAL_STORAGE`), which is not asked for
//! in a dialog but granted by the user on a page of the system settings. That
//! page is what [`request`] opens. Android 7 to 10 know nothing of it; there
//! the older read and write permissions are asked for in the usual dialog.
//!
//! The alternative — the Storage Access Framework, where the user picks a
//! folder and the app receives `content://` addresses instead of paths —
//! would need every file access in the program to go through Java. The
//! permission's drawback is Google Play, which admits it only for certain
//! kinds of apps; Cantara is distributed as an APK of its own.
//!
//! Neither way of asking reports back: the settings page is another app, and
//! the dialog's answer goes to the activity, which is Dioxus' and not ours. So
//! whoever asks watches [`has_access`] until it turns true — see
//! [`crate::components::storage_access`].
//!
//! The manifest entries these permissions need are in `Dioxus.toml`
//! (`[android.raw]`), and `requestLegacyExternalStorage`, which Android 10
//! needs on top, is added by `packaging/scripts/build-android-apk.sh`.
//!
//! Everywhere else the answer is always yes, and asking does nothing.

/// Whether Cantara can read every file in shared storage.
///
/// Cheap enough to be called once a second: two calls into Java.
pub fn has_access() -> bool {
    #[cfg(target_os = "android")]
    {
        android::has_access()
    }
    #[cfg(not(target_os = "android"))]
    {
        true
    }
}

/// Whether this platform asks for access at all, so that the interface
/// can leave out everything about it where it would only confuse.
pub const fn is_restricted_platform() -> bool {
    cfg!(target_os = "android")
}

/// Asks the user for access: opens the settings page on Android 11 and newer,
/// the permission dialog before that.
///
/// Returns as soon as the question is on screen, not when it is answered —
/// watch [`has_access`] for that.
pub fn request() {
    #[cfg(target_os = "android")]
    android::request();
}

#[cfg(target_os = "android")]
mod android {
    // `::` because `dioxus::prelude::*` also brings a `jni` into scope; see
    // `get_android_files_dir` in `logic::settings`.
    use ::jni::errors::Error as JniError;
    use ::jni::objects::{JObject, JString, JValue};
    use ::jni::{Env, JavaVM, jni_sig, jni_str};
    use log::{error, info, warn};

    /// Android 11, the first version with "access to all files".
    const ANDROID_11: i32 = 30;
    /// `PackageManager.PERMISSION_GRANTED`.
    const PERMISSION_GRANTED: i32 = 0;
    /// `Intent.FLAG_ACTIVITY_NEW_TASK`: needed if the context Dioxus hands
    /// out is not an activity, harmless if it is.
    const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;
    /// Whatever: the answer is never received (see the module notes).
    const REQUEST_CODE: i32 = 4711;

    const READ_EXTERNAL_STORAGE: &str = "android.permission.READ_EXTERNAL_STORAGE";
    const WRITE_EXTERNAL_STORAGE: &str = "android.permission.WRITE_EXTERNAL_STORAGE";

    /// The settings page for this app's "access to all files".
    const MANAGE_APP_ALL_FILES: &str = "android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION";
    /// The same for all apps, which some manufacturers' settings have instead.
    const MANAGE_ALL_FILES: &str = "android.settings.MANAGE_ALL_FILES_ACCESS_PERMISSION";
    /// The app's page in the settings, from which every permission can be
    /// granted — the last resort.
    const APPLICATION_DETAILS: &str = "android.settings.APPLICATION_DETAILS_SETTINGS";

    pub(super) fn has_access() -> bool {
        with_activity("ask Android whether Cantara may read shared storage", |env, activity| {
            if sdk_int(env)? >= ANDROID_11 {
                env.call_static_method(
                    jni_str!("android/os/Environment"),
                    jni_str!("isExternalStorageManager"),
                    jni_sig!("()Z"),
                    &[],
                )?
                .z()
            } else {
                is_granted(env, activity, READ_EXTERNAL_STORAGE)
            }
        })
        .unwrap_or(false)
    }

    pub(super) fn request() {
        with_activity("ask for access to shared storage", |env, activity| {
            if sdk_int(env)? >= ANDROID_11 {
                let package = package_name(env, activity)?;
                let uri = format!("package:{package}");
                // The page for this app, then the list of all apps, then the
                // app's own settings page: not every manufacturer's settings
                // have all three.
                if open_settings(env, activity, MANAGE_APP_ALL_FILES, Some(&uri))?
                    || open_settings(env, activity, MANAGE_ALL_FILES, None)?
                    || open_settings(env, activity, APPLICATION_DETAILS, Some(&uri))?
                {
                    return Ok(());
                }
                warn!("No settings page for access to all files could be opened.");
                Ok(())
            } else {
                request_permissions(env, activity)
            }
        });
    }

    /// Runs `f` with the activity Dioxus' Android runtime created.
    ///
    /// A Java exception the calls leave behind is cleared here: a pending
    /// exception makes every later call into Java fail, in code that has
    /// nothing to do with this.
    fn with_activity<T>(
        what: &str,
        f: impl FnOnce(&mut Env, &JObject) -> Result<T, JniError>,
    ) -> Option<T> {
        let ctx = ndk_context::android_context();

        // Safety: the VM pointer from ndk-context is valid for the app's lifetime.
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };

        let result: Result<T, JniError> = vm.attach_current_thread(|env| {
            // Safety: the context pointer is a valid Activity jobject managed
            // by android-activity.
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

    /// `Build.VERSION.SDK_INT`, the Android version as a number.
    fn sdk_int(env: &mut Env) -> Result<i32, JniError> {
        env.get_static_field(
            jni_str!("android/os/Build$VERSION"),
            jni_str!("SDK_INT"),
            jni_sig!("I"),
        )?
        .i()
    }

    /// `Context.getPackageName()`.
    fn package_name(env: &mut Env, activity: &JObject) -> Result<String, JniError> {
        let name = env
            .call_method(
                activity,
                jni_str!("getPackageName"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let name = env.cast_local::<JString>(name)?;
        name.try_to_string(env)
    }

    /// `Context.checkSelfPermission(permission) == PERMISSION_GRANTED`.
    fn is_granted(env: &mut Env, activity: &JObject, permission: &str) -> Result<bool, JniError> {
        let permission = env.new_string(permission)?;
        let result = env
            .call_method(
                activity,
                jni_str!("checkSelfPermission"),
                jni_sig!("(Ljava/lang/String;)I"),
                &[JValue::Object(&permission)],
            )?
            .i()?;
        Ok(result == PERMISSION_GRANTED)
    }

    /// Opens a page of the system settings. `false` if this Android has no
    /// such page — `startActivity` throws `ActivityNotFoundException` then,
    /// which is cleared here so that the next one can be tried.
    fn open_settings(
        env: &mut Env,
        activity: &JObject,
        action: &str,
        uri: Option<&str>,
    ) -> Result<bool, JniError> {
        let action_string = env.new_string(action)?;
        let intent = match uri {
            Some(uri) => {
                let uri = env.new_string(uri)?;
                let uri = env
                    .call_static_method(
                        jni_str!("android/net/Uri"),
                        jni_str!("parse"),
                        jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                        &[JValue::Object(&uri)],
                    )?
                    .l()?;
                env.new_object(
                    jni_str!("android/content/Intent"),
                    jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
                    &[JValue::Object(&action_string), JValue::Object(&uri)],
                )?
            }
            None => env.new_object(
                jni_str!("android/content/Intent"),
                jni_sig!("(Ljava/lang/String;)V"),
                &[JValue::Object(&action_string)],
            )?,
        };
        env.call_method(
            &intent,
            jni_str!("addFlags"),
            jni_sig!("(I)Landroid/content/Intent;"),
            &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)],
        )?;

        match env.call_method(
            activity,
            jni_str!("startActivity"),
            jni_sig!("(Landroid/content/Intent;)V"),
            &[JValue::Object(&intent)],
        ) {
            Ok(_) => {
                info!("Opened the settings page {action}.");
                Ok(true)
            }
            Err(JniError::JavaException) => {
                env.exception_clear();
                warn!("This Android has no settings page {action}.");
                Ok(false)
            }
            Err(err) => Err(err),
        }
    }

    /// `Activity.requestPermissions`, for Android 7 to 10.
    fn request_permissions(env: &mut Env, activity: &JObject) -> Result<(), JniError> {
        let permissions =
            env.new_object_array(2, jni_str!("java/lang/String"), JObject::null())?;
        let read = env.new_string(READ_EXTERNAL_STORAGE)?;
        let write = env.new_string(WRITE_EXTERNAL_STORAGE)?;
        permissions.set_element(env, 0, &read)?;
        permissions.set_element(env, 1, &write)?;
        env.call_method(
            activity,
            jni_str!("requestPermissions"),
            jni_sig!("([Ljava/lang/String;I)V"),
            &[JValue::Object(&permissions), JValue::Int(REQUEST_CODE)],
        )?;
        info!("Asked for permission to read and write shared storage.");
        Ok(())
    }
}
