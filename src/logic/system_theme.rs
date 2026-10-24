//! Whether Android is in dark mode.
//!
//! The page decides between PicoCSS' light and dark colours by
//! `prefers-color-scheme`, which every other web view takes from the system.
//! Android's does not: for an app that targets Android 13 or newer, it takes
//! it from the *app's* theme, and dx gives every app a light one. So the page
//! was light whatever the phone was set to.
//!
//! Two things put that right. `Dioxus.toml` gives the app a DayNight theme,
//! which follows the system and also makes the window behind the page and
//! the system bars dark. And this asks the system itself, so that `App` can
//! set `data-theme` the way it does on Linux — which does not depend on the
//! version of the WebView installed, and which follows the switch while
//! Cantara is open: the activity declares `uiMode` among the changes it
//! handles itself, so it is not recreated, and nothing else would tell the
//! page.

use ::jni::errors::Error as JniError;
use ::jni::{jni_sig, jni_str};

use super::android_jni::with_activity;

/// `Configuration.UI_MODE_NIGHT_MASK`.
const UI_MODE_NIGHT_MASK: i32 = 0x30;
/// `Configuration.UI_MODE_NIGHT_YES`.
const UI_MODE_NIGHT_YES: i32 = 0x20;

/// Whether the system is in dark mode: `getResources().getConfiguration()
/// .uiMode`. `None` if Android could not be asked.
///
/// Cheap: three calls into Java.
pub fn is_dark() -> Option<bool> {
    with_activity("ask Android whether it is in dark mode", |env, activity| {
        let resources = env
            .call_method(
                activity,
                jni_str!("getResources"),
                jni_sig!("()Landroid/content/res/Resources;"),
                &[],
            )?
            .l()?;
        let configuration = env
            .call_method(
                &resources,
                jni_str!("getConfiguration"),
                jni_sig!("()Landroid/content/res/Configuration;"),
                &[],
            )?
            .l()?;
        let ui_mode = env
            .get_field(&configuration, jni_str!("uiMode"), jni_sig!("I"))?
            .i()?;
        Ok::<bool, JniError>(ui_mode & UI_MODE_NIGHT_MASK == UI_MODE_NIGHT_YES)
    })
}
