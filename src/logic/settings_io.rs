//! Handing a single design or slide division to somebody else.
//!
//! A running order travels as a [selection file](crate::logic::selection_io).
//! What travels here is one *setting*: the design a congregation has settled
//! on, or the way they divide a song into slides — the things a church builds
//! once and then wants on the second computer, or wants to pass to the
//! neighbouring parish.
//!
//! # The two formats
//!
//! | | Extension | Holds |
//! |---|---|---|
//! | A design | `.cantara-design.zip` | the design and everything it needs to look right |
//! | A slide division | `.cantara-slides.json` | the division |
//!
//! A division is a handful of switches and has nothing else to carry, so it is
//! a plain JSON file that can be read and even edited by hand. A design refers
//! to things outside itself — a background picture, a font that not every
//! computer has — and those have to travel with it, which is why it is an
//! archive:
//!
//! ```text
//! design.json      the design, and what its assets are called
//! assets/…         the background picture and the font files
//! ```
//!
//! Only fonts that would not be there anyway are included: a family Cantara
//! bundles is in every copy of the program, and a web-safe one is on every
//! computer. See [`crate::logic::fonts::FontSource::travels_with_cantara`].
//!
//! # Opening one
//!
//! Both arrive through the same Import button as a selection file — a user has
//! a file and wants Cantara to take it, and which of the four kinds it is, is
//! Cantara's problem rather than theirs. [`read_import`] is where that is
//! decided.

use crate::logic::fonts;
use crate::logic::selection_io::{SelectionDocument, SelectionIoError, read_selection};
use crate::logic::settings::{
    PresentationDesign, Settings, SongSlideSettings,
};
use crate::logic::sourcefiles::SourceFile;
// Used by `import_design`, which writes pictures to a folder, and by the tests.
#[cfg(any(not(target_arch = "wasm32"), test))]
use crate::logic::sourcefiles::{ImageSourceFile, SourceFileType};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The version [`write_design`] writes.
pub const DESIGN_VERSION: u32 = 1;

/// What a design archive calls its manifest.
pub const DESIGN_MANIFEST: &str = "design.json";

/// The folder the design's assets live in, inside the archive.
pub const DESIGN_ASSETS: &str = "assets";

/// What a slide-division file says it is, so that a `.json` file can be told
/// from every other `.json` file in the world.
pub const SLIDE_SETTINGS_FORMAT: &str = "cantara.slide_settings";

/// The extension a design archive is offered under.
pub const DESIGN_EXTENSION: &str = "cantara-design.zip";

/// The extension a slide division is offered under.
pub const SLIDE_SETTINGS_EXTENSION: &str = "cantara-slides.json";

/// `design.json` at the root of a design archive.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DesignManifest {
    /// Always `cantara.design`.
    pub format: String,
    /// See [`DESIGN_VERSION`].
    pub version: u32,
    /// Which Cantara wrote it. For a human reading the file.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created_by: String,
    /// The design itself, exactly as the settings hold it.
    pub design: PresentationDesign,
    /// Where the background picture is inside the archive, if the design has
    /// one — `assets/Backdrop.jpg`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_image: Option<String>,
    /// The font files that travel with the design.
    #[serde(default)]
    pub fonts: Vec<ManifestFont>,
}

/// One font file inside a design archive.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ManifestFont {
    /// The family as the design names it.
    pub family: String,
    /// Where the file is inside the archive — `assets/Open Sans.ttf`.
    pub file: String,
}

/// A slide-division file.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SlideSettingsFile {
    /// Always [`SLIDE_SETTINGS_FORMAT`].
    pub format: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created_by: String,
    /// The division, name and description included.
    pub slide_settings: SongSlideSettings,
}

/// What an opened file turned out to be.
#[derive(Clone, PartialEq, Debug)]
pub enum ImportedFile {
    /// A running order — see [`crate::logic::selection_io`].
    Selection(SelectionDocument),
    /// One design, with whatever it needs to look right.
    Design(Box<DesignPackage>),
    /// One slide division.
    SlideSettings(SongSlideSettings),
}

