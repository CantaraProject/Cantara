//! Writing a selection to a file and reading one back.
//!
//! A *selection* is the running order of a service: which elements are shown,
//! in which order, and what each of them is shown with. Cantara keeps it in
//! memory while the program runs; this module is what makes it a file — to
//! hand to a colleague, to prepare at home and open in the hall, or simply to
//! have next Sunday.
//!
//! # The formats
//!
//! | Format | Extension | Reads | Writes | Carries |
//! |---|---|---|---|---|
//! | [`SelectionFormat::CantaraZip`] | `.cantara.zip` | ✓ | ✓ | everything |
//! | [`SelectionFormat::Songtex`] | `.songtex` | ✓ | ✓ | songs and their order |
//! | [`SelectionFormat::CantaraJson`] | `.json` | ✓ | ✓ | songs, their order, a little of their look |
//!
//! The latter two are Cantara 2's, and they are here so that a selection can
//! travel between the two programs. Both know only songs: a picture, a PDF or
//! a piece of Markdown in the running order is left out of them, and Cantara 2
//! has nothing to put a slide division or a stream design into. Where that
//! matters, `.cantara.zip` is the format to use — it is Cantara 3's own and
//! loses nothing.
//!
//! # What `.cantara.zip` looks like
//!
//! An ordinary ZIP archive with a manifest and a folder of files:
//!
//! ```text
//! selection.json      the running order and everything about it
//! assets/…            the elements themselves, under their own names
//! ```
//!
//! `selection.json` names its assets by their path inside the archive, so the
//! archive can be opened with any ZIP tool and read without Cantara. The
//! designs and slide divisions the selection uses are part of the manifest
//! rather than of the program's settings, which is what makes the file
//! self-contained: opening it on another computer shows what the author saw.
//!
//! The full description, field by field, is in `docs/formats/cantara-zip.md`.
//!
//! # Reading is two steps
//!
//! [`read_selection`] turns bytes into a [`SelectionDocument`] and touches
//! nothing else — no file system, no settings, no repositories. Deciding where
//! the elements of that document come from is [`resolve_selection`], which
//! needs the library and may write into it. Keeping them apart is what makes
//! the formats testable without a disk, and it is also what lets the user be
//! asked before anything of theirs is written to.

use crate::logic::settings::{PresentationDesign, SlideTimerSettings, SlideTransition};
use crate::logic::sourcefiles::{SourceFile, SourceFileType};
use crate::logic::states::{SelectedItemRepresentation, VideoSettings};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use cantara_songlib::slides::SlideSettings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The version [`SelectionFormat::CantaraZip`] writes.
///
/// A reader accepts anything up to this. The number goes up when a change
/// would make an archive unreadable to an older Cantara — adding a field that
/// may be absent does not, since serde fills those in.
pub const CANTARA_ZIP_VERSION: u32 = 1;

/// The manifest's name inside the archive.
pub const MANIFEST_NAME: &str = "selection.json";

/// The folder the elements themselves live in, inside the archive.
pub const ASSET_FOLDER: &str = "assets";

/// The header Cantara 2 writes above a `.songtex` file, and which Cantara 3
/// writes so that the file is recognisably the same thing.
const SONGTEX_HEADER: &str = "% This file has been created automatically\n\
     % It can be opened with Cantara (https://cantara.app)\n\
     % Manually editing the content may damage the import\n";

/// A file the selection can be written to and read from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectionFormat {
    /// Cantara 3's own: the whole selection, its designs and its elements.
    CantaraZip,
    /// Cantara 2's song collection: the songs and their order.
    Songtex,
    /// Cantara 2's selection file: the songs, their order and a little of
    /// their look.
    CantaraJson,
}

impl SelectionFormat {
    /// Every format, in the order the menu offers them.
    pub const ALL: &'static [SelectionFormat] = &[
        SelectionFormat::CantaraZip,
        SelectionFormat::Songtex,
        SelectionFormat::CantaraJson,
    ];

    /// The value used in a `<select>`, and what the settings persist.
    pub fn id(self) -> &'static str {
        match self {
            SelectionFormat::CantaraZip => "cantara_zip",
            SelectionFormat::Songtex => "songtex",
            SelectionFormat::CantaraJson => "cantara_json",
        }
    }

    /// The format of that id, if it is one.
    pub fn of_id(id: &str) -> Option<SelectionFormat> {
        SelectionFormat::ALL
            .iter()
            .copied()
            .find(|format| format.id() == id)
    }

    /// The translation key of the format's label.
    pub fn label_key(self) -> &'static str {
        match self {
            SelectionFormat::CantaraZip => "selection.selection_format_cantara_zip",
            SelectionFormat::Songtex => "selection.selection_format_songtex",
            SelectionFormat::CantaraJson => "selection.selection_format_cantara_json",
        }
    }

    /// What the file is called, without its name.
    ///
    /// `.cantara.zip` is a double extension on purpose: it stays a ZIP file to
    /// everything that handles ZIP files, and still says whose it is.
    pub fn extension(self) -> &'static str {
        match self {
            SelectionFormat::CantaraZip => "cantara.zip",
            SelectionFormat::Songtex => "songtex",
            SelectionFormat::CantaraJson => "json",
        }
    }

    /// The format a file of this name is in, going by its extension.
    ///
    /// A `.json` file could be anything, so it is only offered to the Cantara 2
    /// reader, which says for itself whether it recognises the content.
    pub fn of_file_name(file_name: &str) -> Option<SelectionFormat> {
        let lower = file_name.to_lowercase();
        if lower.ends_with(".cantara.zip") || lower.ends_with(".zip") {
            Some(SelectionFormat::CantaraZip)
        } else if lower.ends_with(".songtex") {
            Some(SelectionFormat::Songtex)
        } else if lower.ends_with(".json") {
            Some(SelectionFormat::CantaraJson)
        } else {
            None
        }
    }

    /// Whether the format can hold anything but songs.
    ///
    /// Both Cantara 2 formats are lists of song files; a picture or a PDF in
    /// the running order has nowhere to go in them and is left out.
    pub fn holds_only_songs(self) -> bool {
        !matches!(self, SelectionFormat::CantaraZip)
    }
}

/// Why a selection could not be written or read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SelectionIoError {
    /// The selection holds nothing this format can carry.
    Empty,
    /// An element could not be read from the library.
    Unreadable { name: String, reason: String },
    /// The file is not what it claims to be.
    Malformed(String),
    /// The file was written by a later Cantara.
    TooNew { found: u32, supported: u32 },
    /// Building or unpacking the archive itself failed.
    Archive(String),
}

impl SelectionIoError {
    /// The key of the message shown to the user, with its parameters.
    ///
    /// Kept here rather than in the view so that a new error cannot be added
    /// without a message: the match is exhaustive.
    pub fn message_key(&self) -> (&'static str, Vec<(&'static str, String)>) {
        match self {
            SelectionIoError::Empty => ("selection.import_error_empty", vec![]),
            SelectionIoError::Unreadable { name, reason } => (
                "selection.export_error_unreadable",
                vec![("name", name.clone()), ("reason", reason.clone())],
            ),
            SelectionIoError::Malformed(reason) => (
                "selection.import_error_malformed",
                vec![("reason", reason.clone())],
            ),
            SelectionIoError::TooNew { found, supported } => (
                "selection.import_error_too_new",
                vec![
                    ("found", found.to_string()),
                    ("supported", supported.to_string()),
                ],
            ),
            SelectionIoError::Archive(reason) => (
                "selection.import_error_archive",
                vec![("reason", reason.clone())],
            ),
        }
    }
}

/// A written selection, ready to be put somewhere.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SelectionFile {
    /// The suggested file name, extension included.
    pub name: String,
    pub bytes: Vec<u8>,
}

