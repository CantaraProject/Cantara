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

use std::fmt;

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

/// Every family a user can choose, bundled first, then web-safe, then whatever
/// is installed on this computer.
///
/// A name is listed once: a bundled font that happens to share its name with an
/// installed one keeps the bundled entry, because that is the copy Cantara will
/// actually use.
pub fn available() -> Vec<FontFamily> {
    let mut families = bundled();

    families.extend(WEB_SAFE_FAMILIES.iter().map(|name| FontFamily {
        name: (*name).to_string(),
        source: FontSource::WebSafe,
    }));

    families.extend(system());

    let mut seen: Vec<String> = Vec::new();
    families.retain(|family| {
        let key = family.name.to_lowercase();
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_safe_families_are_always_offered() {
        let families = available();

        for expected in ["Arial", "Times New Roman", "Courier New"] {
            assert!(
                families.iter().any(|family| family.name == expected),
                "'{expected}' should always be available"
            );
        }
    }

    #[test]
    fn test_no_family_is_listed_twice() {
        let families = available();

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
        let families = available();
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