/// A design as it came out of an archive: the design itself, and the files it
/// refers to, still in memory.
#[derive(Clone, PartialEq, Debug)]
pub struct DesignPackage {
    pub design: PresentationDesign,
    /// The background picture: its name and its bytes.
    pub background_image: Option<(String, Vec<u8>)>,
    /// The fonts, by family: file name and bytes.
    pub fonts: Vec<(String, String, Vec<u8>)>,
}

impl DesignPackage {
    /// The families the archive brought that this computer does not have.
    ///
    /// What the import dialog says out loud, because keeping a font file is
    /// the one part of an import that leaves something behind beyond a
    /// setting.
    pub fn fonts_to_keep(&self) -> Vec<String> {
        let known = fonts::available_now();
        self.fonts
            .iter()
            .filter(|(family, _, _)| {
                !known
                    .iter()
                    .any(|font| font.name.eq_ignore_ascii_case(family))
            })
            .map(|(family, _, _)| family.clone())
            .collect()
    }
}

// ── Writing ──────────────────────────────────────────────────────────────────

/// The design and everything it needs, as an archive.
///
/// `read_asset` reads the background picture from wherever the library keeps
/// it; the font files are looked up on this computer by
/// [`fonts::file_of_family`].
pub fn write_design(
    design: &PresentationDesign,
    read_asset: &dyn Fn(&SourceFile) -> Result<Vec<u8>, String>,
) -> Result<(String, Vec<u8>), SelectionIoError> {
    let mut assets: Vec<(String, Vec<u8>)> = Vec::new();

    // Whatever kind of view the design describes, it is its template that
    // names a background picture — a monitor design has one exactly as an
    // audience design does, and a picture that did not travel with it would
    // arrive as a design pointing at a file on somebody else's computer.
    let template = design.presentation_design_settings.template();

    let background_image = match template.and_then(|template| template.background_image.clone()) {
        Some(picture) => {
            let source = picture.as_source().clone();
            let bytes = read_asset(&source).map_err(|reason| SelectionIoError::Unreadable {
                name: source.name.clone(),
                reason,
            })?;
            let file_name = source.file_name().to_string();
            assets.push((file_name.clone(), bytes));
            Some(format!("{DESIGN_ASSETS}/{file_name}"))
        }
        None => None,
    };

    let mut manifest_fonts: Vec<ManifestFont> = Vec::new();
    for family in families_to_carry(design) {
        // A family this computer cannot produce a file for is left out rather
        // than refused: the design still opens, and CSS falls back to
        // something readable.
        let Some((file_name, bytes)) = fonts::file_of_family(&family) else {
            log::info!("no font file found for {family}; the design travels without it");
            continue;
        };
        if !assets.iter().any(|(name, _)| *name == file_name) {
            assets.push((file_name.clone(), bytes));
        }
        manifest_fonts.push(ManifestFont {
            family,
            file: format!("{DESIGN_ASSETS}/{file_name}"),
        });
    }

    let manifest = DesignManifest {
        format: "cantara.design".to_string(),
        version: DESIGN_VERSION,
        created_by: format!("Cantara {}", env!("CARGO_PKG_VERSION")),
        design: design.clone(),
        background_image,
        fonts: manifest_fonts,
    };

    let json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| SelectionIoError::Malformed(error.to_string()))?;
    let bytes = build_zip(DESIGN_MANIFEST, &json, &assets)?;

    Ok((
        format!("{}.{DESIGN_EXTENSION}", safe_file_name(&design.name)),
        bytes,
    ))
}

