//! Handing files to Android's share sheet.
//!
//! A web view on Android drops a download on the floor: there is no handler
//! for it, so the export the other builds hand to the page as a download
//! reported success and produced nothing. And a phone has no file dialog that
//! Rust could open. What it has instead is the share sheet, from which a file
//! goes to "Files" to be saved, to a cloud drive, to a mail or a messenger —
//! which is where an exported song or deck is usually headed anyway.
//!
//! The files are written into the app's cache, under `shared/`, and offered
//! through a `FileProvider`: a `content://` address that the app receiving
//! the file is allowed to read for as long as it needs to. The provider and
//! the folder it may hand out are declared in the manifest, and kept from
//! being shrunk away by R8, by `packaging/scripts/build-android-apk.sh` (with
//! `packaging/android/`) — dx has no setting for either.

use ::jni::errors::Error as JniError;
use ::jni::objects::{JClass, JObject, JString, JValue};
use ::jni::{Env, jni_sig, jni_str};
use log::info;

use super::android_jni::with_activity;

/// The folder under the cache that the provider may hand out. Kept in step
/// with `packaging/android/res/xml/cantara_shared_files.xml`.
const SHARED_FOLDER: &str = "shared";

/// What the provider is called: the package name and this. Kept in step with
/// the `<provider>` that `build-android-apk.sh` adds to the manifest.
const AUTHORITY_SUFFIX: &str = ".fileprovider";

/// `Intent.FLAG_GRANT_READ_URI_PERMISSION`.
const FLAG_GRANT_READ_URI_PERMISSION: i32 = 0x0000_0001;
/// `Intent.FLAG_ACTIVITY_NEW_TASK`.
const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;

/// A file to be shared: its name, with the extension, and its content.
pub struct SharedFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Offers `files` in the share sheet, under the heading `title`.
///
/// Returns once the sheet is on screen; what the user does with it is the
/// receiving app's business, and Android does not say.
pub fn share(files: Vec<SharedFile>, title: &str) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    with_activity("offer a file to share", |env, activity| {
        let folder = shared_folder(env, activity)?;
        let paths = write_files(&folder, &files).map_err(|reason| {
            log::error!("could not write the files to share: {reason}");
            JniError::JavaException
        })?;
        offer(env, activity, &paths, title)
    })
    .ok_or_else(|| "Android would not take the file".to_string())
}

/// `<cache>/shared`, emptied of whatever an earlier export left in it.
fn shared_folder(env: &mut Env, activity: &JObject) -> Result<std::path::PathBuf, JniError> {
    let cache = env
        .call_method(activity, jni_str!("getCacheDir"), jni_sig!("()Ljava/io/File;"), &[])?
        .l()?;
    let path = env
        .call_method(
            &cache,
            jni_str!("getAbsolutePath"),
            jni_sig!("()Ljava/lang/String;"),
            &[],
        )?
        .l()?;
    let path = env.cast_local::<JString>(path)?;
    let folder = std::path::PathBuf::from(path.try_to_string(env)?).join(SHARED_FOLDER);

    // The files of the last export have been handed over by now, and a cache
    // that only ever grows is a phone that runs out of space.
    let _ = std::fs::remove_dir_all(&folder);
    Ok(folder)
}

/// Writes every file into `folder` and gives back where each one went.
fn write_files(folder: &std::path::Path, files: &[SharedFile]) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    files
        .iter()
        .map(|file| {
            // The name is the song's title: a slash in it would be a folder.
            let name = file.name.replace(['/', '\\'], "-");
            let path = folder.join(name);
            std::fs::write(&path, &file.bytes).map_err(|error| error.to_string())?;
            Ok(path.display().to_string())
        })
        .collect()
}

