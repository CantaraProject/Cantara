//! Taking a Cantara 2 installation over, the first time Cantara 3 starts.
//!
//! Somebody who has been projecting with Cantara 2 for years has a library, a
//! background picture, a font they settled on and a metadata line they wrote
//! themselves. Meeting them with an empty welcome wizard asks them to describe
//! all of it a second time, from memory, to a program that could simply have
//! read it: Cantara 2 wrote everything down, in one INI file.
//!
//! So on the first start — and only then, when Cantara 3 has no settings of
//! its own yet — that file is looked for, read, and turned into settings. The
//! wizard is skipped and the user is told what was taken over instead.
//!
//! # Where the file is
//!
//! Cantara 2 asked Free Pascal for `GetAppConfigFile`, which puts
//! `cantara.cfg` beside the other configuration of the user's account. What
//! "beside" means is the operating system's business, and on Linux it is also
//! the *sandbox's* business: inside a Flatpak or a Snap, the config directory
//! is the one inside that sandbox. That is exactly what is wanted here — a
//! Flatpak Cantara 3 should take over the Flatpak Cantara 2 and not some other
//! copy on the same machine — and it comes for free, because Cantara 3 asks
//! the same question of the same environment. See [`config_candidates`].
//!
//! # What is carried over
//!
//! | Cantara 2 | Cantara 3 |
//! |---|---|
//! | `Repo-Path` | the one repository |
//! | colours, font, alignment, padding, background picture | the **first** presentation design |
//! | title slide, spoiler, metadata, wrapping | the **first** slide division |
//!
//! Cantara 3's own default design and division are not thrown away: they move
//! to the second position, so that switching back is a matter of picking them
//! from a list rather than rebuilding them.
//!
//! # What is not
//!
//! Cantara 2 knew underlined and struck-through type as well as bold and
//! italic; a design here has no counterpart for either, so those two are
//! dropped. The window geometry and the export switches under `[Size]` and
//! `[Exporter]` describe a program that no longer exists.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rgb::{RGB8, RGBA8, Rgb, Rgba};

use crate::logic::css::CssFontFamily;
use crate::logic::settings::{
    CssSize, FontRepresentation, HorizontalAlign, PresentationDesign, PresentationDesignSettings,
    Settings, SongSlideSettings, TopBottomLeftRight, VerticalAlign,
};

/// What Cantara 2's configuration file is called, on every platform.
pub const CONFIG_FILE_NAME: &str = "cantara.cfg";

/// The section everything worth reading lives in. `[Size]` is window geometry
/// and `[Exporter]` is a licence checkbox for a program that no longer exists.
const CONFIG_SECTION: &str = "Config";

// ── The file ────────────────────────────────────────────────────────────────

/// A parsed INI file.
///
/// Only as much of the format as Free Pascal's `TIniFile` writes, which is all
/// this ever has to read: sections in square brackets, `key=value` lines, and
/// comments. Names are matched without regard to case, as `TIniFile` matches
/// them.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct IniFile {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl IniFile {
    /// Reads the text of an INI file.
    ///
    /// Nothing here can fail. A line that is not a section, a comment or a
    /// pair is skipped, because half a configuration file is still worth more
    /// than none — and the values that were not found simply fall back to
    /// Cantara 2's own defaults in [`LegacyConfig::read`].
    pub fn parse(text: &str) -> IniFile {
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        // Everything before the first `[Section]`. Cantara 2 never writes such
        // a line, but a file edited by hand may have one, and it has to go
        // somewhere rather than into whatever section comes next.
        let mut current = String::new();

        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = name.trim().to_lowercase();
                sections.entry(current.clone()).or_default();
                continue;
            }

            // Split at the *first* `=`: a metadata template is full of them.
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_lowercase(), value.trim().to_string());
        }

        IniFile { sections }
    }

    /// The value at `section`/`key`, if the file has one.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_lowercase())?
            .get(&key.to_lowercase())
            .map(String::as_str)
    }

    /// The value as text, or `fallback` where the file is silent.
    pub fn string(&self, section: &str, key: &str, fallback: &str) -> String {
        self.get(section, key).unwrap_or(fallback).to_string()
    }

    /// The value as a whole number, the way `TIniFile.ReadInteger` reads one:
    /// anything that is not a number at all is the default rather than an
    /// error. `$` introduces hexadecimal, as it does in Pascal.
    pub fn integer(&self, section: &str, key: &str, fallback: i64) -> i64 {
        self.get(section, key)
            .and_then(parse_pascal_integer)
            .unwrap_or(fallback)
    }

    /// The value as a switch. `TIniFile.ReadBool` reads a number and asks
    /// whether it is zero, which is why `1` and `-1` and `2` are all "on".
    pub fn boolean(&self, section: &str, key: &str, fallback: bool) -> bool {
        match self.get(section, key).and_then(parse_pascal_integer) {
            Some(number) => number != 0,
            None => fallback,
        }
    }
}