/// The families a design uses that would not be on another computer anyway.
fn families_to_carry(design: &PresentationDesign) -> Vec<String> {
    // A monitor design has fonts too, and they have to travel for the same
    // reason an audience design's do: the family it names may not be
    // installed wherever the design is opened.
    let Some(template) = design.presentation_design_settings.template() else {
        return Vec::new();
    };

    let known = fonts::available_now();
    let mut families: Vec<String> = Vec::new();

    for font in &template.fonts {
        let Some(family) = font
            .font_family
            .as_ref()
            .and_then(|family| family.family.clone())
            .filter(|family| !family.trim().is_empty())
        else {
            continue;
        };
        // A family Cantara brings itself, or one every computer has, is
        // already there wherever the design is opened.
        let standard = known
            .iter()
            .any(|font| font.name.eq_ignore_ascii_case(&family) && font.source.travels_with_cantara());
        if standard || families.iter().any(|kept| kept.eq_ignore_ascii_case(&family)) {
            continue;
        }
        families.push(family);
    }

    families
}

/// A slide division as a file.
pub fn write_slide_settings(
    division: &SongSlideSettings,
    index: usize,
) -> Result<(String, Vec<u8>), SelectionIoError> {
    let file = SlideSettingsFile {
        format: SLIDE_SETTINGS_FORMAT.to_string(),
        version: DESIGN_VERSION,
        created_by: format!("Cantara {}", env!("CARGO_PKG_VERSION")),
        slide_settings: division.clone(),
    };

    let bytes = serde_json::to_vec_pretty(&file)
        .map_err(|error| SelectionIoError::Malformed(error.to_string()))?;

    Ok((
        format!(
            "{}.{SLIDE_SETTINGS_EXTENSION}",
            safe_file_name(&division.display_name(index))
        ),
        bytes,
    ))
}

/// A file name that no file system objects to.
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect();
    match cleaned.trim() {
        "" => "Cantara".to_string(),
        trimmed => trimmed.to_string(),
    }
}

fn build_zip(
    manifest_name: &str,
    manifest: &[u8],
    assets: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, SelectionIoError> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut buffer);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut write = |name: &str, bytes: &[u8]| -> Result<(), SelectionIoError> {
            archive
                .start_file(name, options)
                .map_err(|error| SelectionIoError::Archive(error.to_string()))?;
            archive
                .write_all(bytes)
                .map_err(|error| SelectionIoError::Archive(error.to_string()))
        };

        write(manifest_name, manifest)?;
        for (name, bytes) in assets {
            write(&format!("{DESIGN_ASSETS}/{name}"), bytes)?;
        }

        archive
            .finish()
            .map_err(|error| SelectionIoError::Archive(error.to_string()))?;
    }

    Ok(buffer.into_inner())
}

// ── Reading ──────────────────────────────────────────────────────────────────

/// What this file is, and what is in it.
///
/// The user has a file; which of the kinds Cantara reads it happens to be is
/// not their problem. The name is consulted where it says something and the
/// content decides otherwise — a design archive and a selection archive are
/// both ZIP files, and what tells them apart is which manifest is inside.
pub fn read_import(bytes: &[u8], file_name: &str) -> Result<ImportedFile, SelectionIoError> {
    if bytes.starts_with(b"PK") {
        return match archive_holds(bytes, DESIGN_MANIFEST) {
            true => read_design(bytes).map(|package| ImportedFile::Design(Box::new(package))),
            false => read_selection(bytes, file_name).map(ImportedFile::Selection),
        };
    }

    let text = String::from_utf8_lossy(bytes);
    if text.trim_start().starts_with('{')
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        && value.get("format").and_then(|format| format.as_str()) == Some(SLIDE_SETTINGS_FORMAT)
    {
        let file: SlideSettingsFile = serde_json::from_str(&text)
            .map_err(|error| SelectionIoError::Malformed(error.to_string()))?;
        return Ok(ImportedFile::SlideSettings(file.slide_settings));
    }

    read_selection(bytes, file_name).map(ImportedFile::Selection)
}

/// Whether the archive holds an entry of that name.
fn archive_holds(bytes: &[u8], name: &str) -> bool {
    zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec()))
        .map(|mut archive| archive.by_name(name).is_ok())
        .unwrap_or(false)
}

