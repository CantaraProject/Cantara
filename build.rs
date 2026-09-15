//! Cantara's frontend depends on npm packages which this build script will automatically install if the 'dist/' folder does not exist in the repository.
//!
//! Additionally, when the `CANTARA_BUNDLED_REPOS` environment variable is set (comma-separated
//! "owner/repo" entries), this build script scans the `bundled_repos/{owner}/{repo}` directories
//! and generates a Rust source file that embeds all supported files (songs, images, PDFs) via
//! `include_bytes!`. This allows WebAssembly builds to ship with pre-bundled repository content
//! so that no external fetching or CORS workarounds are needed at runtime.

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// What every step of this script gives back.
///
/// A build script has nobody to show a dialog to, so a failure is reported the
/// only way that reaches anyone: by returning it, which cargo prints and which
/// stops the build. Written out rather than left to `unwrap`, so the message
/// names what could not be done instead of a line number in this file.
type BuildResult<T = ()> = Result<T, Box<dyn Error>>;

/// File extensions that Cantara supports as source files.
///
/// Kept in step with `SourceFileType` in `src/logic/sourcefiles.rs`; this list
/// decides what a bundled repository ships to the web build.
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "song", "ccli", "yml", "yaml", "jpg", "jpeg", "png", "pdf", "md",
];

/// npm packages whose files are referenced by `asset!()` at compile time.
/// If one of them is missing the build fails with a confusing macro error, so
/// their presence is checked before anything else runs.
const REQUIRED_NPM_PACKAGES: &[&str] = &["pdfjs-dist", "abcjs", "pptxgenjs", "@picocss/pico"];

/// Where the about page's prose lives, and the shape of the file names in it.
///
/// One file per language, named by the primary subtag — `de`, not `de-DE` —
/// which is the convention `locales/` already uses. See
/// `docs/specs/0005-add-info-page.md`, question 3.
const ABOUT_DIR: &str = "docs/about";
const ABOUT_PREFIX: &str = "cantara-info-";

/// The language every build must have a text for.
///
/// A missing translation is a normal state and falls back to this one. A
/// missing *fallback* is a broken program that looks perfectly healthy — an
/// about page with an empty body — so it stops the build instead of shipping.
const ABOUT_FALLBACK: &str = "en";

fn main() -> BuildResult {
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=assets/fonts");
    ensure_npm_packages()?;
    generate_bundled_repos_data()?;
    generate_bundled_fonts_data()?;
    generate_about_data()?;
    Ok(())
}

/// Generates `about_data.rs` in `OUT_DIR`: the about page's prose for every
/// language it has been written in, and the year this binary was built.
///
/// Generated rather than a hand-written list, for the same reason the fonts
/// are: adding a language becomes adding a file, which is the rule `locales/`
/// already follows. A match arm per language would be a second place to
/// forget.
fn generate_about_data() -> BuildResult {
    let out_dir = std::env::var("OUT_DIR").map_err(|_| "OUT_DIR is not set")?;
    let dest_path = Path::new(&out_dir).join("about_data.rs");

    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").map_err(|_| "CARGO_MANIFEST_DIR is not set")?;

    // The language of each file, paired with the absolute path it will be
    // included from. Absolute, because the generated file is compiled from
    // `OUT_DIR` and a path relative to this script would not resolve there.
    let mut texts: Vec<(String, String)> = Vec::new();

    if let Ok(entries) = fs::read_dir(ABOUT_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
                continue;
            }
            let Some(language) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.strip_prefix(ABOUT_PREFIX))
            else {
                continue;
            };

            let absolute = Path::new(&manifest_dir).join(&path);
            let Some(absolute) = absolute.to_str() else {
                continue;
            };
            texts.push((language.to_lowercase(), absolute.to_string()));
        }
    }

    // Sorted, so that the generated file is the same for the same inputs
    // whatever order the directory happened to be read in.
    texts.sort();

    if !texts.iter().any(|(language, _)| language == ABOUT_FALLBACK) {
        return Err(format!(
            "{ABOUT_DIR}/{ABOUT_PREFIX}{ABOUT_FALLBACK}.md is missing — \
             the about page has no text to fall back to"
        )
        .into());
    }

    let mut file = fs::File::create(&dest_path)
        .map_err(|error| format!("{} could not be created: {error}", dest_path.display()))?;

    writeln!(
        file,
        "/// The about page's prose, by language, sorted by the primary subtag."
    )?;
    writeln!(file, "pub const ABOUT_TEXTS: &[(&str, &str)] = &[")?;
    for (language, path) in &texts {
        writeln!(file, "    ({:?}, include_str!({:?})),", language, path)?;
    }
    writeln!(file, "];")?;
    writeln!(file)?;

    writeln!(
        file,
        "/// The year this binary was built — see `logic::about::copyright`."
    )?;
    writeln!(file, "pub const BUILD_YEAR: i32 = {};", build_year())?;

    println!("cargo:rerun-if-changed={ABOUT_DIR}");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    Ok(())
}