// ── The manifest ─────────────────────────────────────────────────────────────
//
// These are the types `selection.json` is made of. They are deliberately their
// own types rather than the program's: what a running order *is* may change
// with the program, and a file written last year has to keep working. Every
// field that can be absent is, so that an older archive still reads.

/// `selection.json` at the root of a `.cantara.zip`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SelectionManifest {
    /// Always `cantara.selection`, so that a JSON file can say what it is.
    pub format: String,
    /// See [`CANTARA_ZIP_VERSION`].
    pub version: u32,
    /// Which Cantara wrote it. For a human reading the file; nothing depends
    /// on it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created_by: String,
    /// The designs the elements refer to, in the order they are referred to.
    #[serde(default)]
    pub designs: Vec<PresentationDesign>,
    /// The slide divisions the elements refer to.
    #[serde(default)]
    pub slide_settings: Vec<SlideSettings>,
    /// The running order itself.
    pub items: Vec<ManifestItem>,
}

/// One element of the running order.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ManifestItem {
    /// Where the element is inside the archive — `assets/Amazing Grace.song`.
    ///
    /// Absent for an element that is not a file at all: a piece of Markdown
    /// typed into the program carries its text in
    /// [`inline_markdown`](Self::inline_markdown) instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// What the element is called in the running order.
    pub name: String,
    /// The fingerprint of the file, so that the same element already in the
    /// library can be recognised even under a different name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    /// Which of the manifest's [`designs`](SelectionManifest::designs) this
    /// element is shown with. Absent means the one the opening Cantara uses
    /// generally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<usize>,
    /// The same for the slide division.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_settings: Option<usize>,
    /// What the network stream shows instead, where that differs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_design: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_slide_settings: Option<usize>,
    /// Markdown typed into the program rather than read from a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_markdown: Option<String>,
    /// The automatic advance, where the element has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer: Option<SlideTimerSettings>,
    /// How the slides of this element arrive.
    #[serde(default)]
    pub transition: SlideTransition,
    /// Which pages of a PDF to show — `1-3+6`. Left out when it is every page,
    /// which is what a file written before this existed also means.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pdf_pages: String,
    /// How a video is played. Left out when it is the ordinary case, which is
    /// also what a running order written before videos existed means.
    #[serde(default, skip_serializing_if = "is_default_video_settings")]
    pub video_settings: VideoSettings,
}

/// Whether these are the settings a video gets when nothing was said about it.
///
/// Used to keep them out of a written running order, so that a file only
/// mentions what the service actually chose.
fn is_default_video_settings(settings: &VideoSettings) -> bool {
    *settings == VideoSettings::default()
}

/// What a selection file was found to contain.
///
/// Everything is here as it was in the file: nothing has been looked up in the
/// library and nothing has been written. [`resolve_selection`] does that.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SelectionDocument {
    pub items: Vec<DocumentItem>,
    /// The designs the file brought with it, referred to by position.
    pub designs: Vec<PresentationDesign>,
    /// The slide divisions the file brought with it.
    pub slide_settings: Vec<SlideSettings>,
}

/// One element of a selection that has been read but not yet resolved.
#[derive(Clone, PartialEq, Debug)]
pub struct DocumentItem {
    /// The file name the element had — `Amazing Grace.song`.
    pub file_name: String,
    /// The display name.
    pub name: String,
    /// The element's own bytes, where the file carried them. A `.songtex` and
    /// a `.cantara.zip` always do; nothing else has to.
    pub content: Option<Vec<u8>>,
    pub md5: Option<String>,
    pub design: Option<PresentationDesign>,
    pub slide_settings: Option<SlideSettings>,
    pub stream_design: Option<PresentationDesign>,
    pub stream_slide_settings: Option<SlideSettings>,
    pub inline_markdown: Option<String>,
    pub timer: Option<SlideTimerSettings>,
    pub transition: SlideTransition,
    /// Which pages of a PDF to show, as the user wrote it.
    pub pdf_pages: String,
    /// How a video is played.
    pub video_settings: VideoSettings,
}

impl DocumentItem {
    /// A bare element of that name, with nothing set on it.
    fn of_file(file_name: &str, content: Vec<u8>) -> DocumentItem {
        DocumentItem {
            file_name: file_name.to_string(),
            name: SourceFileType::display_name(file_name),
            md5: Some(fingerprint_of(&content)),
            content: Some(content),
            design: None,
            slide_settings: None,
            stream_design: None,
            stream_slide_settings: None,
            inline_markdown: None,
            timer: None,
            transition: SlideTransition::default(),
            pdf_pages: String::new(),
            video_settings: VideoSettings::default(),
        }
    }
}

/// The fingerprint of some bytes, in the form
/// [`SourceFile::md5_hash`](crate::logic::sourcefiles::SourceFile::md5_hash)
/// uses — which is what makes the two comparable.
pub fn fingerprint_of(bytes: &[u8]) -> String {
    format!("{:x}", md5::compute(bytes))
}

// ── Writing ──────────────────────────────────────────────────────────────────

/// Writes `items` as a file in `format`.
///
/// `name` is the file name without an extension; the caller usually offers the
/// user something like "Service 24 August".
pub fn write_selection(
    items: &[SelectedItemRepresentation],
    format: SelectionFormat,
    name: &str,
    read_asset: &dyn Fn(&SourceFile) -> Result<Vec<u8>, String>,
) -> Result<SelectionFile, SelectionIoError> {
    let bytes = match format {
        SelectionFormat::CantaraZip => write_cantara_zip(items, read_asset)?,
        SelectionFormat::Songtex => write_songtex(items, read_asset)?.into_bytes(),
        SelectionFormat::CantaraJson => write_cantara_json(items, read_asset)?.into_bytes(),
    };

    Ok(SelectionFile {
        name: format!("{name}.{}", format.extension()),
        bytes,
    })
}

/// The elements a Cantara 2 format can carry.
fn songs_of(items: &[SelectedItemRepresentation]) -> Vec<&SelectedItemRepresentation> {
    items
        .iter()
        .filter(|item| item.source_file.file_type == SourceFileType::Song)
        .collect()
}

fn write_songtex(
    items: &[SelectedItemRepresentation],
    read_asset: &dyn Fn(&SourceFile) -> Result<Vec<u8>, String>,
) -> Result<String, SelectionIoError> {
    let songs = songs_of(items);
    if songs.is_empty() {
        return Err(SelectionIoError::Empty);
    }

    let mut out = String::from(SONGTEX_HEADER);
    for item in songs {
        let content = read_item(item, read_asset)?;
        let text = String::from_utf8_lossy(&content);
        out.push_str(&format!("\\beginfile{{{}}}\n", item.source_file.file_name()));
        out.push_str(text.trim_end_matches('\n'));
        out.push_str("\n\\endfile\n");
    }
    Ok(out)
}

fn write_cantara_json(
    items: &[SelectedItemRepresentation],
    read_asset: &dyn Fn(&SourceFile) -> Result<Vec<u8>, String>,
) -> Result<String, SelectionIoError> {
    let songs = songs_of(items);
    if songs.is_empty() {
        return Err(SelectionIoError::Empty);
    }

    let mut entries: Vec<serde_json::Value> = Vec::with_capacity(songs.len());
    for item in songs {
        let content = read_item(item, read_asset)?;
        entries.push(serde_json::json!({
            "file_name": item.source_file.file_name(),
            "file_content": BASE64.encode(&content),
            // Cantara 2's style object describes a font and a colour, which is
            // not what a Cantara 3 design is. Rather than invent a mapping
            // that would come back as something nobody chose, every song is
            // written as "default" and keeps its look on this side.
            "style_setting": "default",
            "background_image": serde_json::Value::Null,
        }));
    }

    let document = serde_json::json!({ "version": 1, "songs": entries });
    serde_json::to_string_pretty(&document)
        .map_err(|error| SelectionIoError::Malformed(error.to_string()))
}