/// Reads a design archive.
pub fn read_design(bytes: &[u8]) -> Result<DesignPackage, SelectionIoError> {
    use std::io::Read;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec()))
        .map_err(|error| SelectionIoError::Archive(error.to_string()))?;

    let manifest: DesignManifest = {
        let mut file = archive
            .by_name(DESIGN_MANIFEST)
            .map_err(|_| SelectionIoError::Malformed(format!("no {DESIGN_MANIFEST}")))?;
        let mut json = String::new();
        file.read_to_string(&mut json)
            .map_err(|error| SelectionIoError::Archive(error.to_string()))?;
        serde_json::from_str(&json)
            .map_err(|error| SelectionIoError::Malformed(error.to_string()))?
    };

    if manifest.version > DESIGN_VERSION {
        return Err(SelectionIoError::TooNew {
            found: manifest.version,
            supported: DESIGN_VERSION,
        });
    }

    let mut read_entry = |path: &str| -> Result<(String, Vec<u8>), SelectionIoError> {
        let mut file = archive.by_name(path).map_err(|_| {
            SelectionIoError::Malformed(format!("{path} is named but not in the archive"))
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| SelectionIoError::Archive(error.to_string()))?;
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        Ok((name, bytes))
    };

    let background_image = match &manifest.background_image {
        Some(path) => Some(read_entry(path)?),
        None => None,
    };

    let mut fonts = Vec::with_capacity(manifest.fonts.len());
    for font in &manifest.fonts {
        let (name, bytes) = read_entry(&font.file)?;
        fonts.push((font.family.clone(), name, bytes));
    }

    Ok(DesignPackage {
        design: manifest.design,
        background_image,
        fonts,
    })
}

// ── Keeping what was read ────────────────────────────────────────────────────

/// What came of importing a design.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct DesignImportOutcome {
    /// The families whose files were kept, so the design looks right here.
    pub fonts_kept: Vec<String>,
    /// Where the background picture was written, if there was one.
    pub background_image: Option<PathBuf>,
    /// Whether the design was added to the settings, or was already there.
    pub added: bool,
}

/// Adds an imported design to the settings, with its assets.
///
/// The background picture goes into `picture_folder` — a repository the user
/// picked, so that it is also a picture they can use elsewhere — and the fonts
/// into Cantara's own folder. The design's own copy of the picture path is
/// re-pointed at what was written, since the path it arrived with is a path on
/// somebody else's computer.
#[cfg(not(target_arch = "wasm32"))]
pub fn import_design(
    settings: &mut Settings,
    package: &DesignPackage,
    picture_folder: &std::path::Path,
) -> Result<DesignImportOutcome, SelectionIoError> {
    let mut outcome = DesignImportOutcome::default();
    let mut design = package.design.clone();

    if let Some((file_name, bytes)) = &package.background_image {
        let path = write_beside(picture_folder, file_name, bytes)?;
        if let Some(template) = design.presentation_design_settings.template_mut()
            && let Some(picture) = ImageSourceFile::new(SourceFile {
                name: SourceFileType::display_name(file_name),
                file_type: SourceFileType::Image,
                md5_hash: None,
                relative_path: None,
                path: path.clone(),
            })
        {
            template.background_image = Some(picture);
        }
        outcome.background_image = Some(path);
    }

    for (family, file_name, bytes) in &package.fonts {
        match fonts::install(file_name, bytes) {
            Ok(_) => outcome.fonts_kept.push(family.clone()),
            // A font that cannot be kept is not a reason to lose the design:
            // it is shown in whatever the page falls back to, and the user is
            // told which family that was.
            Err(reason) => log::warn!("{family} could not be kept: {reason}"),
        }
    }

    if !settings.presentation_designs.contains(&design) {
        settings.presentation_designs.push(design);
        settings.ensure_slide_settings_for_designs();
        outcome.added = true;
    }

    Ok(outcome)
}

