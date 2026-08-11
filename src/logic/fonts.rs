//! The font families Cantara can offer for a presentation.
//!
//! Three sources feed the list, in this order:
//!
//! 1. **Bundled** — anything dropped into `assets/fonts/`. These are shipped
//!    with the application, so a presentation looks the same on every machine
//!    and on the web. See [`bundled`].
//! 2. **System** — the fonts installed on the computer. Only on desktop; the
//!    web build has no way to enumerate them.
//! 3. **Web-safe** — a short list of families that are present on virtually
//!    every system. These are the only sensible choice online, and they are
//!    offered on the desktop too so that a presentation built there still
//!    renders when it is opened in a browser.
//!
//! # Adding a bundled font
//!
//! Put the file in `assets/fonts/` and rebuild. `build.rs` picks up every
//! `.ttf`, `.otf`, `.woff` and `.woff2` there and generates the list this
//! module reads; nothing else has to be registered.

use std::collections::HashSet;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Where a font family comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontSource {
    /// Shipped with Cantara, available everywhere.
    Bundled,
    /// Installed on this computer. Desktop only, and not guaranteed to exist
    /// on the machine a presentation is later opened on.
    System,
    /// Present on virtually every system, so safe to use anywhere.
    WebSafe,
    /// Came with an imported design and was kept, so that the design looks
    /// here the way it looked where it was made. See [`imported`].
    Imported,
}

impl FontSource {
    /// Whether a design using this family can be handed to another Cantara
    /// without the font going with it.
    ///
    /// A bundled family is in every copy of the program and a web-safe one is
    /// on every computer. Anything else has to travel with the design, which
    /// is what a design archive is for.
    pub fn travels_with_cantara(self) -> bool {
        matches!(self, FontSource::Bundled | FontSource::WebSafe)
    }
}

/// One font family a user can pick.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FontFamily {
    /// The family name, used verbatim in CSS.
    pub name: String,
    pub source: FontSource,
}

impl fmt::Display for FontFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// Families that exist on practically every platform.
///
/// Kept deliberately short: a long list of near-duplicates makes the settings
/// harder to use, and anything unusual is better shipped as a bundled font.
const WEB_SAFE_FAMILIES: &[&str] = &[
    "Arial",
    "Verdana",
    "Tahoma",
    "Trebuchet MS",
    "Times New Roman",
    "Georgia",
    "Garamond",
    "Courier New",
    "Brush Script MT",
];

/// The fonts shipped inside the application.
///
/// `build.rs` writes this table from the contents of `assets/fonts/`, as
/// `(family name, file name)` pairs.
mod bundled_data {
    include!(concat!(env!("OUT_DIR"), "/bundled_fonts_data.rs"));
}

/// The families bundled with Cantara.
pub fn bundled() -> Vec<FontFamily> {
    bundled_data::BUNDLED_FONTS
        .iter()
        .map(|(name, _)| FontFamily {
            name: (*name).to_string(),
            source: FontSource::Bundled,
        })
        .collect()
}

// ── The installed families ───────────────────────────────────────────────────
//
// Reading them means opening and parsing every font file on the computer.
// That took the better part of a second here and rather longer on a machine
// with a designer's font collection — and it happened while a settings page
// was rendering, once for every font block on it, and once more when the page
// change had settled and the page was drawn a second time. It is read once per
// run, on a thread of its own, and the settings show what is there and fill
// the rest in when it arrives.

/// The installed families, once they have been read.
static SYSTEM_FAMILIES: OnceLock<Mutex<Option<Vec<FontFamily>>>> = OnceLock::new();

/// Counts up when the families land, so a view that remembers the last value
/// it saw knows to look again. A background thread cannot write to a signal.
static SYSTEM_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Whether a thread is reading them right now.
static SYSTEM_PENDING: AtomicU64 = AtomicU64::new(0);

fn system_cache() -> &'static Mutex<Option<Vec<FontFamily>>> {
    SYSTEM_FAMILIES.get_or_init(|| Mutex::new(None))
}

/// The installed families, but only if they have already been read.
///
/// Never touches the file system: this is what a view calls while it renders.
/// `None` means "not yet", which the settings show as the bundled and web-safe
/// families alone.
pub fn system_ready() -> Option<Vec<FontFamily>> {
    system_cache().lock().ok()?.clone()
}