fn write_cantara_zip(
    items: &[SelectedItemRepresentation],
    read_asset: &dyn Fn(&SourceFile) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, SelectionIoError> {
    if items.is_empty() {
        return Err(SelectionIoError::Empty);
    }

    // The designs and divisions are collected as they are met and referred to
    // by position, so that ten songs sharing a design store it once.
    let mut designs: Vec<PresentationDesign> = Vec::new();
    let mut slide_settings: Vec<SlideSettings> = Vec::new();
    let mut manifest_items: Vec<ManifestItem> = Vec::new();
    let mut assets: Vec<(String, Vec<u8>)> = Vec::new();

    for item in items {
        // Two elements may well be the same file — a song sung twice in one
        // service — and it is stored once.
        let file = match item.inline_markdown {
            Some(_) => None,
            None => {
                let name = unique_asset_name(&assets, item.source_file.file_name());
                let content = read_item(item, read_asset)?;
                match assets.iter().find(|(_, bytes)| *bytes == content) {
                    Some((existing, _)) => Some(existing.clone()),
                    None => {
                        assets.push((name.clone(), content));
                        Some(name)
                    }
                }
            }
        };

        let md5 = file.as_ref().and_then(|name| {
            assets
                .iter()
                .find(|(asset, _)| asset == name)
                .map(|(_, bytes)| fingerprint_of(bytes))
        });

        manifest_items.push(ManifestItem {
            file: file.map(|name| format!("{ASSET_FOLDER}/{name}")),
            name: item.source_file.name.clone(),
            md5,
            design: item
                .presentation_design_option
                .clone()
                .map(|design| position_of(&mut designs, design)),
            slide_settings: item
                .slide_settings_option
                .clone()
                .map(|settings| position_of(&mut slide_settings, settings)),
            stream_design: item
                .stream_design_option
                .clone()
                .map(|design| position_of(&mut designs, design)),
            stream_slide_settings: item
                .stream_slide_settings_option
                .clone()
                .map(|settings| position_of(&mut slide_settings, settings)),
            inline_markdown: item.inline_markdown.clone(),
            timer: item.timer_settings_option.clone(),
            transition: item.transition_effect,
            pdf_pages: item.pdf_pages.clone(),
            video_settings: item.video_settings,
        });
    }

    let manifest = SelectionManifest {
        format: "cantara.selection".to_string(),
        version: CANTARA_ZIP_VERSION,
        created_by: format!("Cantara {}", env!("CARGO_PKG_VERSION")),
        designs,
        slide_settings,
        items: manifest_items,
    };

    build_zip(&manifest, &assets)
}

/// The position of `value` in `list`, adding it if it is not there yet.
fn position_of<T: PartialEq>(list: &mut Vec<T>, value: T) -> usize {
    match list.iter().position(|existing| *existing == value) {
        Some(index) => index,
        None => {
            list.push(value);
            list.len() - 1
        }
    }
}

/// A name for an asset that no other asset in the archive has.
fn unique_asset_name(assets: &[(String, Vec<u8>)], wanted: &str) -> String {
    if !assets.iter().any(|(name, _)| name == wanted) {
        return wanted.to_string();
    }

    let (stem, suffix) = match wanted.split_once('.') {
        Some((stem, suffix)) => (stem, format!(".{suffix}")),
        None => (wanted, String::new()),
    };
    (2..)
        .map(|number| format!("{stem} ({number}){suffix}"))
        .find(|candidate| !assets.iter().any(|(name, _)| name == candidate))
        .unwrap_or_else(|| wanted.to_string())
}

fn read_item(
    item: &SelectedItemRepresentation,
    read_asset: &dyn Fn(&SourceFile) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, SelectionIoError> {
    if let Some(markdown) = &item.inline_markdown {
        return Ok(markdown.clone().into_bytes());
    }
    read_asset(&item.source_file).map_err(|reason| SelectionIoError::Unreadable {
        name: item.source_file.name.clone(),
        reason,
    })
}

fn build_zip(
    manifest: &SelectionManifest,
    assets: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, SelectionIoError> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let manifest_json = serde_json::to_vec_pretty(manifest)
        .map_err(|error| SelectionIoError::Malformed(error.to_string()))?;

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

        write(MANIFEST_NAME, &manifest_json)?;
        for (name, bytes) in assets {
            write(&format!("{ASSET_FOLDER}/{name}"), bytes)?;
        }

        archive
            .finish()
            .map_err(|error| SelectionIoError::Archive(error.to_string()))?;
    }

    Ok(buffer.into_inner())
}

// ── Reading ──────────────────────────────────────────────────────────────────

/// Reads a selection file, whatever of the three it is.
///
/// The format is taken from the file name where it says something, and from
/// the content otherwise — a file that has been renamed still opens.
pub fn read_selection(
    bytes: &[u8],
    file_name: &str,
) -> Result<SelectionDocument, SelectionIoError> {
    match SelectionFormat::of_file_name(file_name) {
        Some(SelectionFormat::CantaraZip) => read_cantara_zip(bytes),
        Some(SelectionFormat::Songtex) => read_songtex(&text_of(bytes)),
        Some(SelectionFormat::CantaraJson) => read_cantara_json(&text_of(bytes)),
        None => read_by_content(bytes),
    }
}

/// What a file is, going by what is in it.
///
/// A ZIP archive begins with `PK`; anything else is text here, and a selection
/// in text is either JSON or the TeX-like `.songtex`.
fn read_by_content(bytes: &[u8]) -> Result<SelectionDocument, SelectionIoError> {
    if bytes.starts_with(b"PK") {
        return read_cantara_zip(bytes);
    }
    let text = text_of(bytes);
    if text.trim_start().starts_with('{') {
        read_cantara_json(&text)
    } else {
        read_songtex(&text)
    }
}

fn text_of(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Reads Cantara 2's `.songtex`.
///
/// The format is a header of `%` comments and then one `\beginfile{name}` …
/// `\endfile` block per song. A `\noselection` line marks a file that is a
/// collection of songs rather than a running order; it is read all the same,
/// since a collection put in order *is* a running order and refusing it would
/// help nobody.
fn read_songtex(text: &str) -> Result<SelectionDocument, SelectionIoError> {
    const BEGIN: &str = "\\beginfile{";
    const END: &str = "\\endfile";

    let mut items = Vec::new();
    let mut current: Option<(String, Vec<&str>)> = None;

    for line in text.lines() {
        let trimmed = line.trim_end_matches('\r');
        if let Some(rest) = trimmed.trim_start().strip_prefix(BEGIN) {
            let name = rest.trim_end().trim_end_matches('}').to_string();
            current = Some((name, Vec::new()));
        } else if trimmed.trim_start().starts_with(END) {
            if let Some((name, lines)) = current.take() {
                let content = lines.join("\n");
                items.push(DocumentItem::of_file(&name, content.into_bytes()));
            }
        } else if let Some((_, lines)) = current.as_mut() {
            lines.push(trimmed);
        }
    }

    if items.is_empty() {
        return Err(SelectionIoError::Malformed(
            "no \\beginfile block found".to_string(),
        ));
    }

    Ok(SelectionDocument {
        items,
        ..Default::default()
    })
}

/// Reads Cantara 2's selection JSON.
///
/// Its `style_setting` is deliberately ignored: it describes a font, a colour
/// and a padding of a program whose slides are laid out differently, and a
/// design guessed from it would be a design nobody chose. The songs and their
/// order — what the file is actually for — come across exactly.
fn read_cantara_json(text: &str) -> Result<SelectionDocument, SelectionIoError> {
    let document: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| SelectionIoError::Malformed(error.to_string()))?;

    let songs = document
        .get("songs")
        .and_then(|songs| songs.as_array())
        .ok_or_else(|| SelectionIoError::Malformed("no \"songs\" array".to_string()))?;

    let mut items = Vec::new();
    for song in songs {
        let file_name = song
            .get("file_name")
            .and_then(|name| name.as_str())
            .unwrap_or_default();
        if file_name.is_empty() {
            continue;
        }
        let content = song
            .get("file_content")
            .and_then(|content| content.as_str())
            .map(|encoded| BASE64.decode(encoded).unwrap_or_default())
            .unwrap_or_default();

        items.push(DocumentItem::of_file(file_name, content));
    }

    if items.is_empty() {
        return Err(SelectionIoError::Malformed(
            "the file holds no songs".to_string(),
        ));
    }

    Ok(SelectionDocument {
        items,
        ..Default::default()
    })
}

/// Reads Cantara 3's own archive.
fn read_cantara_zip(bytes: &[u8]) -> Result<SelectionDocument, SelectionIoError> {
    use std::io::Read;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec()))
        .map_err(|error| SelectionIoError::Archive(error.to_string()))?;

    let manifest: SelectionManifest = {
        let mut file = archive
            .by_name(MANIFEST_NAME)
            .map_err(|_| SelectionIoError::Malformed(format!("no {MANIFEST_NAME}")))?;
        let mut json = String::new();
        file.read_to_string(&mut json)
            .map_err(|error| SelectionIoError::Archive(error.to_string()))?;
        serde_json::from_str(&json)
            .map_err(|error| SelectionIoError::Malformed(error.to_string()))?
    };

    if manifest.version > CANTARA_ZIP_VERSION {
        return Err(SelectionIoError::TooNew {
            found: manifest.version,
            supported: CANTARA_ZIP_VERSION,
        });
    }

    let mut items = Vec::with_capacity(manifest.items.len());
    for entry in &manifest.items {
        let content = match &entry.file {
            Some(path) => {
                let mut file = archive.by_name(path).map_err(|_| {
                    SelectionIoError::Malformed(format!("{path} is named but not in the archive"))
                })?;
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)
                    .map_err(|error| SelectionIoError::Archive(error.to_string()))?;
                Some(bytes)
            }
            None => None,
        };

        let pick = |list: &Vec<PresentationDesign>, index: Option<usize>| {
            index.and_then(|index| list.get(index).cloned())
        };
        let pick_settings = |list: &Vec<SlideSettings>, index: Option<usize>| {
            index.and_then(|index| list.get(index).cloned())
        };

        items.push(DocumentItem {
            file_name: entry
                .file
                .as_deref()
                .and_then(|path| path.rsplit('/').next())
                .unwrap_or(&entry.name)
                .to_string(),
            name: entry.name.clone(),
            md5: entry
                .md5
                .clone()
                .or_else(|| content.as_ref().map(|bytes| fingerprint_of(bytes))),
            content,
            design: pick(&manifest.designs, entry.design),
            slide_settings: pick_settings(&manifest.slide_settings, entry.slide_settings),
            stream_design: pick(&manifest.designs, entry.stream_design),
            stream_slide_settings: pick_settings(
                &manifest.slide_settings,
                entry.stream_slide_settings,
            ),
            inline_markdown: entry.inline_markdown.clone(),
            timer: entry.timer.clone(),
            transition: entry.transition,
            pdf_pages: entry.pdf_pages.clone(),
            video_settings: entry.video_settings,
        });
    }

    Ok(SelectionDocument {
        items,
        designs: manifest.designs,
        slide_settings: manifest.slide_settings,
    })
}