/// The year to put at the end of the copyright line.
///
/// `SOURCE_DATE_EPOCH` where the build sets it, the wall clock otherwise. That
/// is the conventional way to keep a build reproducible: the same source and
/// the same epoch give the same binary, where the clock would give a different
/// one on either side of New Year.
///
/// **This does not rebuild by itself.** A build script reruns when a file or a
/// named environment variable changes, and "the year changed" is neither, so a
/// development build carried across New Year shows the old year until
/// something else forces a rebuild. A release build is always cold, so the
/// released binary is right; it is only worth knowing before someone reports
/// it as a defect.
fn build_year() -> i32 {
    use chrono::Datelike;

    let stamped = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|epoch| epoch.trim().parse::<i64>().ok())
        .and_then(|epoch| chrono::DateTime::from_timestamp(epoch, 0));

    match stamped {
        Some(stamped) => stamped.year(),
        None => chrono::Local::now().year(),
    }
}

/// Generates `bundled_fonts_data.rs` in `OUT_DIR` listing the fonts in
/// `assets/fonts/` as `(family name, file name)` pairs.
///
/// The family name is taken from the file name, so `Open Sans.ttf` becomes the
/// family "Open Sans". That keeps the step simple and predictable: what a user
/// sees in the settings is what they named the file.
fn generate_bundled_fonts_data() -> BuildResult {
    let out_dir = std::env::var("OUT_DIR").map_err(|_| "OUT_DIR is not set")?;
    let dest_path = Path::new(&out_dir).join("bundled_fonts_data.rs");

    let mut fonts: Vec<(String, String)> = Vec::new();

    if let Ok(entries) = fs::read_dir("assets/fonts") {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !matches!(extension.as_str(), "ttf" | "otf" | "woff" | "woff2") {
                continue;
            }

            let family = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(file_name)
                .to_string();

            fonts.push((family, file_name.to_string()));
        }
    }

    fonts.sort();

    let mut file = fs::File::create(&dest_path)
        .map_err(|error| format!("{} could not be created: {error}", dest_path.display()))?;
    writeln!(
        file,
        "/// Fonts shipped in `assets/fonts/`, as (family name, file name)."
    )?;
    writeln!(file, "pub const BUNDLED_FONTS: &[(&str, &str)] = &[")?;
    for (family, file_name) in &fonts {
        writeln!(file, "    ({:?}, {:?}),", family, file_name)?;
    }
    writeln!(file, "];")?;

    Ok(())
}