/// A number as Pascal writes one: decimal, or hexadecimal behind a `$`.
fn parse_pascal_integer(text: &str) -> Option<i64> {
    let text = text.trim();
    match text.strip_prefix('$') {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

// ── The values ──────────────────────────────────────────────────────────────

/// Cantara 2's presentation settings, with its own defaults filled in.
///
/// The defaults are not Cantara 3's: a key that is missing from the file means
/// the user never touched that switch, and what they saw on screen was
/// Cantara 2's default for it. They are taken from `TfrmSettings.loadSettings`.
#[derive(Clone, PartialEq, Debug)]
pub struct LegacyConfig {
    /// The folder the song library is in.
    pub repository_path: String,

    /// An empty slide after each song.
    pub empty_frame: bool,

    /// The colour the lyrics are set in.
    pub text_color: RGB8,

    /// The colour behind them.
    pub background_color: RGB8,

    /// Whether the next block is previewed under the current one.
    pub spoiler: bool,

    /// The font family, as the platform's font dialog named it.
    pub font_name: String,

    /// Its size in points.
    pub font_size: i64,

    /// Bold.
    pub bold: bool,

    /// Italic. Cantara 2's other two styles have nowhere to go — see the
    /// module documentation.
    pub italic: bool,

    /// Whether each song opens with a slide carrying its title.
    pub title_slide: bool,

    /// The metadata line on the first content slide.
    pub meta_first_slide: bool,

    /// …and on the last.
    pub meta_last_slide: bool,

    /// What that line says, in Cantara 2's own template language.
    pub meta_syntax: String,

    /// The background picture, whether or not it is switched on.
    pub background_picture_path: String,

    /// Whether it is switched on.
    pub show_background_picture: bool,

    /// How far the picture is faded towards the background colour, 0–100.
    pub image_brightness: i64,

    /// How many lines a slide may hold before the block is broken in two.
    /// Zero means "do not break".
    pub auto_wrap: i64,

    /// 0 left, 1 centred, 2 right.
    pub align_horizontal: i64,

    /// 0 top, 1 middle, 2 bottom.
    pub align_vertical: i64,

    /// The margins, in pixels of the presentation canvas.
    pub padding_left: i64,
    pub padding_top: i64,
    pub padding_right: i64,
    pub padding_bottom: i64,
}

/// What Cantara 2 padded a slide with when the user never said.
const LEGACY_DEFAULT_PADDING: i64 = 15;

/// One letter of `Font-Style`, which Cantara 2 writes as four of `T` or `F`.
///
/// A string that is too short — a hand-edited file, or an empty setting — is
/// read as "off" rather than as a reason to give up on the whole design.
fn style_flag(style: &str, position: usize) -> bool {
    style.chars().nth(position) == Some('T')
}

impl LegacyConfig {
    /// Reads the `[Config]` section.
    pub fn read(ini: &IniFile) -> LegacyConfig {
        let style = ini.string(CONFIG_SECTION, "Font-Style", "FFFF");

        LegacyConfig {
            repository_path: ini.string(CONFIG_SECTION, "Repo-Path", ""),
            empty_frame: ini.boolean(CONFIG_SECTION, "empty-Frame", true),
            text_color: color_of(&ini.string(CONFIG_SECTION, "Text-Color", "clWhite"))
                .unwrap_or(Rgb::new(255, 255, 255)),
            background_color: color_of(&ini.string(CONFIG_SECTION, "Background-Color", "clBlack"))
                .unwrap_or(Rgb::new(0, 0, 0)),
            spoiler: ini.boolean(CONFIG_SECTION, "Spoiler", true),
            font_name: ini.string(CONFIG_SECTION, "Font-Name", "default"),
            font_size: ini.integer(CONFIG_SECTION, "Font-Size", 42),
            // `TTFF` — bold, italic, underline, struck through, one letter
            // each. The first two have a counterpart here; see the module
            // documentation for the other two.
            bold: style_flag(&style, 0),
            italic: style_flag(&style, 1),
            title_slide: ini.boolean(CONFIG_SECTION, "TitleSlide", false),
            meta_first_slide: ini.boolean(CONFIG_SECTION, "MetaDataFirstSlide", false),
            meta_last_slide: ini.boolean(CONFIG_SECTION, "MetaDataLastSlide", false),
            meta_syntax: ini.string(CONFIG_SECTION, "MetaDataSyntax", ""),
            background_picture_path: ini.string(CONFIG_SECTION, "BackgroundPicture-Path", ""),
            show_background_picture: ini.boolean(CONFIG_SECTION, "BackgroundPicture", false),
            image_brightness: ini.integer(CONFIG_SECTION, "ImageBrightness", 0),
            auto_wrap: ini.integer(CONFIG_SECTION, "AutoWrap", 4),
            align_horizontal: ini.integer(CONFIG_SECTION, "AlignHorizontal", 1),
            align_vertical: ini.integer(CONFIG_SECTION, "AlignVertical", 1),
            padding_left: ini.integer(CONFIG_SECTION, "Padding-Left", LEGACY_DEFAULT_PADDING),
            padding_top: ini.integer(CONFIG_SECTION, "Padding-Top", LEGACY_DEFAULT_PADDING),
            padding_right: ini.integer(CONFIG_SECTION, "Padding-Right", LEGACY_DEFAULT_PADDING),
            padding_bottom: ini.integer(CONFIG_SECTION, "Padding-Bottom", LEGACY_DEFAULT_PADDING),
        }
    }
}

// ── Colours ─────────────────────────────────────────────────────────────────

/// The sixteen names Delphi and Lazarus give colours, as `$00BBGGRR`.
///
/// The order is the surprise: a `TColor` is *blue* first, so `clRed` is
/// `$0000FF` and `clBlue` is `$FF0000`. Reading these as HTML colours silently
/// swaps every red and blue on the slide, which is the sort of mistake nobody
/// notices until the projector is on.
const DELPHI_COLORS: &[(&str, u32)] = &[
    ("clblack", 0x000000),
    ("clmaroon", 0x000080),
    ("clgreen", 0x008000),
    ("clolive", 0x008080),
    ("clnavy", 0x800000),
    ("clpurple", 0x800080),
    ("clteal", 0x808000),
    ("clgray", 0x808080),
    ("clgrey", 0x808080),
    ("cldkgray", 0x808080),
    ("clmedgray", 0xA4A0A0),
    ("clsilver", 0xC0C0C0),
    ("clltgray", 0xC0C0C0),
    ("clred", 0x0000FF),
    ("cllime", 0x00FF00),
    ("clyellow", 0x00FFFF),
    ("clblue", 0xFF0000),
    ("clfuchsia", 0xFF00FF),
    ("claqua", 0xFFFF00),
    ("clwhite", 0xFFFFFF),
    ("clmoneygreen", 0xC0DCC0),
    ("clskyblue", 0xF0CAA6),
    ("clcream", 0xF0FBFF),
    // The two system colours a text or background setting is ever left at.
    // They are looked up from the desktop theme in Cantara 2, and what that
    // theme was is not knowable from here — but on a projector they were
    // black on white.
    ("clwindowtext", 0x000000),
    ("clwindow", 0xFFFFFF),
];

/// A Cantara 2 colour, as red, green and blue.
///
/// Accepts what `StringToColor` accepts: one of the names above, `$00BBGGRR`,
/// or a plain number.
pub fn color_of(text: &str) -> Option<RGB8> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let lower = text.to_lowercase();
    if let Some((_, value)) = DELPHI_COLORS.iter().find(|(name, _)| *name == lower) {
        return Some(bgr_to_rgb(*value));
    }

    let number = match lower.strip_prefix('$') {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => match lower.strip_prefix("0x") {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            // A negative number is a system colour Cantara 2 resolved against
            // the desktop theme. There is nothing to resolve it against here.
            None => lower.parse::<u32>().ok()?,
        },
    };

    Some(bgr_to_rgb(number))
}

/// `$00BBGGRR` as red, green, blue.
fn bgr_to_rgb(value: u32) -> RGB8 {
    Rgb::new(
        (value & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        ((value >> 16) & 0xFF) as u8,
    )
}

/// A font family as Cantara 3 should name it.
///
/// The platform's font dialog hands back a name with the script it was
/// selected for appended in brackets — `Nimbus Sans [UKWN]` — and that bracket
/// is part of no font family in the world. `default` is Cantara 2's way of
/// saying "whatever the system uses", which is what `None` means here.
pub fn font_family_of(name: &str) -> Option<String> {
    let name = match name.split_once('[') {
        Some((before, _)) => before.trim(),
        None => name.trim(),
    };

    match name.is_empty() || name.eq_ignore_ascii_case("default") {
        true => None,
        false => Some(name.to_string()),
    }
}

// ── The metadata template ───────────────────────────────────────────────────

/// Cantara 2's template language, as Handlebars.
///
/// Cantara 2 wrote a metadata line like
///
/// ```text
/// {%author%}Author: {author}
/// {%bible%}Bible reference: {bible}
/// ```
///
/// `{name}` is the value of a tag, and `{%name%}` opens a stretch that is only
/// printed when the song has that tag at all — so a song without an author
/// does not project the word "Author:" followed by nothing. That stretch ends
/// at `{%end%}` or, failing that, at the end of the line; Cantara 2 splits its
/// template on line breaks and never looks past one.
///
/// Handlebars says the same things with `{{name}}` and `{{#if name}}…{{/if}}`,
/// so the conversion is a rewrite rather than a reimplementation. Names are
/// wrapped in brackets where they are not plain identifiers, since a tag such
/// as `ccli-songnumber` would otherwise be read as a subtraction.
///
/// A `</br>` is how the configuration file stores a line break, the file being
/// one line per setting.
pub fn convert_meta_syntax(source: &str) -> String {
    let source = source.replace("</br>", "\n").replace("<br>", "\n");

    source
        .lines()
        .map(convert_meta_line)
        .collect::<Vec<String>>()
        .join("\n")
}

/// One line of a Cantara 2 template.
fn convert_meta_line(line: &str) -> String {
    let characters: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    // How many `{%name%}` are still waiting to be closed. A line ends them all,
    // because that is where Cantara 2 stopped looking.
    let mut open = 0usize;
    let mut index = 0usize;

    while index < characters.len() {
        let Some((token, next)) = read_token(&characters, index) else {
            out.push(characters[index]);
            index += 1;
            continue;
        };
        index = next;

        match token {
            Token::End => {
                if open > 0 {
                    open -= 1;
                    out.push_str("{{/if}}");
                }
            }
            Token::Condition(name) => {
                open += 1;
                out.push_str(&format!("{{{{#if {}}}}}", handlebars_name(&name)));
            }
            Token::Value(name) => {
                out.push_str(&format!("{{{{{}}}}}", handlebars_name(&name)));
            }
        }
    }

    for _ in 0..open {
        out.push_str("{{/if}}");
    }
    out
}

/// What a `{…}` in a Cantara 2 template turned out to be.
enum Token {
    /// `{%end%}`
    End,
    /// `{%name%}`
    Condition(String),
    /// `{name}`
    Value(String),
}

/// Reads one `{…}` starting at `index`, and says where it ended.
///
/// `None` when there is no complete, non-empty one there — a stray brace is
/// literal text, exactly as it was in Cantara 2.
fn read_token(characters: &[char], index: usize) -> Option<(Token, usize)> {
    if characters.get(index) != Some(&'{') {
        return None;
    }

    let close = characters[index + 1..].iter().position(|c| *c == '}')? + index + 1;
    let inner: String = characters[index + 1..close].iter().collect();
    let after = close + 1;

    // `{%name%}` — the `%` belongs to the delimiter, not to the name.
    if let Some(name) = inner
        .strip_prefix('%')
        .and_then(|rest| rest.strip_suffix('%'))
    {
        let name = name.trim().to_lowercase();
        return match name.as_str() {
            "" => None,
            "end" => Some((Token::End, after)),
            _ => Some((Token::Condition(name), after)),
        };
    }

    let name = inner.trim().to_lowercase();
    match name.is_empty() {
        true => None,
        false => Some((Token::Value(name), after)),
    }
}

/// A tag name as a Handlebars path.
///
/// Anything that is not a bare identifier goes in brackets — Handlebars' own
/// escape for a name it would otherwise try to read as an expression.
fn handlebars_name(name: &str) -> String {
    let plain = !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');

    match plain {
        true => name.to_string(),
        false => format!("[{name}]"),
    }
}

// ── Becoming Cantara 3 settings ─────────────────────────────────────────────

/// The design Cantara 2's appearance settings describe.
///
/// `name` is passed in rather than translated here so that the logic stays
/// free of the interface; the caller has the user's language.
pub fn design_of(config: &LegacyConfig, name: String) -> PresentationDesign {
    // `Default` is a template and has been since the type existed. Asked for
    // rather than matched on, so that a design kind added later does not have
    // to be answered for here — and falling back to a plain default template
    // rather than panicking, because this runs while a Cantara 2 installation
    // is being taken over and there is nothing to be gained by refusing.
    let mut template = PresentationDesignSettings::default()
        .template()
        .cloned()
        .unwrap_or_default();

    let family = font_family_of(&config.font_name).map(CssFontFamily::with_family);
    let color: RGBA8 = Rgba::new(
        config.text_color.r,
        config.text_color.g,
        config.text_color.b,
        255,
    );
    let horizontal = match config.align_horizontal {
        0 => HorizontalAlign::Left,
        2 => HorizontalAlign::Right,
        _ => HorizontalAlign::Centered,
    };

    // One block for the lyrics, one for the spoiler, one for the metadata.
    // Cantara 2 drew the spoiler at half the height of the lyrics and the
    // metadata at a third of it, and those proportions are as much a part of
    // what the user is used to as the size itself.
    let size = config.font_size.clamp(1, 500) as f32;
    let mut main = FontRepresentation {
        font_family: family,
        font_size: CssSize::Pt(size),
        color,
        horizontal_alignment: horizontal,
        italic: config.italic,
        ..FontRepresentation::default()
    };
    main.set_bold(config.bold);

    let mut spoiler = main.clone();
    spoiler.font_size = CssSize::Pt(size / 2.0);

    let mut meta = main.clone();
    meta.font_size = CssSize::Pt(size / 3.0);

    template.fonts = vec![main, spoiler, meta];
    template.spoiler_index = Some(1);
    template.meta_index = Some(2);

    template.background_color = config.background_color;
    template.vertical_alignment = match config.align_vertical {
        0 => VerticalAlign::Top,
        2 => VerticalAlign::Bottom,
        _ => VerticalAlign::Middle,
    };
    template.padding = TopBottomLeftRight {
        top: CssSize::Px(config.padding_top.max(0) as f32),
        bottom: CssSize::Px(config.padding_bottom.max(0) as f32),
        left: CssSize::Px(config.padding_left.max(0) as f32),
        right: CssSize::Px(config.padding_right.max(0) as f32),
    };
    // Cantara 2 drew the picture at `255 - brightness × 2.55` of full opacity
    // over the background colour; here the same number is a percentage the
    // picture is faded by. The scale is the same, only the name changed.
    template.background_transparency = config.image_brightness.unsigned_abs().min(100) as u8;
    template.background_image = background_image_of(config);

    PresentationDesign {
        name,
        description: String::new(),
        presentation_design_settings: PresentationDesignSettings::Template(template),
    }
}

/// The background picture, if the old settings had one switched on and it is
/// still where they say it is.
///
/// A picture that has been moved or deleted since is left out rather than
/// carried over as a broken path: a design pointing at nothing renders as a
/// black slide with no hint of why.
fn background_image_of(config: &LegacyConfig) -> Option<crate::logic::sourcefiles::ImageSourceFile> {
    use crate::logic::sourcefiles::{ImageSourceFile, SourceFile, SourceFileType};

    if !config.show_background_picture {
        return None;
    }

    let path = PathBuf::from(config.background_picture_path.trim());
    if !path.is_file() {
        return None;
    }

    let file_name = path.file_name()?.to_string_lossy().to_string();
    ImageSourceFile::new(SourceFile {
        name: SourceFileType::display_name(&file_name),
        file_type: SourceFileType::Image,
        md5_hash: None,
        relative_path: None,
        path,
    })
}

/// The slide division Cantara 2's slide settings describe.
pub fn slide_settings_of(config: &LegacyConfig, name: String) -> SongSlideSettings {
    use cantara_songlib::slides::{ShowMetaInformation, SlideSettings};

    SongSlideSettings {
        name,
        description: String::new(),
        settings: SlideSettings {
            title_slide: config.title_slide,
            show_spoiler: config.spoiler,
            // Cantara 2 had no separate switch for the title slide's metadata:
            // the title slide *was* the metadata slide, and printed the line
            // underneath the title whenever it was shown at all.
            show_meta_information: ShowMetaInformation {
                title_slide: config.title_slide,
                first_slide: config.meta_first_slide,
                last_slide: config.meta_last_slide,
                // Cantara 2 could not put the line on every slide.
                all_slides: false,
            },
            meta_syntax: convert_meta_syntax(&config.meta_syntax),
            empty_last_slide: config.empty_frame,
            // Zero is Cantara 2's "do not break a block up at all".
            max_lines: match config.auto_wrap {
                wrap if wrap > 0 => Some(wrap as usize),
                _ => None,
            },
            ..SlideSettings::default()
        },
    }
}

// ── What the user is told afterwards ────────────────────────────────────────

/// What an import took over, so that the user can be told rather than left to
/// discover it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LegacyImportReport {
    /// The configuration file that was read.
    pub source: PathBuf,

    /// The library folder that became a repository.
    pub repository: Option<String>,

    /// The library folder the old settings named, where it is no longer
    /// there. Worth saying out loud: it is the one thing the user has to go
    /// and fix.
    pub missing_repository: Option<String>,

    /// The font family the design was given, where it names one.
    pub font_family: Option<String>,

    /// Whether a background picture came with it.
    pub background_image: bool,
}

// ── Doing it ────────────────────────────────────────────────────────────────

/// Puts a Cantara 2 configuration into fresh Cantara 3 settings.
///
/// The design and the division go *first* and Cantara 3's defaults move to
/// second place, so that what the user sees on the first start is what they
/// are used to, and the alternative is one click away in the same list.
///
/// The names are passed in for the reason given on [`design_of`].
pub fn apply(
    settings: &mut Settings,
    config: &LegacyConfig,
    design_name: String,
    division_name: String,
) -> LegacyImportReport {
    let mut report = LegacyImportReport {
        source: PathBuf::new(),
        repository: None,
        missing_repository: None,
        font_family: font_family_of(&config.font_name),
        background_image: false,
    };

    let folder = config.repository_path.trim();
    if !folder.is_empty() {
        match Path::new(folder).is_dir() {
            true => {
                settings.add_repository_folder(folder.to_string());
                report.repository = settings
                    .repositories
                    .last()
                    .map(|repository| repository.name.clone());
            }
            false => report.missing_repository = Some(folder.to_string()),
        }
    }

    let design = design_of(config, design_name);
    report.background_image = design
        .presentation_design_settings
        .template()
        .is_some_and(|template| template.background_image.is_some());

    settings.presentation_designs.insert(0, design);
    settings
        .song_slide_settings
        .insert(0, slide_settings_of(config, division_name));

    // The imported pair is what a presentation is built with unless something
    // says otherwise; Cantara 3's own default sits behind it at position two.
    settings.default_design_index = 0;
    settings.default_slide_settings_index = 0;
    settings.ensure_slide_settings_for_designs();

    // There is nothing left for the welcome wizard to ask.
    settings.wizard_completed = true;

    report
}

/// Where Cantara 2 may have left its configuration file.
///
/// The first entry is where this very copy of Cantara would put its own
/// configuration, which is the answer in every sandboxed case: a Flatpak sees
/// the Flatpak's configuration directory, a Snap the Snap's, and a package
/// from the distribution or the AUR the plain one under `~/.config`. The rest
/// are the places Free Pascal's `GetAppConfigFile` puts a file on Windows and
/// macOS, where it appends the application's name to the directory and the
/// exact shape has varied between compiler versions.
///
/// They are tried in order and the first one that is a file wins.
#[cfg(not(target_arch = "wasm32"))]
pub fn config_candidates() -> Vec<PathBuf> {
    // `~/.config` on Linux — honouring `XDG_CONFIG_HOME`, and so the sandbox —
    // `%LOCALAPPDATA%` on Windows, `~/Library/Application Support` on macOS.
    // The same question Cantara 3 asks for its own settings, which is what
    // makes the two agree about which installation they are.
    candidates_under(dirs::config_local_dir().as_deref(), dirs::home_dir().as_deref())
}

/// The same list, for a given pair of directories.
///
/// Split out so that the search can be exercised against a folder built for
/// the purpose. Asking the real environment would make the test say whatever
/// the machine it runs on happens to be set up like.
#[cfg(not(target_arch = "wasm32"))]
fn candidates_under(config: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(config) = config {
        candidates.push(config.join(CONFIG_FILE_NAME));
        // Some Free Pascal versions put the file in a folder of the
        // application's name instead. Windows and macOS do not care about the
        // capital, but a case-sensitive file system does.
        for folder in ["cantara", "Cantara"] {
            candidates.push(config.join(folder).join(CONFIG_FILE_NAME));
        }
        candidates.push(config.join("Cantara.cfg"));

        // Roaming application data, which is where an older Free Pascal put
        // it. `config_local_dir` is `%LOCALAPPDATA%` on Windows and
        // `config_dir` is `%APPDATA%`; everywhere else they are the same
        // folder and this adds nothing that is not deduplicated away.
        #[cfg(target_os = "windows")]
        if let Some(roaming) = dirs::config_dir() {
            candidates.push(roaming.join(CONFIG_FILE_NAME));
            candidates.push(roaming.join("Cantara").join(CONFIG_FILE_NAME));
        }
    }

    if let Some(home) = home {
        // The fallback Free Pascal uses when it cannot find a configuration
        // directory at all: a dot-folder named after the program.
        candidates.push(home.join(".cantara").join(CONFIG_FILE_NAME));
        #[cfg(target_os = "macos")]
        candidates.push(home.join("Library/Preferences").join(CONFIG_FILE_NAME));
    }

    // Two of these coincide on most systems — a plain Linux install has the
    // same folder twice — and looking in the same place twice is only slower.
    candidates.dedup();
    candidates
}

/// The Cantara 2 configuration file on this machine, if there is one.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_config() -> Option<PathBuf> {
    config_candidates().into_iter().find(|path| path.is_file())
}