// ── Resolving ────────────────────────────────────────────────────────────────

/// Where an element of an imported selection is going to come from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AssetOrigin {
    /// It is already in the library — this is the file it is.
    InLibrary(PathBuf),
    /// It is not, and the archive brought it. It has to be written somewhere
    /// before it can be shown.
    New,
    /// It is not a file at all: Markdown typed into the program.
    Inline,
    /// It is not in the library and the file did not bring it either, which is
    /// what a Cantara 2 JSON without content looks like. Nothing can be shown
    /// for it.
    Missing,
}

/// What was found for one element of the document.
#[derive(Clone, PartialEq, Debug)]
pub struct ResolvedItem {
    pub item: DocumentItem,
    pub origin: AssetOrigin,
}

/// Where every element of an imported selection stands.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ResolvedSelection {
    pub items: Vec<ResolvedItem>,
    /// The designs of the file that the settings do not already hold.
    pub new_designs: Vec<PresentationDesign>,
    /// The slide divisions of the file that the settings do not already hold.
    pub new_slide_settings: Vec<SlideSettings>,
}

impl ResolvedSelection {
    /// How many elements would have to be written into the library.
    pub fn new_asset_count(&self) -> usize {
        self.items
            .iter()
            .filter(|resolved| resolved.origin == AssetOrigin::New)
            .count()
    }

    /// How many elements cannot be shown at all.
    pub fn missing_count(&self) -> usize {
        self.items
            .iter()
            .filter(|resolved| resolved.origin == AssetOrigin::Missing)
            .count()
    }
}

/// Looks every element of `document` up in the library.
///
/// Matching is by fingerprint first and by file name second: the same song
/// under another name is the same song, and a song of the same name that has
/// been edited since is still the one meant. Nothing is written here — what
/// this produces is the answer to "what would importing this do", which is
/// what the user is shown before anything happens.
pub fn resolve_selection(
    document: &SelectionDocument,
    library: &[SourceFile],
    known_designs: &[PresentationDesign],
    known_slide_settings: &[SlideSettings],
) -> ResolvedSelection {
    let items = document
        .items
        .iter()
        .map(|item| {
            let origin = if item.inline_markdown.is_some() {
                AssetOrigin::Inline
            } else if let Some(found) = find_in_library(item, library) {
                AssetOrigin::InLibrary(found)
            } else if item.content.is_some() {
                AssetOrigin::New
            } else {
                AssetOrigin::Missing
            };
            ResolvedItem {
                item: item.clone(),
                origin,
            }
        })
        .collect();

    let new_designs = document
        .designs
        .iter()
        .filter(|design| !known_designs.contains(design))
        .cloned()
        .collect();
    let new_slide_settings = document
        .slide_settings
        .iter()
        .filter(|settings| !known_slide_settings.contains(settings))
        .cloned()
        .collect();

    ResolvedSelection {
        items,
        new_designs,
        new_slide_settings,
    }
}

/// The file in the library this element is, if it is there.
fn find_in_library(item: &DocumentItem, library: &[SourceFile]) -> Option<PathBuf> {
    if let Some(md5) = &item.md5
        && let Some(found) = library
            .iter()
            .find(|file| file.md5_hash.as_deref() == Some(md5.as_str()))
    {
        return Some(found.path.clone());
    }

    library
        .iter()
        .find(|file| file.file_name().eq_ignore_ascii_case(&item.file_name))
        .map(|file| file.path.clone())
}

/// Turns a resolved element into something the selection can hold.
///
/// `path` is where the element's file ended up — the one found in the library,
/// or the one it was just written to. An element that is neither a file nor
/// Markdown has nothing to show and gives `None`.
pub fn selected_item_of(resolved: &ResolvedItem, path: Option<PathBuf>) -> Option<SelectedItemRepresentation> {
    let item = &resolved.item;

    let source_file = match (&item.inline_markdown, path) {
        (Some(_), _) => SourceFile {
            name: item.name.clone(),
            path: PathBuf::from(&item.file_name),
            file_type: SourceFileType::Markdown,
            md5_hash: None,
            relative_path: None,
        },
        (None, Some(path)) => SourceFile {
            name: item.name.clone(),
            file_type: SourceFileType::of(&item.file_name)?,
            md5_hash: item.md5.clone(),
            relative_path: None,
            path,
        },
        (None, None) => return None,
    };

    Some(SelectedItemRepresentation {
        source_file,
        presentation_design_option: item.design.clone(),
        slide_settings_option: item.slide_settings.clone(),
        stream_design_option: item.stream_design.clone(),
        stream_slide_settings_option: item.stream_slide_settings.clone(),
        inline_markdown: item.inline_markdown.clone(),
        timer_settings_option: item.timer.clone(),
        transition_effect: item.transition,
        pdf_pages: item.pdf_pages.clone(),
        video_settings: item.video_settings,
    })
}