/// Writes a file into `folder` without overwriting anything, exactly as an
/// imported song is written — see [`crate::logic::selection_io`].
#[cfg(not(target_arch = "wasm32"))]
fn write_beside(
    folder: &std::path::Path,
    file_name: &str,
    content: &[u8],
) -> Result<PathBuf, SelectionIoError> {
    std::fs::create_dir_all(folder).map_err(|error| SelectionIoError::Unreadable {
        name: folder.display().to_string(),
        reason: error.to_string(),
    })?;

    let (stem, suffix) = match file_name.rsplit_once('.') {
        Some((stem, suffix)) => (stem.to_string(), format!(".{suffix}")),
        None => (file_name.to_string(), String::new()),
    };

    for attempt in 0..1000 {
        let candidate = match attempt {
            0 => folder.join(file_name),
            _ => folder.join(format!("{stem} ({}){suffix}", attempt + 1)),
        };
        match std::fs::read(&candidate) {
            Ok(existing) if existing == content => return Ok(candidate),
            Ok(_) => continue,
            Err(_) => {
                std::fs::write(&candidate, content).map_err(|error| {
                    SelectionIoError::Unreadable {
                        name: candidate.display().to_string(),
                        reason: error.to_string(),
                    }
                })?;
                return Ok(candidate);
            }
        }
    }

    Err(SelectionIoError::Unreadable {
        name: file_name.to_string(),
        reason: "no free file name".to_string(),
    })
}

/// The web build has no file system, so a design arrives without its assets:
/// the design is kept, the picture and the fonts are not.
#[cfg(target_arch = "wasm32")]
pub fn import_design(
    settings: &mut Settings,
    package: &DesignPackage,
    _picture_folder: &std::path::Path,
) -> Result<DesignImportOutcome, SelectionIoError> {
    let mut outcome = DesignImportOutcome::default();
    if !settings.presentation_designs.contains(&package.design) {
        settings.presentation_designs.push(package.design.clone());
        settings.ensure_slide_settings_for_designs();
        outcome.added = true;
    }
    Ok(outcome)
}

/// Adds an imported slide division to the settings, unless it is there already.
///
/// Compared by the division itself rather than by its name: the same switches
/// under two names are one division, and a second copy would only make the
/// list longer.
pub fn import_slide_settings(settings: &mut Settings, division: &SongSlideSettings) -> bool {
    if settings
        .song_slide_settings
        .iter()
        .any(|known| known.settings == division.settings)
    {
        return false;
    }
    settings.song_slide_settings.push(division.clone());
    true
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    reason = "these design structs keep private fields, so `..Default::default()`               is not available outside the module that defines them"
)]
mod tests {
    use super::*;
    use crate::logic::css::CssFontFamily;
    use crate::logic::settings::{
        FontRepresentation, PresentationDesignSettings, PresentationDesignTemplate,
    };
    use cantara_songlib::slides::SlideSettings;

    fn design_with_font(family: &str) -> PresentationDesign {
        let mut template = PresentationDesignTemplate::default();
        let mut font = FontRepresentation::default();
        font.font_family = Some(CssFontFamily::with_family(family.to_string()));
        template.fonts = vec![font];

        PresentationDesign {
            name: "Dark".to_string(),
            description: "For the evening service".to_string(),
            presentation_design_settings: PresentationDesignSettings::Template(template),
        }
    }

    fn unreadable(_: &SourceFile) -> Result<Vec<u8>, String> {
        Err("nothing to read".to_string())
    }

    /// A design has to come back out of its archive exactly as it went in —
    /// that is the whole point of handing one to a colleague.
    #[test]
    fn a_design_survives_a_round_trip() {
        let design = design_with_font("Arial");

        let (name, bytes) = write_design(&design, &unreadable).expect("written");

        assert_eq!(name, "Dark.cantara-design.zip");
        assert!(bytes.starts_with(b"PK"));

        let package = read_design(&bytes).expect("read");
        assert_eq!(package.design, design);
        assert_eq!(package.background_image, None);
    }

    /// The archive is what the documentation says it is, and other programs
    /// are invited to read it.
    #[test]
    fn the_design_archive_is_a_manifest_and_a_folder() {
        let (_, bytes) = write_design(&design_with_font("Arial"), &unreadable).expect("written");

        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).expect("an archive");
        let names: Vec<String> = (0..archive.len())
            .filter_map(|index| archive.by_index(index).ok().map(|f| f.name().to_string()))
            .collect();
        assert!(names.contains(&DESIGN_MANIFEST.to_string()), "{names:?}");