/// Takes a Cantara 2 installation over, if this is a first start and there is
/// one to take over.
///
/// Called from [`Settings::load`](crate::logic::settings::Settings::load) with
/// settings that are still the defaults, so that the signal the whole program
/// reads is born holding the imported configuration rather than being
/// corrected a moment later.
///
/// Returns what was taken over, for the notice the user is shown once.
#[cfg(not(target_arch = "wasm32"))]
pub fn import_from_cantara_2(settings: &mut Settings) -> Option<LegacyImportReport> {
    let report = import_from_file(settings, &find_config()?)?;

    if let Err(error) = settings.try_save() {
        // The import still stands for this session; it is only the *next*
        // start that would ask again. Worth a line in the log and not worth a
        // dialog on top of the one the user is about to see.
        log::warn!("the imported settings could not be saved: {error}");
    }

    Some(report)
}

/// Reads one Cantara 2 configuration file into the settings.
///
/// Everything [`import_from_cantara_2`] does except finding the file and
/// saving the result — which is what makes it something a test can run against
/// a file it wrote itself.
#[cfg(not(target_arch = "wasm32"))]
pub fn import_from_file(settings: &mut Settings, path: &Path) -> Option<LegacyImportReport> {
    use rust_i18n::t;

    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            log::warn!("{} could not be read: {error}", path.display());
            return None;
        }
    };

    let config = LegacyConfig::read(&IniFile::parse(&text));
    let mut report = apply(
        settings,
        &config,
        t!("legacy_import.design_name").to_string(),
        t!("legacy_import.division_name").to_string(),
    );
    report.source = path.to_path_buf();

    Some(report)
}