/// Counts up when the installed families arrive.
pub fn system_generation() -> u64 {
    SYSTEM_GENERATION.load(Ordering::Relaxed)
}

/// Whether the installed families are still being read.
pub fn system_fonts_pending() -> bool {
    SYSTEM_PENDING.load(Ordering::Relaxed) > 0
}

/// Reads the installed families off the calling thread.
///
/// Returns at once, and does nothing when they are already there or on their
/// way — so a view may call it whenever it is drawn.
pub fn prepare_system_fonts() {
    if system_ready().is_some() {
        return;
    }
    // Claim the work: whoever moves the counter from 0 to 1 does it.
    if SYSTEM_PENDING
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let read_them = || {
        let families = system();
        if let Ok(mut cache) = system_cache().lock() {
            *cache = Some(families);
        }
        SYSTEM_PENDING.store(0, Ordering::SeqCst);
        SYSTEM_GENERATION.fetch_add(1, Ordering::Relaxed);
    };

    // The web build has no threads, and nothing to enumerate either — there
    // this is an empty list and costs nothing.
    #[cfg(target_arch = "wasm32")]
    read_them();

    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(read_them);
}

/// The font families installed on this computer.
///
/// Enumerating them needs a font directory, which the web build does not have,
/// so there it yields nothing.
#[cfg(not(target_arch = "wasm32"))]
pub fn system() -> Vec<FontFamily> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();

    let mut names: Vec<String> = database
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
        .collect();

    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();

    names
        .into_iter()
        .map(|name| FontFamily {
            name,
            source: FontSource::System,
        })
        .collect()
}

/// The web build cannot enumerate installed fonts.
#[cfg(target_arch = "wasm32")]
pub fn system() -> Vec<FontFamily> {
    Vec::new()
}

// ── Fonts that came with a design ────────────────────────────────────────────
//
// A design made on a computer with a font that this one does not have would
// otherwise be shown in something else entirely — which is the one thing a
// design is supposed to pin down. So a design archive carries its font files,
// and they are kept here: in a folder of Cantara's own, beside the settings,
// and declared to the page as `@font-face` so that the family works by name
// exactly as a bundled one does.
//
// Nothing is installed into the operating system. The files belong to Cantara
// and can be deleted with its settings.

/// Where imported font files are kept.
///
/// `None` where there is no settings folder to put them beside — the web
/// build, which has no file system at all.
#[cfg(not(target_arch = "wasm32"))]
pub fn imported_folder() -> Option<std::path::PathBuf> {
    crate::logic::settings::get_settings_folder().map(|folder| folder.join("fonts"))
}

/// The web build keeps no files, so it has no folder for them either.
#[cfg(target_arch = "wasm32")]
pub fn imported_folder() -> Option<std::path::PathBuf> {
    None
}

/// The suffixes a font file may have, which is also what is offered to a
/// browser as a `@font-face` format.
const FONT_SUFFIXES: &[(&str, &str)] = &[
    (".ttf", "truetype"),
    (".otf", "opentype"),
    (".woff2", "woff2"),
    (".woff", "woff"),
];

/// The families that came with an imported design.
///
/// The family is the file's own name, as it is for a bundled font: what the
/// user sees in the settings is what the file is called.
#[cfg(not(target_arch = "wasm32"))]
pub fn imported() -> Vec<FontFamily> {
    let Some(folder) = imported_folder() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut families: Vec<FontFamily> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            font_format(&name)?;
            Some(FontFamily {
                name: family_of_file(&name),
                source: FontSource::Imported,
            })
        })
        .collect();
    families.sort_by_key(|family| family.name.to_lowercase());
    families.dedup_by(|one, other| one.name == other.name);
    families
}

/// The web build has nowhere to keep them.
#[cfg(target_arch = "wasm32")]
pub fn imported() -> Vec<FontFamily> {
    Vec::new()
}

/// The family a font file stands for: its name without the suffix, which is
/// the same rule `build.rs` applies to the bundled ones.
pub fn family_of_file(file_name: &str) -> String {
    let lower = file_name.to_lowercase();
    for (suffix, _) in FONT_SUFFIXES {
        if lower.ends_with(suffix) {
            return file_name[..file_name.len() - suffix.len()].to_string();
        }
    }
    file_name.to_string()
}