/// Builds the intent and opens the share sheet.
fn offer(env: &mut Env, activity: &JObject, paths: &[String], title: &str) -> Result<(), JniError> {
    let authority = format!("{}{AUTHORITY_SUFFIX}", package_name(env, activity)?);
    let provider = file_provider_class(env, activity)?;
    let authority = env.new_string(&authority)?;

    let mut uris = Vec::with_capacity(paths.len());
    for path in paths {
        let path = env.new_string(path)?;
        let file = env.new_object(
            jni_str!("java/io/File"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&path)],
        )?;
        let uri = env
            .call_static_method(
                &provider,
                jni_str!("getUriForFile"),
                jni_sig!("(Landroid/content/Context;Ljava/lang/String;Ljava/io/File;)Landroid/net/Uri;"),
                &[
                    JValue::Object(activity),
                    JValue::Object(&authority),
                    JValue::Object(&file),
                ],
            )?
            .l()?;
        uris.push(uri);
    }

    let mime = env.new_string(mime_type(paths))?;
    let stream = env.new_string("android.intent.extra.STREAM")?;

    let intent = if let [uri] = uris.as_slice() {
        let action = env.new_string("android.intent.action.SEND")?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&action)],
        )?;
        env.call_method(
            &intent,
            jni_str!("putExtra"),
            jni_sig!("(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;"),
            &[JValue::Object(&stream), JValue::Object(uri)],
        )?;
        intent
    } else {
        let action = env.new_string("android.intent.action.SEND_MULTIPLE")?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&action)],
        )?;
        let list = env.new_object(jni_str!("java/util/ArrayList"), jni_sig!("()V"), &[])?;
        for uri in &uris {
            env.call_method(
                &list,
                jni_str!("add"),
                jni_sig!("(Ljava/lang/Object;)Z"),
                &[JValue::Object(uri)],
            )?;
        }
        env.call_method(
            &intent,
            jni_str!("putParcelableArrayListExtra"),
            jni_sig!("(Ljava/lang/String;Ljava/util/ArrayList;)Landroid/content/Intent;"),
            &[JValue::Object(&stream), JValue::Object(&list)],
        )?;
        intent
    };

    env.call_method(
        &intent,
        jni_str!("setType"),
        jni_sig!("(Ljava/lang/String;)Landroid/content/Intent;"),
        &[JValue::Object(&mime)],
    )?;

    // The files as clip data as well: that is what the share sheet itself
    // reads to show a preview, and what the read permission below is granted
    // through on the Android versions that look there rather than at the
    // extra.
    let label = env.new_string("")?;
    let clip = env
        .call_static_method(
            jni_str!("android/content/ClipData"),
            jni_str!("newRawUri"),
            jni_sig!("(Ljava/lang/CharSequence;Landroid/net/Uri;)Landroid/content/ClipData;"),
            &[JValue::Object(&label), JValue::Object(&uris[0])],
        )?
        .l()?;
    for uri in uris.iter().skip(1) {
        let item = env.new_object(
            jni_str!("android/content/ClipData$Item"),
            jni_sig!("(Landroid/net/Uri;)V"),
            &[JValue::Object(uri)],
        )?;
        env.call_method(
            &clip,
            jni_str!("addItem"),
            jni_sig!("(Landroid/content/ClipData$Item;)V"),
            &[JValue::Object(&item)],
        )?;
    }
    env.call_method(
        &intent,
        jni_str!("setClipData"),
        jni_sig!("(Landroid/content/ClipData;)V"),
        &[JValue::Object(&clip)],
    )?;
    env.call_method(
        &intent,
        jni_str!("addFlags"),
        jni_sig!("(I)Landroid/content/Intent;"),
        &[JValue::Int(FLAG_GRANT_READ_URI_PERMISSION)],
    )?;

    let title = env.new_string(title)?;
    let chooser = env
        .call_static_method(
            jni_str!("android/content/Intent"),
            jni_str!("createChooser"),
            jni_sig!("(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;"),
            &[JValue::Object(&intent), JValue::Object(&title)],
        )?
        .l()?;
    env.call_method(
        &chooser,
        jni_str!("addFlags"),
        jni_sig!("(I)Landroid/content/Intent;"),
        &[JValue::Int(FLAG_ACTIVITY_NEW_TASK | FLAG_GRANT_READ_URI_PERMISSION)],
    )?;
    env.call_method(
        activity,
        jni_str!("startActivity"),
        jni_sig!("(Landroid/content/Intent;)V"),
        &[JValue::Object(&chooser)],
    )?;

    info!("Offered {} file(s) to share.", paths.len());
    Ok(())
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

/// `androidx.core.content.FileProvider`, through the app's class loader.
///
/// Not by name: a thread that Rust attached to the VM finds only Android's
/// own classes that way, and this one comes with the app.
fn file_provider_class<'local>(
    env: &mut Env<'local>,
    activity: &JObject,
) -> Result<JClass<'local>, JniError> {
    let loader = env
        .call_method(
            activity,
            jni_str!("getClassLoader"),
            jni_sig!("()Ljava/lang/ClassLoader;"),
            &[],
        )?
        .l()?;
    let name = env.new_string("androidx.core.content.FileProvider")?;
    let class = env
        .call_method(
            &loader,
            jni_str!("loadClass"),
            jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
            &[JValue::Object(&name)],
        )?
        .l()?;
    env.cast_local::<JClass>(class)
}

/// What the receiving app is told the files are.
///
/// One type for the whole set, as `Intent.setType` takes one: the files'
/// own if they agree, anything otherwise.
fn mime_type(paths: &[String]) -> &'static str {
    let of = |path: &String| match path.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("zip") => "application/zip",
        Some("json") => "application/json",
        Some("pdf") => "application/pdf",
        Some("txt" | "song" | "songtex" | "md" | "ccli") => "text/plain",
        _ => "application/octet-stream",
    };
    let first = paths.first().map(of).unwrap_or("application/octet-stream");
    if paths.iter().all(|path| of(path) == first) {
        first
    } else {
        "*/*"
    }
}