/// The notice waiting to be shown, if an import happened during this start.
///
/// A one-shot letterbox rather than something kept in the settings: the notice
/// belongs to *this* start of the program, and a flag written to disk would
/// have to be written a second time to clear it — with a crash in between
/// leaving the user with a dialog about an import they were told about
/// yesterday.
#[cfg(not(target_arch = "wasm32"))]
static PENDING_NOTICE: std::sync::Mutex<Option<LegacyImportReport>> =
    std::sync::Mutex::new(None);

/// Leaves a notice for the interface to pick up.
#[cfg(not(target_arch = "wasm32"))]
pub fn leave_notice(report: LegacyImportReport) {
    if let Ok(mut pending) = PENDING_NOTICE.lock() {
        *pending = Some(report);
    }
}

/// Takes the notice, if there is one. The second caller gets nothing, which is
/// what makes the dialog appear exactly once.
#[cfg(not(target_arch = "wasm32"))]
pub fn take_notice() -> Option<LegacyImportReport> {
    PENDING_NOTICE.lock().ok()?.take()
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the design structs keep private fields, so `..Default::default()` is not \
              available outside the module that defines them"
)]
mod tests {
    use super::*;

    /// The file the question was asked with: a real Cantara 2 configuration,
    /// from an installation that has been in use for years.
    ///
    /// Two paths in it were changed to ones that cannot exist. Whether a
    /// folder is there is something these tests decide for themselves — the
    /// original paths are real on the machine the file came from, and a test
    /// that passes there and fails everywhere else is worse than no test.
    const REAL_CONFIG: &str = "\
[Size]
main-window-maximized=0
panel-mutliscreen-position=1032
editor-splitter-location=643

[Exporter]
pptxgenjs=1

[Config]
Repo-Path=/nonexistent-cantara-2/Liederverzeichnis
empty-Frame=1
Text-Color=clWhite
Background-Color=clBlack
Spoiler=1
Font-Name=Nimbus Sans [UKWN]
Font-Size=63
Font-Style=FFFF
TitleSlide=1
MetaDataFirstSlide=1
MetaDataLastSlide=1
MetaDataSyntax={%bible%}Bibelstellen: {bible}</br>{%author%}Autor: {author}</br>{%copyright%}Copyright: {copyright}
BackgroundPicture-Path=/nonexistent-cantara-2/background-1.jpg
BackgroundPicture=1
ImageBrightness=30
AutoWrap=4
AlignHorizontal=0
AlignVertical=1
Padding-Left=30
Padding-Top=15
Padding-Right=15
Padding-Bottom=15
BlackScreenOnEmpty=0
FadeTransition=1
FadeDurationMs=150
HideCursorInPresentation=0
";