/// What a browser has to be told a font file is, or `None` when the name does
/// not promise a font at all.
pub fn font_format(file_name: &str) -> Option<&'static str> {
    let lower = file_name.to_lowercase();
    FONT_SUFFIXES
        .iter()
        .find(|(suffix, _)| lower.ends_with(suffix))
        .map(|(_, format)| *format)
}

/// Keeps a font file that came with a design.
///
/// A file that is already there is left alone: the same design imported twice
/// must not write the font twice, and a file of that name holding something
/// else is not overwritten either — it is the same family, and one copy of it
/// is what is wanted.
#[cfg(not(target_arch = "wasm32"))]
pub fn install(file_name: &str, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    let Some(folder) = imported_folder() else {
        return Err("there is no folder to keep fonts in".to_string());
    };
    if font_format(file_name).is_none() {
        return Err(format!("{file_name} is not a font file"));
    }

    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let path = folder.join(file_name);
    if !path.exists() {
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    }
    Ok(path)
}

/// The web build cannot keep a font file.
#[cfg(target_arch = "wasm32")]
pub fn install(_file_name: &str, _bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    Err("this build keeps no files".to_string())
}

/// The file an installed family is in, and its bytes — what a design archive
/// puts beside the design.
///
/// Looked for among the imported fonts first and among the system's second, so
/// that a design that was imported once can be handed on again.
#[cfg(not(target_arch = "wasm32"))]
pub fn file_of_family(family: &str) -> Option<(String, Vec<u8>)> {
    if let Some(folder) = imported_folder()
        && let Ok(entries) = std::fs::read_dir(&folder)
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if font_format(&name).is_some()
                && family_of_file(&name).eq_ignore_ascii_case(family)
                && let Ok(bytes) = std::fs::read(entry.path())
            {
                return Some((name, bytes));
            }
        }
    }

    // Reading the system's fonts is expensive, which is why it is done here
    // and not kept: this runs when a design is exported, once, and not while
    // anything is being drawn.
    let mut database = fontdb::Database::new();
    database.load_system_fonts();

    let face = database.faces().find(|face| {
        face.families
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(family))
    })?;

    let fontdb::Source::File(path) = &face.source else {
        return None;
    };
    let file_name = path.file_name()?.to_string_lossy().to_string();
    // A family may well come as a `.ttc` collection or something else the page
    // cannot load; there is no point putting that into an archive.
    font_format(&file_name)?;
    let bytes = std::fs::read(path).ok()?;
    Some((file_name, bytes))
}

/// The web build has no fonts of its own to hand out.
#[cfg(target_arch = "wasm32")]
pub fn file_of_family(_family: &str) -> Option<(String, Vec<u8>)> {
    None
}

/// Every family a user can choose *right now*: bundled first, then web-safe,
/// then whatever is installed on this computer and has been read.
///
/// A name is listed once: a bundled font that happens to share its name with an
/// installed one keeps the bundled entry, because that is the copy Cantara will
/// actually use.
///
/// Never touches the file system — the installed families are read by
/// [`prepare_system_fonts`] and appear here once they are in. There is
/// deliberately no variant that waits for them: waiting is what a view must
/// not do.
pub fn available_now() -> Vec<FontFamily> {
    with_installed(system_ready().unwrap_or_default())
}

/// The sources in their order, with each name kept only once.
fn with_installed(installed: Vec<FontFamily>) -> Vec<FontFamily> {
    let mut families = bundled();

    families.extend(WEB_SAFE_FAMILIES.iter().map(|name| FontFamily {
        name: (*name).to_string(),
        source: FontSource::WebSafe,
    }));

    // Before the system's: a family that came with a design is the copy
    // Cantara itself holds, and that is the one to draw with.
    families.extend(imported());

    families.extend(installed);

    // A set rather than a list to look the name up in: a computer with a
    // thousand families made this a million comparisons, on the render.
    let mut seen: HashSet<String> = HashSet::new();
    families.retain(|family| seen.insert(family.name.to_lowercase()));

    families
}

/// `@font-face` rules for the bundled fonts.
///
/// Injected into the app and into the presentation window so the families show
/// up under the names [`bundled`] reports. Returns an empty string when no font
/// is bundled, which is the default.
pub fn bundled_font_face_css() -> String {
    bundled_data::BUNDLED_FONTS
        .iter()
        .map(|(name, file)| {
            format!(
                "@font-face {{\n  font-family: \"{name}\";\n  src: url(\"{}\");\n  font-display: block;\n}}\n",
                bundled_font_url(file)
            )
        })
        .collect()
}