        let manifest: serde_json::Value = {
            let mut file = archive.by_name(DESIGN_MANIFEST).expect("the manifest");
            let mut json = String::new();
            std::io::Read::read_to_string(&mut file, &mut json).expect("readable");
            serde_json::from_str(&json).expect("valid JSON")
        };
        assert_eq!(manifest["format"], "cantara.design");
        assert_eq!(manifest["version"], DESIGN_VERSION);
        assert_eq!(manifest["design"]["name"], "Dark");
    }

    /// A family every Cantara has needs no file: putting a megabyte of Arial
    /// into every archive would help nobody.
    #[test]
    fn a_standard_family_does_not_travel() {
        let (_, bytes) = write_design(&design_with_font("Arial"), &unreadable).expect("written");
        let package = read_design(&bytes).expect("read");

        assert!(
            package.fonts.is_empty(),
            "a web-safe family was put into the archive: {:?}",
            package.fonts
        );
    }

    /// The background picture travels with the design, since a design without
    /// it is not the design that was handed over.
    #[test]
    fn the_background_picture_travels_with_the_design() {
        let picture = ImageSourceFile::new(SourceFile {
            name: "Backdrop".to_string(),
            path: PathBuf::from("/pictures/Backdrop.jpg"),
            file_type: SourceFileType::Image,
            md5_hash: None,
            relative_path: None,
        })
        .expect("a picture");

        let mut template = PresentationDesignTemplate::default();
        template.background_image = Some(picture);
        let design = PresentationDesign {
            name: "Dark".to_string(),
            description: String::new(),
            presentation_design_settings: PresentationDesignSettings::Template(template),
        };

        let (_, bytes) = write_design(&design, &|_: &SourceFile| Ok(b"a picture".to_vec()))
            .expect("written");
        let package = read_design(&bytes).expect("read");

        assert_eq!(
            package.background_image,
            Some(("Backdrop.jpg".to_string(), b"a picture".to_vec()))
        );
    }

    /// A picture that cannot be read has to stop the export with a reason: an
    /// archive that quietly lost the background is worse than none.
    #[test]
    fn a_picture_that_cannot_be_read_stops_the_export() {
        let picture = ImageSourceFile::new(SourceFile {
            name: "Backdrop".to_string(),
            path: PathBuf::from("/pictures/Backdrop.jpg"),
            file_type: SourceFileType::Image,
            md5_hash: None,
            relative_path: None,
        })
        .expect("a picture");
        let mut template = PresentationDesignTemplate::default();
        template.background_image = Some(picture);
        let design = PresentationDesign {
            name: "Dark".to_string(),
            description: String::new(),
            presentation_design_settings: PresentationDesignSettings::Template(template),
        };

        assert!(matches!(
            write_design(&design, &unreadable),
            Err(SelectionIoError::Unreadable { .. })
        ));
    }

    /// A slide division is a file a person can read, and it comes back as what
    /// it was — name, description and all.
    #[test]
    fn a_slide_division_survives_a_round_trip() {
        let division = SongSlideSettings {
            name: "Two languages".to_string(),
            description: "For the international service".to_string(),
            settings: SlideSettings {
                max_lines: Some(4),
                ..SlideSettings::default()
            },
        };

        let (name, bytes) = write_slide_settings(&division, 0).expect("written");
        assert_eq!(name, "Two languages.cantara-slides.json");

        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
        assert_eq!(value["format"], SLIDE_SETTINGS_FORMAT);
        // Flattened, so the division reads as itself rather than as a nested
        // object nobody asked for.
        assert_eq!(value["slide_settings"]["name"], "Two languages");
        assert_eq!(value["slide_settings"]["max_lines"], 4);

        match read_import(&bytes, &name).expect("read") {
            ImportedFile::SlideSettings(read) => assert_eq!(read, division),
            other => panic!("read as {other:?}"),
        }
    }

    /// One button opens all four kinds of file, so the reader has to tell them
    /// apart on its own.
    #[test]
    fn each_kind_of_file_is_recognised() {
        let (design_name, design) = write_design(&design_with_font("Arial"), &unreadable)
            .expect("a design archive");
        let (division_name, division) =
            write_slide_settings(&SongSlideSettings::default(), 0).expect("a division");

        assert!(matches!(
            read_import(&design, &design_name),
            Ok(ImportedFile::Design(_))
        ));
        assert!(matches!(
            read_import(&division, &division_name),
            Ok(ImportedFile::SlideSettings(_))
        ));

        // A selection still reads as a selection, both as an archive and as
        // the text formats.
        let songtex = b"\\beginfile{Amazing Grace.song}\nAmazing grace\n\\endfile\n";
        assert!(matches!(
            read_import(songtex, "service.songtex"),
            Ok(ImportedFile::Selection(_))
        ));

        // And something that is none of them says so.
        assert!(read_import(b"hello", "notes.txt").is_err());
    }

    /// A design already in the settings is not added twice — importing the
    /// same archive on two evenings must not leave two identical designs.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn importing_the_same_design_twice_adds_it_once() {
        let folder = tempfile::tempdir().expect("a folder");
        let mut settings = Settings::default();
        let package = DesignPackage {
            design: design_with_font("Arial"),
            background_image: None,
            fonts: vec![],
        };

        let first = import_design(&mut settings, &package, folder.path()).expect("imported");
        let second = import_design(&mut settings, &package, folder.path()).expect("imported");

        assert!(first.added);
        assert!(!second.added);
        assert_eq!(
            settings
                .presentation_designs
                .iter()
                .filter(|design| design.name == "Dark")
                .count(),
            1
        );
    }

    /// The picture arrives with a path from somebody else's computer, so the
    /// design has to be re-pointed at the copy that was just written — else it
    /// shows nothing at all.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn an_imported_background_points_at_the_file_that_was_written() {
        let folder = tempfile::tempdir().expect("a folder");
        let mut settings = Settings::default();
        let package = DesignPackage {
            design: design_with_font("Arial"),
            background_image: Some(("Backdrop.jpg".to_string(), b"a picture".to_vec())),
            fonts: vec![],
        };

        let outcome = import_design(&mut settings, &package, folder.path()).expect("imported");

        let written = outcome.background_image.expect("the picture was written");
        assert_eq!(std::fs::read(&written).expect("readable"), b"a picture");

        let imported = settings.presentation_designs.last().expect("the design");
        let template = imported
            .presentation_design_settings
            .template()
            .expect("the imported design carries a template");
        assert_eq!(
            template
                .background_image
                .as_ref()
                .map(|picture| picture.as_source().path.clone()),
            Some(written)
        );
    }

    /// Two divisions that differ only in their name are one division: what is
    /// compared is what they do.
    #[test]
    fn a_division_that_is_already_there_is_not_added_again() {
        let mut settings = Settings::default();
        let known = settings.song_slide_settings[0].settings.clone();

        let under_another_name = SongSlideSettings {
            name: "Somebody else's name".to_string(),
            description: String::new(),
            settings: known,
        };
        assert!(!import_slide_settings(&mut settings, &under_another_name));

        let different = SongSlideSettings {
            name: "Four lines".to_string(),
            description: String::new(),
            settings: SlideSettings {
                max_lines: Some(4),
                ..SlideSettings::default()
            },
        };
        assert!(import_slide_settings(&mut settings, &different));
        assert_eq!(settings.song_slide_settings.len(), 2);
    }

    /// A name is not a file name: a design called "Advent / Christmas" must
    /// still produce a file that can be saved.
    #[test]
    fn a_name_becomes_a_usable_file_name() {
        assert_eq!(safe_file_name("Advent / Christmas"), "Advent - Christmas");
        assert_eq!(safe_file_name("  "), "Cantara");
        assert_eq!(safe_file_name("Dark"), "Dark");
    }
}