    fn real_config() -> LegacyConfig {
        LegacyConfig::read(&IniFile::parse(REAL_CONFIG))
    }

    // ── The file ────────────────────────────────────────────────────────────

    #[test]
    fn a_real_configuration_file_reads_as_what_is_in_it() {
        let config = real_config();

        assert_eq!(
            config.repository_path,
            "/nonexistent-cantara-2/Liederverzeichnis"
        );
        assert_eq!(config.font_size, 63);
        assert_eq!(config.font_name, "Nimbus Sans [UKWN]");
        assert!(config.spoiler);
        assert!(config.title_slide);
        assert_eq!(config.align_horizontal, 0);
        assert_eq!(config.align_vertical, 1);
        assert_eq!(config.padding_left, 30);
        assert_eq!(config.image_brightness, 30);
        assert_eq!(config.auto_wrap, 4);
        assert!(!config.bold);
    }

    /// The settings live in `[Config]`; a key of the same name elsewhere in
    /// the file is a different setting and must not be picked up.
    #[test]
    fn sections_are_kept_apart() {
        let ini = IniFile::parse(
            "[Size]\nFont-Size=11\n\n[Config]\nFont-Size=63\n",
        );

        assert_eq!(ini.integer("Config", "Font-Size", 42), 63);
        assert_eq!(ini.integer("Size", "Font-Size", 42), 11);
        assert_eq!(ini.integer("Exporter", "Font-Size", 42), 42);
    }

    /// `TIniFile` does not care about capitals, and neither may this: a file
    /// written by one version and read by another differs in exactly that.
    #[test]
    fn names_are_matched_without_regard_to_case() {
        let ini = IniFile::parse("[CONFIG]\nFONT-SIZE=63\n");

        assert_eq!(ini.integer("Config", "Font-Size", 42), 63);
        assert_eq!(ini.integer("config", "font-size", 42), 63);
    }

    /// A template is full of `=`. Splitting anywhere but the first one would
    /// cut the user's metadata line in half.
    #[test]
    fn a_value_may_contain_the_separator() {
        let ini = IniFile::parse("[Config]\nMetaDataSyntax=a=b=c\n");

        assert_eq!(ini.get("Config", "MetaDataSyntax"), Some("a=b=c"));
    }

    /// Everything that is not a setting is passed over rather than taken as
    /// one, and a file that is nothing but rubbish still reads as a
    /// configuration in which nothing was set.
    #[test]
    fn what_is_not_a_setting_is_ignored() {
        let ini = IniFile::parse(
            "\u{feff}; a comment\n# another\n[Config]\nnonsense\n\nFont-Size=63\n[unclosed\n",
        );

        assert_eq!(ini.integer("Config", "Font-Size", 42), 63);

        let empty = IniFile::parse("hello\nthere\n");
        assert_eq!(empty.integer("Config", "Font-Size", 42), 42);
        assert_eq!(empty.string("Config", "Repo-Path", "nowhere"), "nowhere");
    }

    /// Windows line endings, and a file that does not end in one.
    #[test]
    fn carriage_returns_are_not_part_of_the_value() {
        let ini = IniFile::parse("[Config]\r\nRepo-Path=C:\\Songs\r\nFont-Size=63");

        assert_eq!(ini.get("Config", "Repo-Path"), Some("C:\\Songs"));
        assert_eq!(ini.integer("Config", "Font-Size", 42), 63);
    }

    /// `ReadBool` asks whether a number is zero, and `ReadInteger` falls back
    /// to the default rather than failing — so a hand-edited `yes` reads as
    /// whatever the setting would have been without it.
    #[test]
    fn switches_read_the_way_pascal_reads_them() {
        let ini = IniFile::parse("[Config]\na=1\nb=0\nc=-1\nd=yes\ne=$10\n");

        assert!(ini.boolean("Config", "a", false));
        assert!(!ini.boolean("Config", "b", true));
        assert!(ini.boolean("Config", "c", false));
        assert!(ini.boolean("Config", "d", true));
        assert!(!ini.boolean("Config", "d", false));
        assert_eq!(ini.integer("Config", "e", 0), 16);
    }

    /// A setting that is not in the file is one the user never touched, and
    /// what they saw was Cantara 2's default for it — not Cantara 3's.
    #[test]
    fn a_missing_setting_falls_back_to_what_cantara_2_did() {
        let config = LegacyConfig::read(&IniFile::parse("[Config]\n"));

        assert_eq!(config.font_size, 42);
        assert_eq!(config.auto_wrap, 4);
        assert_eq!(config.padding_left, 15);
        assert_eq!(config.align_horizontal, 1);
        assert_eq!(config.align_vertical, 1);
        assert!(config.empty_frame);
        assert!(config.spoiler);
        assert!(!config.title_slide);
        assert!(!config.show_background_picture);
        assert_eq!(config.text_color, Rgb::new(255, 255, 255));
        assert_eq!(config.background_color, Rgb::new(0, 0, 0));
    }

    // ── Colours ─────────────────────────────────────────────────────────────