// ── Importing ────────────────────────────────────────────────────────────────

/// What came of an import.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ImportOutcome {
    /// The running order, ready to be shown.
    pub items: Vec<SelectedItemRepresentation>,
    /// The files that were written into the library.
    pub written: Vec<PathBuf>,
    /// How many elements had to be left out because nothing could be found or
    /// written for them.
    pub left_out: usize,
}

/// Turns a resolved selection into a running order, writing the elements the
/// library does not have into `directory`.
///
/// The directory is a repository the user picked — or a temporary one, when
/// they would rather not have their library written to. Either way every
/// element that has content ends up somewhere it can be read from, because a
/// running order with a hole in it is of no use on a Sunday morning.
///
/// An element that is written keeps its own file name where that is free, so
/// that the library stays something a person can look at.
#[cfg(not(target_arch = "wasm32"))]
pub fn import_selection(
    resolved: &ResolvedSelection,
    directory: &std::path::Path,
) -> Result<ImportOutcome, SelectionIoError> {
    let mut outcome = ImportOutcome::default();

    for entry in &resolved.items {
        let path = match &entry.origin {
            AssetOrigin::InLibrary(path) => Some(path.clone()),
            AssetOrigin::Inline => None,
            AssetOrigin::Missing => {
                outcome.left_out += 1;
                continue;
            }
            AssetOrigin::New => {
                let Some(content) = &entry.item.content else {
                    outcome.left_out += 1;
                    continue;
                };
                let path = write_asset(directory, &entry.item.file_name, content)?;
                outcome.written.push(path.clone());
                Some(path)
            }
        };

        match selected_item_of(entry, path) {
            Some(item) => outcome.items.push(item),
            None => outcome.left_out += 1,
        }
    }

    Ok(outcome)
}

