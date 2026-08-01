//! Cantara's frontend depends on npm packages which this build script will automatically install if the 'dist/' folder does not exist in the repository.
//!
//! Additionally, when the `CANTARA_BUNDLED_REPOS` environment variable is set (comma-separated
//! "owner/repo" entries), this build script scans the `bundled_repos/{owner}/{repo}` directories
//! and generates a Rust source file that embeds all supported files (songs, images, PDFs) via
//! `include_bytes!`. This allows WebAssembly builds to ship with pre-bundled repository content
//! so that no external fetching or CORS workarounds are needed at runtime.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// File extensions that Cantara supports as source files.
///
/// Kept in step with `SourceFileType` in `src/logic/sourcefiles.rs`; this list
/// decides what a bundled repository ships to the web build.
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "song", "ccli", "yml", "yaml", "cssf", "jpg", "jpeg", "png", "pdf", "md",
];

/// npm packages whose files are referenced by `asset!()` at compile time.
/// If one of them is missing the build fails with a confusing macro error, so
/// their presence is checked before anything else runs.
const REQUIRED_NPM_PACKAGES: &[&str] = &["pdfjs-dist", "abcjs", "pptxgenjs", "@picocss/pico"];

fn main() {
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=assets/fonts");
    ensure_npm_packages();
    generate_bundled_repos_data();
    generate_bundled_fonts_data();
}

/// Generates `bundled_fonts_data.rs` in `OUT_DIR` listing the fonts in
/// `assets/fonts/` as `(family name, file name)` pairs.
///
/// The family name is taken from the file name, so `Open Sans.ttf` becomes the
/// family "Open Sans". That keeps the step simple and predictable: what a user
/// sees in the settings is what they named the file.
fn generate_bundled_fonts_data() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
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

    let mut file = fs::File::create(&dest_path).expect("Failed to create bundled_fonts_data.rs");
    writeln!(
        file,
        "/// Fonts shipped in `assets/fonts/`, as (family name, file name)."
    )
    .unwrap();
    writeln!(file, "pub const BUNDLED_FONTS: &[(&str, &str)] = &[").unwrap();
    for (family, file_name) in &fonts {
        writeln!(file, "    ({:?}, {:?}),", family, file_name).unwrap();
    }
    writeln!(file, "];").unwrap();
}

/// Install the npm dependencies if any of them is missing.
///
/// Checking for the `node_modules` directory alone is not enough: adding a
/// package to `package.json` leaves the directory in place, so the new package
/// would never be installed and the `asset!()` referencing it would fail.
fn ensure_npm_packages() {
    let missing: Vec<&str> = REQUIRED_NPM_PACKAGES
        .iter()
        .copied()
        .filter(|package| fs::metadata(Path::new("node_modules").join(package)).is_err())
        .collect();

    if missing.is_empty() {
        return;
    }

    println!(
        "cargo:warning=installing missing npm packages: {}",
        missing.join(", ")
    );

    let output = Command::new("npm")
        .arg("install")
        .output()
        .expect("Failed to execute npm install. Make sure that you have npm installed.");

    if !output.status.success() {
        eprintln!(
            "npm install failed: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        panic!("npm install failed");
    }

    let still_missing: Vec<&str> = missing
        .iter()
        .copied()
        .filter(|package| fs::metadata(Path::new("node_modules").join(package)).is_err())
        .collect();
    if !still_missing.is_empty() {
        panic!(
            "npm install did not provide these packages: {}",
            still_missing.join(", ")
        );
    }
}

/// Generates `bundled_repos_data.rs` in `OUT_DIR` containing embedded repository file data.
/// When `CANTARA_BUNDLED_REPOS` is not set, the generated file contains empty constants.
fn generate_bundled_repos_data() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = Path::new(&out_dir).join("bundled_repos_data.rs");

    let repos_str = std::env::var("CANTARA_BUNDLED_REPOS").unwrap_or_default();
    let repos: Vec<&str> = if repos_str.is_empty() {
        vec![]
    } else {
        repos_str.split(',').map(|s| s.trim()).collect()
    };

    let mut f = fs::File::create(&dest_path).expect("Failed to create bundled_repos_data.rs");

    // Write BUNDLED_REPOS constant: list of (owner, repo) tuples
    writeln!(f, "/// List of bundled repositories as (owner, repo) tuples.").unwrap();
    writeln!(f, "pub const BUNDLED_REPOS: &[(&str, &str)] = &[").unwrap();
    for repo_id in &repos {
        let parts: Vec<&str> = repo_id.splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            writeln!(f, "    (\"{}\", \"{}\"),", parts[0], parts[1]).unwrap();
        }
    }
    writeln!(f, "];").unwrap();
    writeln!(f).unwrap();

    // Write BUNDLED_FILES constant: list of (vfs_path, file_bytes) tuples
    writeln!(
        f,
        "/// Embedded file data for bundled repositories as (vfs_path, bytes) tuples."
    )
    .unwrap();
    writeln!(f, "pub const BUNDLED_FILES: &[(&str, &[u8])] = &[").unwrap();
    for repo_id in &repos {
        let repo_path_str = format!("bundled_repos/{}", repo_id);
        let repo_path = Path::new(&repo_path_str);
        if repo_path.exists() && repo_path.is_dir() {
            walk_and_write_files(&mut f, repo_path, repo_id);
            // Re-run build script if the bundled repo directory changes
            println!("cargo:rerun-if-changed={}", repo_path_str);
        }
    }
    writeln!(f, "];").unwrap();

    // Pass the env var through so the Rust code can read it at compile time
    if !repos_str.is_empty() {
        println!("cargo:rustc-env=CANTARA_BUNDLED_REPOS={}", repos_str);
    }

    // Re-run if the env var changes
    println!("cargo:rerun-if-env-changed=CANTARA_BUNDLED_REPOS");
}

/// Recursively walks `base_path` and writes `include_bytes!` entries for supported files.
fn walk_and_write_files(f: &mut fs::File, base_path: &Path, repo_id: &str) {
    visit_dir(f, base_path, base_path, repo_id, 0);
}

fn visit_dir(f: &mut fs::File, dir: &Path, base_path: &Path, repo_id: &str, depth: usize) {
    if depth > 6 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip .git directory
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            visit_dir(f, &path, base_path, repo_id, depth + 1);
        } else if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
        {
            let ext_lower = ext.to_lowercase();
            if SUPPORTED_EXTENSIONS.iter().any(|&e| e == ext_lower) {
                let rel_path = path.strip_prefix(base_path).unwrap();
                // Use web-github:// prefix so existing WASM VFS code finds the files
                let vfs_path = format!(
                    "web-github://{}/{}",
                    repo_id,
                    rel_path.display().to_string().replace('\\', "/")
                );
                let abs_path = fs::canonicalize(&path).unwrap();
                writeln!(
                    f,
                    "    (\"{}\", include_bytes!(\"{}\")),",
                    vfs_path,
                    abs_path.display().to_string().replace('\\', "/")
                )
                .unwrap();
            }
        }
    }
}