    /// A `TColor` is blue first. Reading one as an HTML colour swaps every red
    /// and blue on the slide.
    #[test]
    fn a_pascal_colour_is_blue_green_red() {
        assert_eq!(color_of("clRed"), Some(Rgb::new(255, 0, 0)));
        assert_eq!(color_of("clBlue"), Some(Rgb::new(0, 0, 255)));
        assert_eq!(color_of("clWhite"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(color_of("clBlack"), Some(Rgb::new(0, 0, 0)));
        assert_eq!(color_of("clNavy"), Some(Rgb::new(0, 0, 128)));

        // …and a colour the dialog wrote as a number rather than a name.
        assert_eq!(color_of("$0000FF"), Some(Rgb::new(255, 0, 0)));
        assert_eq!(color_of("$00A0B0C0"), Some(Rgb::new(192, 176, 160)));
        assert_eq!(color_of("255"), Some(Rgb::new(255, 0, 0)));
    }

    /// Names are matched without regard to case, and something that is no
    /// colour at all leaves the default in place instead of turning the slide
    /// black.
    #[test]
    fn a_colour_that_cannot_be_read_is_no_colour() {
        assert_eq!(color_of("CLWHITE"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(color_of("clNotAColour"), None);
        assert_eq!(color_of(""), None);
        assert_eq!(color_of("   "), None);
        // A negative number is a system colour, resolved against a desktop
        // theme that is not here.
        assert_eq!(color_of("-2147483643"), None);

        let config = LegacyConfig::read(&IniFile::parse("[Config]\nText-Color=clNonsense\n"));
        assert_eq!(config.text_color, Rgb::new(255, 255, 255));
    }

    // ── Fonts ───────────────────────────────────────────────────────────────

    /// The font dialog appends the script it was picked for, and no font in
    /// the world is called that.
    #[test]
    fn the_font_dialogs_bracket_is_not_part_of_the_family() {
        assert_eq!(
            font_family_of("Nimbus Sans [UKWN]"),
            Some("Nimbus Sans".to_string())
        );
        assert_eq!(font_family_of("Arial"), Some("Arial".to_string()));
        // Cantara 2's way of saying "whatever the system uses".
        assert_eq!(font_family_of("default"), None);
        assert_eq!(font_family_of("Default"), None);
        assert_eq!(font_family_of(""), None);
        assert_eq!(font_family_of("  "), None);
        assert_eq!(font_family_of("[UKWN]"), None);
    }

    // ── The metadata template ───────────────────────────────────────────────

    /// The whole point of the old syntax: a song without an author does not
    /// project the word "Autor:" with nothing after it.
    #[test]
    fn a_condition_becomes_an_if_that_ends_with_the_line() {
        assert_eq!(
            convert_meta_syntax("{%author%}Autor: {author}"),
            "{{#if author}}Autor: {{author}}{{/if}}"
        );
    }

    /// The configuration file is one line per setting, so a template with
    /// several lines is stored with `</br>` where the breaks were — and each
    /// line closes its own conditions, as Cantara 2 did.
    #[test]
    fn a_stored_line_break_becomes_a_line_break() {
        let converted = convert_meta_syntax(
            "{%bible%}Bibelstellen: {bible}</br>{%author%}Autor: {author}",
        );

        assert_eq!(
            converted,
            "{{#if bible}}Bibelstellen: {{bible}}{{/if}}\n\
             {{#if author}}Autor: {{author}}{{/if}}"
        );
    }

    /// A tag with a hyphen in it is not a subtraction. Handlebars needs the
    /// brackets, and the real configuration file is full of such tags.
    #[test]
    fn a_name_that_is_not_an_identifier_is_bracketed() {
        assert_eq!(
            convert_meta_syntax(
                "{%ccli-songnumber%}CCLI: {ccli-songnumber} | {ccli-licensenumber}"
            ),
            "{{#if [ccli-songnumber]}}CCLI: {{[ccli-songnumber]}} | \
             {{[ccli-licensenumber]}}{{/if}}"
        );

        // Handlebars would read a leading digit as a number, too.
        assert_eq!(convert_meta_syntax("{2nd}"), "{{[2nd]}}");
    }

    /// `{%end%}` closes a condition early, so that a line can go on saying
    /// something after it.
    #[test]
    fn an_explicit_end_closes_the_condition_where_it_stands() {
        assert_eq!(
            convert_meta_syntax("{%author%}Autor: {author}{%end%} — immer"),
            "{{#if author}}Autor: {{author}}{{/if}} — immer"
        );
    }

    /// Two conditions on one line nest, and both are closed — an unbalanced
    /// template would refuse to compile and leave the user with no metadata at
    /// all.
    #[test]
    fn every_condition_is_closed() {
        let converted = convert_meta_syntax("{%a%}A {%b%}B");
        assert_eq!(converted, "{{#if a}}A {{#if b}}B{{/if}}{{/if}}");

        // An `{%end%}` with nothing open is dropped rather than written out
        // as a stray `{{/if}}`.
        assert_eq!(convert_meta_syntax("{%end%}text"), "text");

        // Whatever the template, the result compiles.
        for source in [
            REAL_CONFIG_META,
            "{%a%}{%end%}{%end%}{%b%}",
            "{ } {} {%%} plain text",
            "",
        ] {
            let converted = convert_meta_syntax(source);
            assert!(
                cantara_songlib::templating::MetaTemplate::parse(&converted).is_ok(),
                "{source:?} became {converted:?}, which does not compile"
            );
        }
    }

    const REAL_CONFIG_META: &str = "{%bible%}Bibelstellen: {bible}</br>{%author%}Autor: \
        {author}</br>{%text%}Text 詞: {text}</br>{%ccli-songnumber%}CCLI Liednummer: \
        {ccli-songnumber} | Lizensnummer: {ccli-licensenumber}";

    /// A brace that opens nothing is text, exactly as it was in Cantara 2.
    #[test]
    fn a_stray_brace_is_left_alone() {
        assert_eq!(convert_meta_syntax("a { b"), "a { b");
        assert_eq!(convert_meta_syntax("a } b"), "a } b");
        assert_eq!(convert_meta_syntax("{}"), "{}");
        assert_eq!(convert_meta_syntax("{  }"), "{  }");
    }

    /// Non-ASCII text passes through untouched — the real file has Chinese in
    /// it, and the conversion walks the template by character.
    #[test]
    fn text_that_is_not_ascii_survives() {
        assert_eq!(
            convert_meta_syntax("{%text%}Text 詞: {text}"),
            "{{#if text}}Text 詞: {{text}}{{/if}}"
        );
    }

    /// And the whole thing renders what Cantara 2 rendered.
    #[test]
    fn the_converted_template_says_what_the_old_one_said() {
        use cantara_songlib::song::Song;
        use cantara_songlib::templating::MetaTemplate;

        let converted = convert_meta_syntax(REAL_CONFIG_META);
        let template = MetaTemplate::parse(&converted).expect("a template");

        let mut song = Song::new("Amazing Grace");
        song.set_tag("author", "John Newton");
        let rendered = template.render_song(&song).expect("a line");

        assert_eq!(rendered, "Autor: John Newton");

        // A song with nothing at all yields no line rather than a row of
        // empty labels.
        assert_eq!(template.render_song(&Song::new("Untitled")), None);
    }

    // ── The design ──────────────────────────────────────────────────────────

    fn template_of(design: &PresentationDesign) -> crate::logic::settings::PresentationDesignTemplate {
        design
            .presentation_design_settings
            .template()
            .cloned()
            .expect("the imported design should carry a template")
    }

    #[test]
    fn the_design_is_what_the_old_settings_described() {
        let design = design_of(&real_config(), "Cantara 2".to_string());
        let template = template_of(&design);

        assert_eq!(design.name, "Cantara 2");
        assert_eq!(template.background_color, Rgb::new(0, 0, 0));
        assert_eq!(template.vertical_alignment, VerticalAlign::Middle);
        assert_eq!(template.background_transparency, 30);
        assert_eq!(template.padding.left, CssSize::Px(30.0));
        assert_eq!(template.padding.top, CssSize::Px(15.0));

        let main = &template.fonts[0];
        assert_eq!(main.font_size, CssSize::Pt(63.0));
        assert_eq!(main.color, Rgba::new(255, 255, 255, 255));
        assert_eq!(main.horizontal_alignment, HorizontalAlign::Left);
        assert_eq!(main.weight, 400);
        assert_eq!(
            main.font_family.as_ref().and_then(|f| f.family.clone()),
            Some("Nimbus Sans".to_string())
        );

        // Cantara 2 drew the spoiler at half the height of the lyrics and the
        // metadata at a third.
        assert_eq!(template.fonts[1].font_size, CssSize::Pt(31.5));
        assert_eq!(template.fonts[2].font_size, CssSize::Pt(21.0));
        assert_eq!(template.spoiler_index, Some(1));
        assert_eq!(template.meta_index, Some(2));
    }

    #[test]
    fn the_alignments_are_read_the_way_pascal_numbered_them() {
        for (stored, expected) in [
            (0, HorizontalAlign::Left),
            (1, HorizontalAlign::Centered),
            (2, HorizontalAlign::Right),
            // Anything else is what the enum's default was.
            (7, HorizontalAlign::Centered),
        ] {
            let mut config = real_config();
            config.align_horizontal = stored;
            assert_eq!(
                template_of(&design_of(&config, String::new())).fonts[0].horizontal_alignment,
                expected
            );
        }

        for (stored, expected) in [
            (0, VerticalAlign::Top),
            (1, VerticalAlign::Middle),
            (2, VerticalAlign::Bottom),
            (7, VerticalAlign::Middle),
        ] {
            let mut config = real_config();
            config.align_vertical = stored;
            assert_eq!(
                template_of(&design_of(&config, String::new())).vertical_alignment,
                expected
            );
        }
    }

    /// `Font-Style` is four letters — bold, italic, underline, struck through
    /// — and the first two have a counterpart in a design here.
    #[test]
    fn bold_and_italic_are_read_from_the_four_letters() {
        for (style, bold, italic) in [
            ("FFFF", false, false),
            ("TFFF", true, false),
            ("FTFF", false, true),
            ("TTFF", true, true),
            // The last two letters are underline and struck through, which a
            // design cannot say — and must not be mistaken for the first two.
            ("FFTT", false, false),
        ] {
            let config =
                LegacyConfig::read(&IniFile::parse(&format!("[Config]\nFont-Style={style}\n")));
            assert_eq!(config.bold, bold, "{style} read the wrong bold");
            assert_eq!(config.italic, italic, "{style} read the wrong italic");

            // …and each of the three blocks of the design is set the same way,
            // as Cantara 2 drew them.
            for font in template_of(&design_of(&config, String::new())).fonts {
                assert_eq!(font.is_bold(), bold, "{style}");
                assert_eq!(font.italic, italic, "{style}");
                assert_eq!(font.weight, if bold { 700 } else { 400 }, "{style}");
            }
        }
    }

    /// A style string that is missing, empty or too short is read as "nothing
    /// switched on" rather than being a reason to give up on the design.
    #[test]
    fn a_style_that_is_not_four_letters_is_nothing_switched_on() {
        for line in ["Font-Style=\n", "Font-Style=T\n", "", "Font-Style=nonsense\n"] {
            let config = LegacyConfig::read(&IniFile::parse(&format!("[Config]\n{line}")));
            assert!(!config.italic, "{line:?} switched italic on");
            if line == "Font-Style=T\n" {
                // One letter is a bold that was written down; the rest is not
                // there and is off.
                assert!(config.bold);
            } else {
                assert!(!config.bold, "{line:?} switched bold on");
            }
        }
    }

    /// Nonsense in the file must not produce a design that cannot be rendered:
    /// a font size of zero, or a negative padding, is worse than the default.
    #[test]
    fn absurd_numbers_are_brought_back_into_range() {
        let mut config = real_config();
        config.font_size = 0;
        config.padding_left = -40;
        config.image_brightness = 4000;
        let template = template_of(&design_of(&config, String::new()));

        assert_eq!(template.fonts[0].font_size, CssSize::Pt(1.0));
        assert_eq!(template.padding.left, CssSize::Px(0.0));
        assert_eq!(template.background_transparency, 100);

        // Cantara 2 stored the brightness with a sign it then ignored.
        let mut negative = real_config();
        negative.image_brightness = -30;
        assert_eq!(
            template_of(&design_of(&negative, String::new())).background_transparency,
            30
        );
    }

    /// A picture that has been moved since is left out. A design pointing at
    /// nothing is a black slide with no hint of why.
    #[test]
    fn a_background_picture_that_is_not_there_is_not_carried_over() {
        let config = real_config();
        assert!(config.show_background_picture);
        assert!(template_of(&design_of(&config, String::new())).background_image.is_none());
    }

    #[test]
    fn a_background_picture_that_is_there_is_carried_over() {
        let folder = tempfile::tempdir().expect("a folder");
        let picture = folder.path().join("background-1.jpg");
        std::fs::write(&picture, b"not really a picture").expect("written");

        let mut config = real_config();
        config.background_picture_path = picture.to_string_lossy().to_string();
        let template = template_of(&design_of(&config, String::new()));

        let image = template.background_image.expect("the picture");
        assert_eq!(image.as_source().path, picture);
        assert_eq!(image.as_source().name, "background-1");

        // …unless the switch was off, in which case the user had turned it off.
        config.show_background_picture = false;
        assert!(template_of(&design_of(&config, String::new())).background_image.is_none());
    }

    // ── The slide division ──────────────────────────────────────────────────

    #[test]
    fn the_division_is_what_the_old_settings_described() {
        let division = slide_settings_of(&real_config(), "Cantara 2".to_string());

        assert_eq!(division.name, "Cantara 2");
        assert!(division.settings.title_slide);
        assert!(division.settings.show_spoiler);
        assert!(division.settings.empty_last_slide);
        assert_eq!(division.settings.max_lines, Some(4));
        // Cantara 2's title slide *was* the metadata slide.
        assert!(division.settings.show_meta_information.title_slide);
        assert!(division.settings.show_meta_information.first_slide);
        assert!(division.settings.show_meta_information.last_slide);
    }

    /// Zero is Cantara 2's "do not break a block up at all", and a negative
    /// number is nonsense that means the same.
    #[test]
    fn no_wrapping_is_no_limit() {
        for wrap in [0, -1] {
            let mut config = real_config();
            config.auto_wrap = wrap;
            assert_eq!(slide_settings_of(&config, String::new()).settings.max_lines, None);
        }
    }

    /// Without a title slide there is no title slide to put metadata on.
    #[test]
    fn the_title_slides_metadata_follows_the_title_slide() {
        let mut config = real_config();
        config.title_slide = false;
        let division = slide_settings_of(&config, String::new());

        assert!(!division.settings.title_slide);
        assert!(!division.settings.show_meta_information.title_slide);
        assert!(division.settings.show_meta_information.first_slide);
    }

    // ── Applying it ─────────────────────────────────────────────────────────

    fn apply_to_fresh(config: &LegacyConfig) -> (Settings, LegacyImportReport) {
        let mut settings = Settings::default();
        let report = apply(
            &mut settings,
            config,
            "Cantara 2".to_string(),
            "Cantara 2".to_string(),
        );
        (settings, report)
    }

    /// What the user asked for: their settings first, Cantara 3's own second,
    /// so that switching back is picking from a list.
    #[test]
    fn the_imported_design_comes_first_and_the_default_second() {
        let (settings, _) = apply_to_fresh(&real_config());

        assert_eq!(settings.presentation_designs.len(), 2);
        assert_eq!(settings.presentation_designs[0].name, "Cantara 2");
        assert_eq!(settings.presentation_designs[1], PresentationDesign::default());

        assert_eq!(settings.song_slide_settings.len(), 2);
        assert_eq!(settings.song_slide_settings[0].name, "Cantara 2");
        assert_eq!(settings.song_slide_settings[1], SongSlideSettings::default());

        // And the imported pair is what a presentation is built with.
        assert_eq!(settings.default_design_index, 0);
        assert_eq!(settings.default_slide_settings_index, 0);
        assert_eq!(settings.default_presentation_design().name, "Cantara 2");
        assert_eq!(settings.default_song_slide_settings().max_lines, Some(4));
    }

    /// An import answers everything the welcome wizard would have asked.
    #[test]
    fn an_import_takes_the_place_of_the_wizard() {
        let (settings, _) = apply_to_fresh(&real_config());
        assert!(settings.wizard_completed);
    }

    #[test]
    fn the_library_folder_becomes_the_repository() {
        let folder = tempfile::tempdir().expect("a folder");
        let mut config = real_config();
        config.repository_path = folder.path().to_string_lossy().to_string();

        let (settings, report) = apply_to_fresh(&config);

        assert_eq!(settings.repositories.len(), 1);
        assert_eq!(
            settings.repository_folder(0),
            Some(folder.path().to_path_buf())
        );
        assert_eq!(report.repository, settings.repositories.first().map(|r| r.name.clone()));
        assert_eq!(report.missing_repository, None);
    }

    /// A folder that has been moved since is not added as a repository that
    /// reads nothing — but the rest of the import still happens, and the user
    /// is told which folder to go and look for.
    #[test]
    fn a_library_folder_that_is_gone_is_reported_rather_than_added() {
        let (settings, report) = apply_to_fresh(&real_config());

        assert!(settings.repositories.is_empty());
        assert_eq!(
            report.missing_repository.as_deref(),
            Some("/nonexistent-cantara-2/Liederverzeichnis")
        );
        assert_eq!(report.repository, None);
        // The design still came over.
        assert_eq!(settings.presentation_designs[0].name, "Cantara 2");
        assert!(settings.wizard_completed);
    }

    /// A Cantara 2 that was installed and never configured has no repository
    /// path at all. That is not a missing folder, it is no folder.
    #[test]
    fn an_empty_library_folder_is_not_reported_as_missing() {
        let mut config = real_config();
        config.repository_path = "   ".to_string();

        let (settings, report) = apply_to_fresh(&config);

        assert!(settings.repositories.is_empty());
        assert_eq!(report.missing_repository, None);
        assert_eq!(report.repository, None);
    }

    /// The report says what the user is about to be told, and nothing it
    /// cannot back up.
    #[test]
    fn the_report_describes_the_import() {
        let (_, report) = apply_to_fresh(&real_config());

        assert_eq!(report.font_family, Some("Nimbus Sans".to_string()));
        assert!(!report.background_image);
    }

    /// The whole way through, from the bytes of a real file to settings that
    /// could be saved and read back.
    #[test]
    fn a_whole_file_becomes_settings_that_survive_being_written_out() {
        let config = LegacyConfig::read(&IniFile::parse(REAL_CONFIG));
        let (settings, _) = apply_to_fresh(&config);

        let json = serde_json::to_string(&settings).expect("encodable");
        let read: Settings = serde_json::from_str(&json).expect("readable");

        assert!(read == settings);
        assert_eq!(read.presentation_designs[0].name, "Cantara 2");
    }

    /// An empty file is still a Cantara 2 installation — one that was never
    /// configured. It must import as Cantara 2's defaults rather than fail.
    #[test]
    fn an_empty_file_imports_as_cantara_2s_defaults() {
        let config = LegacyConfig::read(&IniFile::parse(""));
        let (settings, report) = apply_to_fresh(&config);

        assert_eq!(settings.presentation_designs.len(), 2);
        assert_eq!(
            template_of(&settings.presentation_designs[0]).fonts[0].font_size,
            CssSize::Pt(42.0)
        );
        assert_eq!(settings.song_slide_settings[0].settings.max_lines, Some(4));
        assert_eq!(report.font_family, None);
        assert!(settings.wizard_completed);
    }

    // ── Finding the file ────────────────────────────────────────────────────

    /// The file beside this program's own configuration wins over every other
    /// place, which is what makes a Flatpak take over the Flatpak's Cantara 2
    /// and a Snap the Snap's: both ask their own sandbox where configuration
    /// lives, and both get the answer for the sandbox they are in.
    #[test]
    fn the_sandboxs_own_configuration_directory_is_searched_first() {
        let root = tempfile::tempdir().expect("a folder");
        let config = root.path().join("config");
        let home = root.path().join("home");
        std::fs::create_dir_all(config.join("cantara")).expect("created");
        std::fs::create_dir_all(home.join(".cantara")).expect("created");

        let candidates = candidates_under(Some(&config), Some(&home));
        let first_found = |candidates: &[PathBuf]| {
            candidates.iter().find(|path| path.is_file()).cloned()
        };

        // Nothing there at all: nothing is found, and the wizard runs.
        assert_eq!(first_found(&candidates), None);

        // Only the folder-shaped location — an older Free Pascal.
        let in_folder = config.join("cantara").join(CONFIG_FILE_NAME);
        std::fs::write(&in_folder, REAL_CONFIG).expect("written");
        assert_eq!(first_found(&candidates), Some(in_folder));

        // …and once the file is where Cantara 2 actually writes it, that one
        // wins.
        let beside = config.join(CONFIG_FILE_NAME);
        std::fs::write(&beside, REAL_CONFIG).expect("written");
        assert_eq!(first_found(&candidates), Some(beside));
    }

    /// Every candidate is a file called `cantara.cfg`, and no place is
    /// searched twice — which on a plain Linux install it otherwise would be,
    /// the configuration directory and the home directory answering the same.
    #[test]
    fn the_places_searched_are_all_cantara_2_configuration_files() {
        let candidates = config_candidates();
        assert!(!candidates.is_empty(), "nowhere at all was searched");

        for candidate in &candidates {
            assert!(
                candidate
                    .file_name()
                    .map(|name| name.to_string_lossy().eq_ignore_ascii_case(CONFIG_FILE_NAME))
                    .unwrap_or(false),
                "{} is not a Cantara 2 configuration file",
                candidate.display()
            );
        }

        let mut sorted = candidates.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), candidates.len(), "{candidates:?} has a repeat");
    }

    /// The whole chain, from a file on disk to the settings the program runs
    /// on: read, parsed, converted, and in place of the wizard.
    #[test]
    fn a_file_on_disk_becomes_the_settings_the_program_starts_with() {
        let folder = tempfile::tempdir().expect("a folder");
        let library = folder.path().join("Liederverzeichnis");
        std::fs::create_dir(&library).expect("created");

        let path = folder.path().join(CONFIG_FILE_NAME);
        std::fs::write(
            &path,
            REAL_CONFIG.replace(
                "/nonexistent-cantara-2/Liederverzeichnis",
                &library.to_string_lossy(),
            ),
        )
        .expect("written");

        let mut settings = Settings::default();
        let report = import_from_file(&mut settings, &path).expect("imported");

        assert_eq!(report.source, path);
        assert!(report.repository.is_some());
        assert_eq!(settings.repository_folder(0), Some(library));
        assert!(settings.wizard_completed);
        assert_eq!(
            template_of(&settings.presentation_designs[0]).fonts[0].font_size,
            CssSize::Pt(63.0)
        );
        assert_eq!(
            settings.song_slide_settings[0].settings.meta_syntax,
            "{{#if bible}}Bibelstellen: {{bible}}{{/if}}\n\
             {{#if author}}Autor: {{author}}{{/if}}\n\
             {{#if copyright}}Copyright: {{copyright}}{{/if}}"
        );
    }

    /// A path that is not a file at all is not an import. The wizard runs, and
    /// nothing has been half-changed on the way to finding that out.
    #[test]
    fn a_file_that_is_not_there_changes_nothing() {
        let folder = tempfile::tempdir().expect("a folder");
        let mut settings = Settings::default();

        assert_eq!(
            import_from_file(&mut settings, &folder.path().join(CONFIG_FILE_NAME)),
            None
        );
        assert!(settings == Settings::default());
        assert!(!settings.wizard_completed);
    }

    /// The notice is handed over exactly once — that is what makes the dialog
    /// appear on the first start and not on the second.
    #[test]
    fn the_notice_is_handed_over_once() {
        let report = LegacyImportReport {
            source: PathBuf::from("/somewhere/cantara.cfg"),
            repository: None,
            missing_repository: None,
            font_family: None,
            background_image: false,
        };

        leave_notice(report.clone());
        assert_eq!(take_notice(), Some(report));
        assert_eq!(take_notice(), None);
    }
}