/// Writes one element into `directory` without overwriting anything.
///
/// A file of the same name that is already there and already holds the same
/// thing is used as it is; one that holds something else gets the element
/// beside it under a name of its own. Nothing in a user's library is ever
/// replaced by an import — a selection from someone else must not be able to
/// change the songs on this computer.
#[cfg(not(target_arch = "wasm32"))]
fn write_asset(
    directory: &std::path::Path,
    file_name: &str,
    content: &[u8],
) -> Result<PathBuf, SelectionIoError> {
    std::fs::create_dir_all(directory).map_err(|error| SelectionIoError::Unreadable {
        name: directory.display().to_string(),
        reason: error.to_string(),
    })?;

    let (stem, suffix) = match file_name.split_once('.') {
        Some((stem, suffix)) => (stem.to_string(), format!(".{suffix}")),
        None => (file_name.to_string(), String::new()),
    };

    for attempt in 0..1000 {
        let candidate = match attempt {
            0 => directory.join(file_name),
            _ => directory.join(format!("{stem} ({}){suffix}", attempt + 1)),
        };

        match std::fs::read(&candidate) {
            // Already there, and the same thing: nothing to write.
            Ok(existing) if existing == content => return Ok(candidate),
            // Already there and something else: try the next name.
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

/// The web build has no file system to write a library into, so an element it
/// does not already have cannot be imported.
#[cfg(target_arch = "wasm32")]
pub fn import_selection(
    resolved: &ResolvedSelection,
    _directory: &std::path::Path,
) -> Result<ImportOutcome, SelectionIoError> {
    let mut outcome = ImportOutcome::default();
    for entry in &resolved.items {
        let path = match &entry.origin {
            AssetOrigin::InLibrary(path) => Some(path.clone()),
            AssetOrigin::Inline => None,
            AssetOrigin::New | AssetOrigin::Missing => {
                outcome.left_out += 1;
                continue;
            }
        };
        match selected_item_of(entry, path) {
            Some(item) => outcome.items.push(item),
            None => outcome.left_out += 1,
        }
    }
    Ok(outcome)
}

/// Adds the designs and slide divisions the file brought and the settings do
/// not have yet.
///
/// The elements carry their own copies either way, so this is not what makes
/// an imported selection look right — it is what puts a design the user liked
/// into the list they can pick from and edit.
pub fn import_designs(
    settings: &mut crate::logic::settings::Settings,
    resolved: &ResolvedSelection,
) {
    for design in &resolved.new_designs {
        if !settings.presentation_designs.contains(design) {
            settings.presentation_designs.push(design.clone());
        }
    }
    for slide_settings in &resolved.new_slide_settings {
        // Compared by the division itself: what a selection file carries is
        // the division, and the name beside it in the settings is this user's
        // own — a second copy under a different name would help nobody.
        if !settings
            .song_slide_settings
            .iter()
            .any(|named| named.settings == *slide_settings)
        {
            settings
                .song_slide_settings
                .push(slide_settings.clone().into());
        }
    }
    settings.ensure_slide_settings_for_designs();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::settings::{PresentationDesignSettings, PresentationDesignTemplate};

    fn song_file(name: &str, path: &str) -> SourceFile {
        SourceFile {
            name: SourceFileType::display_name(name),
            path: PathBuf::from(path),
            file_type: SourceFileType::Song,
            md5_hash: None,
            relative_path: None,
        }
    }

    fn item_of(name: &str, path: &str) -> SelectedItemRepresentation {
        // Not `for_test`: an element in the library is named the way the
        // library names it, with the suffix stripped, and these tests check
        // exactly that name across a round trip.
        SelectedItemRepresentation::new_with_sourcefile(song_file(name, path))
    }

    /// Every element is read back with the content it was written with.
    fn reader(content: &'static str) -> impl Fn(&SourceFile) -> Result<Vec<u8>, String> {
        move |_: &SourceFile| Ok(content.as_bytes().to_vec())
    }

    const SONG: &str = "Amazing grace, how sweet the sound\nthat saved a wretch like me.";

    /// The whole point of the archive: what goes in comes back out, elements,
    /// order, designs and all.
    #[test]
    fn a_selection_survives_a_round_trip_through_the_archive() {
        let mut first = item_of("Amazing Grace.song", "/songs/Amazing Grace.song");
        first.presentation_design_option = Some(PresentationDesign {
            name: "Dark".to_string(),
            ..PresentationDesign::default()
        });
        first.transition_effect = SlideTransition::Morph;
        first.timer_settings_option = Some(SlideTimerSettings::default());
        let second = item_of("And Can It Be.song", "/songs/And Can It Be.song");

        let written = write_selection(
            &[first.clone(), second.clone()],
            SelectionFormat::CantaraZip,
            "Service",
            &reader(SONG),
        )
        .expect("the selection can be written");

        assert_eq!(written.name, "Service.cantara.zip");
        assert!(written.bytes.starts_with(b"PK"), "not a ZIP archive");

        let document = read_selection(&written.bytes, &written.name).expect("it reads back");

        assert_eq!(document.items.len(), 2);
        assert_eq!(document.items[0].file_name, "Amazing Grace.song");
        assert_eq!(document.items[0].name, "Amazing Grace");
        assert_eq!(
            document.items[0].content.as_deref(),
            Some(SONG.as_bytes()),
            "the song itself has to travel with the file"
        );
        assert_eq!(
            document.items[0].design.as_ref().map(|design| design.name.clone()),
            Some("Dark".to_string())
        );
        assert_eq!(document.items[0].transition, SlideTransition::Morph);
        assert!(document.items[0].timer.is_some());
        assert_eq!(document.items[1].name, "And Can It Be");
        assert_eq!(document.designs.len(), 1, "the design is stored once");
    }

    /// A design used by several elements is stored once and referred to, or an
    /// archive of twenty songs would hold twenty copies of the same design.
    #[test]
    fn a_shared_design_is_stored_once() {
        let design = PresentationDesign {
            name: "Dark".to_string(),
            ..PresentationDesign::default()
        };
        let mut first = item_of("A.song", "/songs/A.song");
        first.presentation_design_option = Some(design.clone());
        let mut second = item_of("B.song", "/songs/B.song");
        second.presentation_design_option = Some(design);

        let written = write_selection(
            &[first, second],
            SelectionFormat::CantaraZip,
            "Service",
            &|file: &SourceFile| Ok(file.name.clone().into_bytes()),
        )
        .expect("written");
        let document = read_selection(&written.bytes, &written.name).expect("read");

        assert_eq!(document.designs.len(), 1);
        assert_eq!(document.items[0].design, document.items[1].design);
    }

    /// The same song twice in one service is one file in the archive, and both
    /// places in the running order point at it.
    #[test]
    fn the_same_song_twice_is_stored_once() {
        let item = item_of("Amazing Grace.song", "/songs/Amazing Grace.song");

        let written = write_selection(
            &[item.clone(), item],
            SelectionFormat::CantaraZip,
            "Service",
            &reader(SONG),
        )
        .expect("written");

        let names = zip_entry_names(&written.bytes);
        assert_eq!(
            names.iter().filter(|name| name.starts_with("assets/")).count(),
            1,
            "the song was stored twice: {names:?}"
        );

        let document = read_selection(&written.bytes, &written.name).expect("read");
        assert_eq!(document.items.len(), 2, "both places stay in the order");
    }

    /// Two different songs that happen to share a file name must not overwrite
    /// each other inside the archive.
    #[test]
    fn two_songs_of_the_same_name_both_survive() {
        let first = item_of("Grace.song", "/one/Grace.song");
        let second = item_of("Grace.song", "/two/Grace.song");
        let mut round = std::cell::Cell::new(0);
        let _ = &mut round;

        let written = write_selection(
            &[first, second],
            SelectionFormat::CantaraZip,
            "Service",
            &|file: &SourceFile| Ok(file.path.to_string_lossy().as_bytes().to_vec()),
        )
        .expect("written");

        let assets: Vec<String> = zip_entry_names(&written.bytes)
            .into_iter()
            .filter(|name| name.starts_with("assets/"))
            .collect();
        assert_eq!(assets.len(), 2, "one of them was lost: {assets:?}");

        let document = read_selection(&written.bytes, &written.name).expect("read");
        assert_ne!(document.items[0].content, document.items[1].content);
    }

    /// Markdown typed into the program is not a file, and travels in the
    /// manifest itself.
    #[test]
    fn typed_markdown_travels_without_a_file() {
        let mut item = item_of("Note.md", "Note.md");
        item.source_file.file_type = SourceFileType::Markdown;
        item.inline_markdown = Some("# Welcome".to_string());

        let written = write_selection(
            &[item],
            SelectionFormat::CantaraZip,
            "Service",
            &|_: &SourceFile| Err("must not be read".to_string()),
        )
        .expect("written");

        let assets: Vec<String> = zip_entry_names(&written.bytes)
            .into_iter()
            .filter(|name| name.starts_with("assets/"))
            .collect();
        assert!(assets.is_empty(), "nothing should have been stored: {assets:?}");

        let document = read_selection(&written.bytes, &written.name).expect("read");
        assert_eq!(
            document.items[0].inline_markdown.as_deref(),
            Some("# Welcome")
        );
    }

    fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).expect("a ZIP archive");
        (0..archive.len())
            .filter_map(|index| archive.by_index(index).ok().map(|file| file.name().to_string()))
            .collect()
    }

    /// The archive has to be readable by anything that reads ZIP files, with
    /// the manifest where the documentation says it is.
    #[test]
    fn the_archive_is_a_manifest_and_a_folder() {
        let written = write_selection(
            &[item_of("Amazing Grace.song", "/songs/Amazing Grace.song")],
            SelectionFormat::CantaraZip,
            "Service",
            &reader(SONG),
        )
        .expect("written");

        let names = zip_entry_names(&written.bytes);
        assert!(names.contains(&"selection.json".to_string()), "{names:?}");
        assert!(
            names.contains(&"assets/Amazing Grace.song".to_string()),
            "{names:?}"
        );
    }

    /// The manifest is what `docs/formats/cantara-zip.md` describes, and other
    /// programs are invited to read it. Its shape is therefore part of the
    /// format rather than an implementation detail — this is what says so.
    #[test]
    fn the_manifest_is_shaped_the_way_it_is_documented() {
        let mut item = item_of("Amazing Grace.song", "/songs/Amazing Grace.song");
        item.presentation_design_option = Some(PresentationDesign::default());
        item.timer_settings_option = Some(SlideTimerSettings::default());

        let written = write_selection(
            &[item],
            SelectionFormat::CantaraZip,
            "Service",
            &reader(SONG),
        )
        .expect("written");

        let manifest: serde_json::Value = {
            let mut archive =
                zip::ZipArchive::new(std::io::Cursor::new(written.bytes)).expect("an archive");
            let mut json = String::new();
            std::io::Read::read_to_string(
                &mut archive.by_name(MANIFEST_NAME).expect("a manifest"),
                &mut json,
            )
            .expect("readable");
            serde_json::from_str(&json).expect("valid JSON")
        };

        assert_eq!(manifest["format"], "cantara.selection");
        assert_eq!(manifest["version"], CANTARA_ZIP_VERSION);
        assert!(manifest["created_by"].is_string());
        assert!(manifest["designs"].is_array());
        assert!(manifest["slide_settings"].is_array());

        let entry = &manifest["items"][0];
        assert_eq!(entry["file"], "assets/Amazing Grace.song");
        assert_eq!(entry["name"], "Amazing Grace");
        assert_eq!(entry["md5"], fingerprint_of(SONG.as_bytes()));
        assert_eq!(entry["design"], 0);
        assert_eq!(entry["transition"], "Fade");
        assert!(entry["timer"]["timer_seconds"].is_number());
        // What is not set is left out rather than written as null, so that the
        // file stays readable by a person.
        assert!(entry.get("slide_settings").is_none());
        assert!(entry.get("inline_markdown").is_none());
    }

    /// Cantara 2 reads the file, so it has to be exactly what Cantara 2
    /// writes: a comment header and `\beginfile{…}` … `\endfile` per song.
    #[test]
    fn songtex_is_written_the_way_cantara_2_writes_it() {
        let written = write_selection(
            &[item_of("Amazing Grace.song", "/songs/Amazing Grace.song")],
            SelectionFormat::Songtex,
            "Service",
            &reader(SONG),
        )
        .expect("written");

        let text = String::from_utf8(written.bytes).expect("text");
        assert!(text.starts_with("% This file has been created automatically"));
        assert!(text.contains("\\beginfile{Amazing Grace.song}"));
        assert!(text.contains(SONG));
        assert!(text.trim_end().ends_with("\\endfile"));
        assert_eq!(written.name, "Service.songtex");
    }

    /// And it has to read what Cantara 2 wrote — including the `\noselection`
    /// marker of a plain collection.
    #[test]
    fn songtex_from_cantara_2_is_read() {
        let file = "% This file has been created automatically\n\
                    % It can be opened with Cantara (https://cantara.app)\n\
                    \\noselection\n\
                    \\beginfile{Amazing Grace.song}\n\
                    Amazing grace\n\
                    how sweet the sound\n\
                    \\endfile\n\
                    \\beginfile{And Can It Be.song}\n\
                    And can it be\n\
                    \\endfile\n";

        let document = read_selection(file.as_bytes(), "collection.songtex").expect("read");

        assert_eq!(document.items.len(), 2);
        assert_eq!(document.items[0].file_name, "Amazing Grace.song");
        assert_eq!(
            String::from_utf8_lossy(document.items[0].content.as_deref().unwrap_or_default()),
            "Amazing grace\nhow sweet the sound"
        );
        assert_eq!(document.items[1].file_name, "And Can It Be.song");
    }

    /// A songtex file Cantara 3 wrote has to come back the same way.
    #[test]
    fn songtex_survives_a_round_trip() {
        let written = write_selection(
            &[
                item_of("Amazing Grace.song", "/songs/Amazing Grace.song"),
                item_of("And Can It Be.song", "/songs/And Can It Be.song"),
            ],
            SelectionFormat::Songtex,
            "Service",
            &reader(SONG),
        )
        .expect("written");

        let document = read_selection(&written.bytes, &written.name).expect("read");
        assert_eq!(document.items.len(), 2);
        assert_eq!(
            String::from_utf8_lossy(document.items[0].content.as_deref().unwrap_or_default()),
            SONG
        );
    }

    /// Cantara 2's selection JSON: the songs travel base64-encoded under the
    /// keys that program uses.
    #[test]
    fn cantara_2_json_is_written_with_its_own_keys() {
        let written = write_selection(
            &[item_of("Amazing Grace.song", "/songs/Amazing Grace.song")],
            SelectionFormat::CantaraJson,
            "Service",
            &reader(SONG),
        )
        .expect("written");

        let value: serde_json::Value =
            serde_json::from_slice(&written.bytes).expect("valid JSON");
        assert_eq!(value["version"], 1);
        let song = &value["songs"][0];
        assert_eq!(song["file_name"], "Amazing Grace.song");
        assert_eq!(song["style_setting"], "default");
        assert!(song["background_image"].is_null());
        let content = BASE64
            .decode(song["file_content"].as_str().unwrap_or_default())
            .expect("base64");
        assert_eq!(String::from_utf8_lossy(&content), SONG);
    }

    /// A selection written by Cantara 2 opens, style settings and all — they
    /// are read past rather than choked on.
    #[test]
    fn a_cantara_2_selection_opens() {
        let json = serde_json::json!({
            "version": 1,
            "songs": [
                {
                    "file_name": "Amazing Grace.song",
                    "file_content": BASE64.encode(SONG),
                    "style_setting": {
                        "background_color": "000000",
                        "text_color": "FFFFFF",
                        "font_name": "Arial",
                        "font_size": 32,
                    },
                    "background_image": serde_json::Value::Null,
                }
            ]
        })
        .to_string();

        let document = read_selection(json.as_bytes(), "service.json").expect("read");

        assert_eq!(document.items.len(), 1);
        assert_eq!(document.items[0].file_name, "Amazing Grace.song");
        assert_eq!(document.items[0].content.as_deref(), Some(SONG.as_bytes()));
        assert!(
            document.items[0].design.is_none(),
            "Cantara 2's style is not a Cantara 3 design and must not be invented"
        );
    }

    /// A picture or a PDF has nowhere to go in a Cantara 2 format. Leaving it
    /// out is the point; writing nothing at all when the selection holds only
    /// such elements is what has to be reported.
    #[test]
    fn a_cantara_2_format_holds_songs_and_says_so() {
        let mut picture = item_of("Logo.png", "/pictures/Logo.png");
        picture.source_file.file_type = SourceFileType::Image;

        assert!(SelectionFormat::Songtex.holds_only_songs());
        assert_eq!(
            write_selection(
                &[picture.clone()],
                SelectionFormat::Songtex,
                "Service",
                &reader(SONG)
            ),
            Err(SelectionIoError::Empty)
        );

        // Mixed with a song, the song comes through and the picture does not.
        let written = write_selection(
            &[picture, item_of("Amazing Grace.song", "/songs/Amazing Grace.song")],
            SelectionFormat::Songtex,
            "Service",
            &reader(SONG),
        )
        .expect("written");
        let document = read_selection(&written.bytes, &written.name).expect("read");
        assert_eq!(document.items.len(), 1);
        assert_eq!(document.items[0].file_name, "Amazing Grace.song");
    }

    /// The archive, by contrast, carries everything.
    #[test]
    fn the_archive_carries_pictures_and_documents_too() {
        let mut picture = item_of("Logo.png", "/pictures/Logo.png");
        picture.source_file.file_type = SourceFileType::Image;
        let mut pdf = item_of("Handout.pdf", "/documents/Handout.pdf");
        pdf.source_file.file_type = SourceFileType::Pdf;

        let written = write_selection(
            &[picture, pdf],
            SelectionFormat::CantaraZip,
            "Service",
            &|file: &SourceFile| Ok(file.name.clone().into_bytes()),
        )
        .expect("written");

        let document = read_selection(&written.bytes, &written.name).expect("read");
        assert_eq!(document.items.len(), 2);
    }

    /// A file that has been renamed still opens: what it is, is in it.
    #[test]
    fn a_file_without_a_telling_name_is_recognised_by_its_content() {
        let written = write_selection(
            &[item_of("Amazing Grace.song", "/songs/Amazing Grace.song")],
            SelectionFormat::CantaraZip,
            "Service",
            &reader(SONG),
        )
        .expect("written");

        assert!(read_selection(&written.bytes, "service").is_ok());

        let songtex = write_selection(
            &[item_of("Amazing Grace.song", "/songs/Amazing Grace.song")],
            SelectionFormat::Songtex,
            "Service",
            &reader(SONG),
        )
        .expect("written");
        assert!(read_selection(&songtex.bytes, "service").is_ok());
    }

    /// An archive from a later Cantara is refused with something the user can
    /// act on, rather than read half-way and shown wrongly.
    #[test]
    fn an_archive_from_a_later_cantara_says_so() {
        let manifest = serde_json::json!({
            "format": "cantara.selection",
            "version": CANTARA_ZIP_VERSION + 1,
            "items": [],
        });
        let bytes = build_zip(
            &serde_json::from_value(manifest).expect("a manifest"),
            &[],
        )
        .expect("written");

        assert_eq!(
            read_selection(&bytes, "later.cantara.zip"),
            Err(SelectionIoError::TooNew {
                found: CANTARA_ZIP_VERSION + 1,
                supported: CANTARA_ZIP_VERSION,
            })
        );
    }

    /// Something that is not a selection at all has to be refused clearly.
    #[test]
    fn nonsense_is_refused() {
        assert!(matches!(
            read_selection(b"hello", "notes.txt"),
            Err(SelectionIoError::Malformed(_))
        ));
        assert!(matches!(
            read_selection(b"{\"a\": 1}", "service.json"),
            Err(SelectionIoError::Malformed(_))
        ));
    }

    /// An element already in the library is used from there rather than
    /// imported a second time — that is what keeps a shared selection from
    /// filling the library with copies.
    #[test]
    fn an_element_already_in_the_library_is_found() {
        let document = SelectionDocument {
            items: vec![DocumentItem::of_file(
                "Amazing Grace.song",
                SONG.as_bytes().to_vec(),
            )],
            ..Default::default()
        };
        let library = vec![song_file("Amazing Grace.song", "/library/Amazing Grace.song")];

        let resolved = resolve_selection(&document, &library, &[], &[]);

        assert_eq!(
            resolved.items[0].origin,
            AssetOrigin::InLibrary(PathBuf::from("/library/Amazing Grace.song"))
        );
        assert_eq!(resolved.new_asset_count(), 0);
    }

    /// The same song under another name is still the same song.
    #[test]
    fn an_element_is_recognised_by_its_fingerprint_under_another_name() {
        let document = SelectionDocument {
            items: vec![DocumentItem::of_file(
                "Amazing Grace.song",
                SONG.as_bytes().to_vec(),
            )],
            ..Default::default()
        };
        let mut library_file = song_file("AmazingGrace_v2.song", "/library/AmazingGrace_v2.song");
        library_file.md5_hash = Some(fingerprint_of(SONG.as_bytes()));

        let resolved = resolve_selection(&document, &[library_file], &[], &[]);

        assert_eq!(
            resolved.items[0].origin,
            AssetOrigin::InLibrary(PathBuf::from("/library/AmazingGrace_v2.song"))
        );
    }

    /// An element the library does not have, but the file brought, is one the
    /// user can import.
    #[test]
    fn an_element_the_library_lacks_is_offered_for_import() {
        let document = SelectionDocument {
            items: vec![DocumentItem::of_file(
                "Unknown.song",
                SONG.as_bytes().to_vec(),
            )],
            ..Default::default()
        };

        let resolved = resolve_selection(&document, &[], &[], &[]);

        assert_eq!(resolved.items[0].origin, AssetOrigin::New);
        assert_eq!(resolved.new_asset_count(), 1);
        assert_eq!(resolved.missing_count(), 0);
    }

    /// A Cantara 2 file that carries no content for a song it names leaves
    /// nothing to show, and that has to be said rather than shown as an empty
    /// slide in front of a congregation.
    #[test]
    fn an_element_with_neither_a_library_entry_nor_content_is_missing() {
        let mut item = DocumentItem::of_file("Unknown.song", Vec::new());
        item.content = None;
        item.md5 = None;
        let document = SelectionDocument {
            items: vec![item],
            ..Default::default()
        };

        let resolved = resolve_selection(&document, &[], &[], &[]);

        assert_eq!(resolved.items[0].origin, AssetOrigin::Missing);
        assert_eq!(resolved.missing_count(), 1);
    }

    /// A design the user already has is not offered a second time; one they do
    /// not have is what the import dialog asks about.
    #[test]
    fn only_designs_the_settings_lack_are_offered() {
        let known = PresentationDesign::default();
        let mut template = PresentationDesignTemplate::default();
        template.background_transparency = 40;
        let brought = PresentationDesign {
            name: "Dark".to_string(),
            presentation_design_settings: PresentationDesignSettings::Template(template),
            ..PresentationDesign::default()
        };
        let document = SelectionDocument {
            items: vec![],
            designs: vec![known.clone(), brought.clone()],
            slide_settings: vec![SlideSettings::default()],
        };

        let resolved = resolve_selection(
            &document,
            &[],
            &[known],
            &[SlideSettings::default()],
        );

        assert_eq!(resolved.new_designs, vec![brought]);
        assert!(resolved.new_slide_settings.is_empty());
    }

    /// What comes out of an import has to be an element the selection can
    /// hold, with everything the file said about it.
    #[test]
    fn a_resolved_element_becomes_a_selected_item() {
        let mut item = DocumentItem::of_file("Amazing Grace.song", SONG.as_bytes().to_vec());
        item.transition = SlideTransition::Morph;
        item.design = Some(PresentationDesign {
            name: "Dark".to_string(),
            ..PresentationDesign::default()
        });
        let resolved = ResolvedItem {
            item,
            origin: AssetOrigin::InLibrary(PathBuf::from("/library/Amazing Grace.song")),
        };

        let selected = selected_item_of(&resolved, Some(PathBuf::from("/library/Amazing Grace.song")))
            .expect("a song is something the selection can hold");

        assert_eq!(selected.source_file.name, "Amazing Grace");
        assert_eq!(selected.source_file.file_type, SourceFileType::Song);
        assert_eq!(selected.transition_effect, SlideTransition::Morph);
        assert_eq!(
            selected.presentation_design_option.map(|design| design.name),
            Some("Dark".to_string())
        );
    }

    /// Importing writes what the library lacks and leaves what it has where
    /// it is, so a shared selection does not fill the library with copies.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn importing_writes_only_what_the_library_lacks() {
        let directory = tempfile::tempdir().expect("a directory to import into");

        let document = SelectionDocument {
            items: vec![
                DocumentItem::of_file("Known.song", SONG.as_bytes().to_vec()),
                DocumentItem::of_file("Unknown.song", b"And can it be".to_vec()),
            ],
            ..Default::default()
        };
        let library = vec![song_file("Known.song", "/library/Known.song")];

        let resolved = resolve_selection(&document, &library, &[], &[]);
        let outcome = import_selection(&resolved, directory.path()).expect("imported");

        assert_eq!(outcome.items.len(), 2, "the whole order has to arrive");
        assert_eq!(outcome.written.len(), 1, "only the unknown song is written");
        assert_eq!(outcome.left_out, 0);
        assert_eq!(
            outcome.items[0].source_file.path,
            PathBuf::from("/library/Known.song"),
            "the copy already in the library is the one used"
        );
        assert_eq!(
            std::fs::read(&outcome.written[0]).expect("the file was written"),
            b"And can it be"
        );
        assert_eq!(
            outcome.items[1].source_file.path,
            outcome.written[0],
            "the imported element points at what was written"
        );
    }

    /// A file of that name that is already there and holds something else must
    /// not be overwritten: a selection from someone else may not change the
    /// songs on this computer.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn importing_never_overwrites_a_file_that_is_already_there() {
        let directory = tempfile::tempdir().expect("a directory");
        let existing = directory.path().join("Grace.song");
        std::fs::write(&existing, b"the version on this computer").expect("written");

        let document = SelectionDocument {
            items: vec![DocumentItem::of_file(
                "Grace.song",
                b"a different version".to_vec(),
            )],
            ..Default::default()
        };
        let resolved = resolve_selection(&document, &[], &[], &[]);
        let outcome = import_selection(&resolved, directory.path()).expect("imported");

        assert_eq!(
            std::fs::read(&existing).expect("still there"),
            b"the version on this computer"
        );
        assert_eq!(outcome.written.len(), 1);
        assert_ne!(outcome.written[0], existing);
        assert_eq!(
            std::fs::read(&outcome.written[0]).expect("written"),
            b"a different version"
        );
    }

    /// The same import run twice writes one copy: the second time the file is
    /// already there with exactly that content.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn importing_the_same_selection_twice_writes_one_copy() {
        let directory = tempfile::tempdir().expect("a directory");
        let document = SelectionDocument {
            items: vec![DocumentItem::of_file(
                "Unknown.song",
                SONG.as_bytes().to_vec(),
            )],
            ..Default::default()
        };
        let resolved = resolve_selection(&document, &[], &[], &[]);

        let first = import_selection(&resolved, directory.path()).expect("imported");
        let second = import_selection(&resolved, directory.path()).expect("imported again");

        assert_eq!(first.written, second.written);
        let files: Vec<_> = std::fs::read_dir(directory.path())
            .expect("readable")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(files.len(), 1, "a second copy was written");
    }

    /// A design the file brought and the user does not have joins their list,
    /// so they can pick it and edit it afterwards.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn importing_designs_adds_only_the_new_ones() {
        use crate::logic::settings::Settings;

        let mut settings = Settings::default();
        let known = settings.presentation_designs[0].clone();
        let brought = PresentationDesign {
            name: "Dark".to_string(),
            ..PresentationDesign::default()
        };
        let document = SelectionDocument {
            items: vec![],
            designs: vec![known.clone(), brought.clone()],
            slide_settings: vec![],
        };
        let resolved = resolve_selection(&document, &[], &settings.presentation_designs, &[]);

        import_designs(&mut settings, &resolved);

        assert!(settings.presentation_designs.contains(&brought));
        assert_eq!(
            settings
                .presentation_designs
                .iter()
                .filter(|design| **design == known)
                .count(),
            1,
            "the design the user already had was added a second time"
        );
        assert!(
            settings.song_slide_settings.len() >= settings.presentation_designs.len(),
            "every design needs a slide division beside it"
        );
    }

    /// An element that could not be found anywhere is left out of the
    /// selection rather than added as something that cannot be shown.
    #[test]
    fn a_missing_element_becomes_nothing() {
        let resolved = ResolvedItem {
            item: DocumentItem::of_file("Unknown.song", Vec::new()),
            origin: AssetOrigin::Missing,
        };

        assert!(selected_item_of(&resolved, None).is_none());
    }
}