/// Install the npm dependencies if any of them is missing.
///
/// Checking for the `node_modules` directory alone is not enough: adding a
/// package to `package.json` leaves the directory in place, so the new package
/// would never be installed and the `asset!()` referencing it would fail.
fn ensure_npm_packages() -> BuildResult {
    let missing: Vec<&str> = REQUIRED_NPM_PACKAGES
        .iter()
        .copied()
        .filter(|package| fs::metadata(Path::new("node_modules").join(package)).is_err())
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    println!(
        "cargo:warning=installing missing npm packages: {}",
        missing.join(", ")
    );

    let output = Command::new("npm").arg("install").output().map_err(|error| {
        format!("npm install could not be run ({error}). Make sure that you have npm installed.")
    })?;

    if !output.status.success() {
        return Err(format!(
            "npm install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let still_missing: Vec<&str> = missing
        .iter()
        .copied()
        .filter(|package| fs::metadata(Path::new("node_modules").join(package)).is_err())
        .collect();
    if !still_missing.is_empty() {
        return Err(format!(
            "npm install did not provide these packages: {}",
            still_missing.join(", ")
        )
        .into());
    }

    Ok(())
}

/// Generates `bundled_repos_data.rs` in `OUT_DIR` containing embedded repository file data.
/// When `CANTARA_BUNDLED_REPOS` is not set, the generated file contains empty constants.
fn generate_bundled_repos_data() -> BuildResult {
    let out_dir = std::env::var("OUT_DIR").map_err(|_| "OUT_DIR is not set")?;
    let dest_path = Path::new(&out_dir).join("bundled_repos_data.rs");

    let repos_str = std::env::var("CANTARA_BUNDLED_REPOS").unwrap_or_default();
    let repos: Vec<&str> = if repos_str.is_empty() {
        vec![]
    } else {
        repos_str.split(',').map(|s| s.trim()).collect()
    };

    let mut f = fs::File::create(&dest_path)
        .map_err(|error| format!("{} could not be created: {error}", dest_path.display()))?;

    // Write BUNDLED_REPOS constant: list of (owner, repo) tuples
    writeln!(f, "/// List of bundled repositories as (owner, repo) tuples.")?;
    writeln!(f, "pub const BUNDLED_REPOS: &[(&str, &str)] = &[")?;
    for repo_id in &repos {
        let parts: Vec<&str> = repo_id.splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            writeln!(f, "    (\"{}\", \"{}\"),", parts[0], parts[1])?;
        }
    }
    writeln!(f, "];")?;
    writeln!(f)?;

    // Write BUNDLED_FILES constant: list of (vfs_path, file_bytes) tuples
    writeln!(
        f,
        "/// Embedded file data for bundled repositories as (vfs_path, bytes) tuples."
    )?;
    writeln!(f, "pub const BUNDLED_FILES: &[(&str, &[u8])] = &[")?;
    for repo_id in &repos {
        let repo_path_str = format!("bundled_repos/{}", repo_id);
        let repo_path = Path::new(&repo_path_str);
        if repo_path.exists() && repo_path.is_dir() {
            walk_and_write_files(&mut f, repo_path, repo_id)?;
            // Re-run build script if the bundled repo directory changes
            println!("cargo:rerun-if-changed={}", repo_path_str);
        }
    }
    writeln!(f, "];")?;

    // Pass the env var through so the Rust code can read it at compile time
    if !repos_str.is_empty() {
        println!("cargo:rustc-env=CANTARA_BUNDLED_REPOS={}", repos_str);
    }

    // Re-run if the env var changes
    println!("cargo:rerun-if-env-changed=CANTARA_BUNDLED_REPOS");

    Ok(())
}

/// Recursively walks `base_path` and writes `include_bytes!` entries for supported files.
fn walk_and_write_files(f: &mut fs::File, base_path: &Path, repo_id: &str) -> BuildResult {
    visit_dir(f, base_path, base_path, repo_id, 0)
}

fn visit_dir(
    f: &mut fs::File,
    dir: &Path,
    base_path: &Path,
    repo_id: &str,
    depth: usize,
) -> BuildResult {
    if depth > 6 {
        return Ok(());
    }
    // A directory that cannot be read bundles nothing rather than failing the
    // build: what is embedded is whatever is there to embed.
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip .git directory
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            visit_dir(f, &path, base_path, repo_id, depth + 1)?;
        } else if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
        {
            let ext_lower = ext.to_lowercase();
            if SUPPORTED_EXTENSIONS.iter().any(|&e| e == ext_lower) {
                // Both of these come from walking `base_path` itself, so a
                // failure means the tree changed under the build — reported
                // rather than embedded as something wrong.
                let rel_path = path.strip_prefix(base_path).map_err(|error| {
                    format!("{} is not inside {}: {error}", path.display(), base_path.display())
                })?;
                // Use web-github:// prefix so existing WASM VFS code finds the files
                let vfs_path = format!(
                    "web-github://{}/{}",
                    repo_id,
                    rel_path.display().to_string().replace('\\', "/")
                );
                let abs_path = fs::canonicalize(&path)
                    .map_err(|error| format!("{} could not be resolved: {error}", path.display()))?;
                writeln!(
                    f,
                    "    (\"{}\", include_bytes!(\"{}\")),",
                    vfs_path,
                    abs_path.display().to_string().replace('\\', "/")
                )?;
            }
        }
    }

    Ok(())
}