/// Where a bundled font file is served from.
///
/// Assets live next to the executable on the desktop and under the base path on
/// the web; both resolve `/assets/fonts/…`.
fn bundled_font_url(file_name: &str) -> String {
    format!("/assets/fonts/{file_name}")
}

/// `@font-face` rules for the fonts that came with imported designs.
///
/// The file is inlined rather than linked: it sits in Cantara's own folder,
/// which nothing serves — the same reason a picture from the library travels
/// into the page as a `data:` URL. See [`crate::logic::images`].
#[cfg(not(target_arch = "wasm32"))]
pub fn imported_font_face_css() -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let Some(folder) = imported_folder() else {
        return String::new();
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return String::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let format = font_format(&file_name)?;
            let bytes = std::fs::read(entry.path()).ok()?;
            Some(format!(
                "@font-face {{\n  font-family: \"{}\";\n  src: url(\"data:font/{};base64,{}\") format(\"{}\");\n  font-display: block;\n}}\n",
                family_of_file(&file_name),
                format,
                BASE64.encode(&bytes),
                format,
            ))
        })
        .collect()
}

/// The web build keeps no imported fonts, so it declares none.
#[cfg(target_arch = "wasm32")]
pub fn imported_font_face_css() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole list, installed families included — which a test may wait for
    /// even though a view may not.
    fn all_families() -> Vec<FontFamily> {
        with_installed(system())
    }

    /// The families that can be offered before anything has been read are the
    /// ones that need no reading: a settings page shows them at once instead
    /// of waiting for the computer's font collection.
    #[test]
    fn the_bundled_and_web_safe_families_need_nothing_read() {
        let families = with_installed(Vec::new());

        assert!(
            families.iter().any(|family| family.name == "Arial"),
            "a web-safe family should be offered without reading a single file"
        );
        assert!(
            families
                .iter()
                .all(|family| family.source != FontSource::System),
            "nothing installed can be offered before it has been read"
        );
    }

    /// Reading them ahead of time must not block the caller, and afterwards
    /// they are there for the render that needs them.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_installed_families_are_read_in_the_background() {
        prepare_system_fonts();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while system_fonts_pending() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert!(
            system_ready().is_some(),
            "the installed families were never read"
        );
        assert!(
            available_now()
                .iter()
                .any(|family| family.source == FontSource::System),
            "the selector would still be missing the installed families"
        );

        // Asking again is not work: every font block on the page asks.
        let generation = system_generation();
        prepare_system_fonts();
        assert!(!system_fonts_pending());
        assert_eq!(system_generation(), generation);
    }

    #[test]
    fn test_web_safe_families_are_always_offered() {
        let families = all_families();

        for expected in ["Arial", "Times New Roman", "Courier New"] {
            assert!(
                families.iter().any(|family| family.name == expected),
                "'{expected}' should always be available"
            );
        }
    }

    #[test]
    fn test_no_family_is_listed_twice() {
        let families = all_families();

        let mut names: Vec<String> = families
            .iter()
            .map(|family| family.name.to_lowercase())
            .collect();
        let total = names.len();
        names.sort();
        names.dedup();

        assert_eq!(names.len(), total, "a family is offered more than once");
    }

    /// A bundled font wins over an installed one of the same name, because the
    /// bundled copy is the one that gets used.
    #[test]
    fn test_bundled_fonts_come_first() {
        let families = all_families();
        let bundled_count = bundled().len();

        for family in families.iter().take(bundled_count) {
            assert_eq!(family.source, FontSource::Bundled);
        }
    }

    #[test]
    fn test_font_face_css_is_empty_without_bundled_fonts() {
        let css = bundled_font_face_css();

        if bundled().is_empty() {
            assert!(css.is_empty());
        } else {
            assert!(css.contains("@font-face"));
            assert!(css.contains("/assets/fonts/"));
        }
    }

    /// The desktop build has to find at least something; a machine with no
    /// fonts at all cannot render anything.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_system_fonts_are_found() {
        let system = system();
        assert!(
            !system.is_empty(),
            "no system font was found — is fontconfig missing?"
        );
        assert!(system.iter().all(|family| !family.name.trim().is_empty()));
    }
}
