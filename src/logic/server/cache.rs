//! A server's libraries on this computer.
//!
//! ```text
//! {cache}/cantara/servers/{instance}/{library}/index.json
//! {cache}/cantara/servers/{instance}/{library}/files/{version}/{file name}
//! ```
//!
//! A version's uuid names exactly one content, forever — the server never
//! changes what is stored under it. So a file in the cache is never stale and
//! never checked again: a new version of a song is a new uuid, fetched once,
//! and the old directory is collected when nothing has referred to it for a
//! while. That is what makes a library usable with no network at all: the
//! last index and the files it names are all there is to read.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use cantara_api::LibraryIndex;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{client_for, display_name, remember_item, Client, ServerItem};
use crate::logic::sourcefiles::{SourceFile, SourceFileType};

/// How long a version nothing refers to any more is kept.
const KEEP_UNREFERENCED: Duration = Duration::from_secs(30 * 24 * 60 * 60);

pub(crate) fn cache_root() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        crate::logic::settings::get_settings_folder().map(|dir| dir.join("cache"))
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::cache_dir()
            .map(|dir| dir.join("cantara"))
            .or_else(|| crate::logic::settings::get_settings_folder().map(|dir| dir.join("cache")))
    }
}

pub fn library_dir(instance: Uuid, library: Uuid) -> Option<PathBuf> {
    Some(cache_root()?.join("servers").join(instance.to_string()).join(library.to_string()))
}

/// Everything cached for an instance — removed when the instance is.
pub fn remove_instance(instance: Uuid) {
    if let Some(root) = cache_root() {
        let _ = std::fs::remove_dir_all(root.join("servers").join(instance.to_string()));
    }
}

fn read_index(dir: &Path) -> Option<LibraryIndex> {
    let text = std::fs::read_to_string(dir.join("index.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_index(dir: &Path, index: &LibraryIndex) {
    let written = std::fs::create_dir_all(dir).and_then(|()| {
        let text = serde_json::to_string(index).map_err(std::io::Error::other)?;
        let temporary = dir.join("index.json.part");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, dir.join("index.json"))
    });
    if let Err(error) = written {
        log::warn!("the index of a server library cannot be cached: {error}");
    }
}

/// What a client knows after applying what the server answered to what it
/// knew before.
pub fn merge(known: Option<LibraryIndex>, answer: LibraryIndex) -> LibraryIndex {
    let Some(known) = known.filter(|_| !answer.complete) else {
        return LibraryIndex {
            complete: true,
            removed: Vec::new(),
            ..answer
        };
    };
    let mut entries: BTreeMap<Uuid, cantara_api::IndexEntry> =
        known.entries.into_iter().map(|entry| (entry.lineage, entry)).collect();
    for removed in &answer.removed {
        entries.remove(removed);
    }
    for entry in answer.entries {
        entries.insert(entry.lineage, entry);
    }
    let mut entries: Vec<_> = entries.into_values().collect();
    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    LibraryIndex {
        library: answer.library,
        revision: answer.revision,
        complete: true,
        entries,
        removed: Vec::new(),
    }
}

/// A file name that is safe as one component of a path.
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim_start_matches('.').to_string();
    if cleaned.is_empty() { "file".to_string() } else { cleaned }
}

/// Fetches a version into `target`, checking that what arrived is what the
/// index says it is.
async fn download(client: &Client, tenant: Uuid, version: Uuid, sha256: &str, target: &Path) -> Result<(), String> {
    let mut response = client.content(tenant, version).await.map_err(|error| error.to_string())?;
    let parent = target.parent().ok_or("no directory")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let partial = target.with_extension("part");
    let mut file = std::fs::File::create(&partial).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        hash.update(&chunk);
        file.write_all(&chunk).map_err(|error| error.to_string())?;
    }
    file.flush().map_err(|error| error.to_string())?;
    drop(file);
    let arrived = format!("{:x}", hash.finalize());
    if arrived != sha256 {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("the content of {version} arrived damaged"));
    }
    std::fs::rename(&partial, target).map_err(|error| error.to_string())
}

/// How long a picture, PDF or video of a library that is not kept whole
/// stays after it was last used.
const KEEP_UNUSED_MEDIA: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Removes versions no entry of `index` names, once they have been unused
/// for [`KEEP_UNREFERENCED`] — and, in a library that is not kept whole, the
/// pictures, PDFs and videos nobody has used for [`KEEP_UNUSED_MEDIA`].
fn collect_garbage(dir: &Path, index: &LibraryIndex, offline: bool) {
    let Ok(versions) = std::fs::read_dir(dir.join("files")) else {
        return;
    };
    let current: std::collections::HashMap<String, bool> = index
        .entries
        .iter()
        .map(|entry| (entry.version.to_string(), entry.kind.is_text()))
        .collect();
    for version in versions.flatten() {
        let name = version.file_name().to_string_lossy().to_string();
        let keep_for = match current.get(&name) {
            Some(true) => continue,
            Some(false) if offline => continue,
            Some(false) => KEEP_UNUSED_MEDIA,
            None => KEEP_UNREFERENCED,
        };
        let old = unused_for(&version.path()).is_none_or(|age| age > keep_for);
        if old {
            let _ = std::fs::remove_dir_all(version.path());
        }
    }
}

/// The files of a server library: brought up to date where the server can be
/// reached, from the cache where it cannot.
pub async fn files_of(instance: Uuid, tenant: Uuid, library: Uuid, offline: bool) -> Vec<SourceFile> {
    let Some(dir) = library_dir(instance, library) else {
        return Vec::new();
    };
    let mut index = read_index(&dir);
    let client = client_for(instance);

    if let Some(client) = &client {
        match client.index(tenant, library, index.as_ref().map(|index| index.revision)).await {
            Ok(answer) => {
                let merged = merge(index.take(), answer);
                write_index(&dir, &merged);
                index = Some(merged);
            }
            Err(error) => log::warn!("{}: {error}; using what is cached", client.base()),
        }
    }
    let Some(index) = index else {
        return Vec::new();
    };

    let mut files = Vec::with_capacity(index.entries.len());
    for entry in &index.entries {
        let Some(file_type) = SourceFileType::of(&entry.file_name) else {
            continue;
        };
        let path = dir
            .join("files")
            .join(entry.version.to_string())
            .join(safe_file_name(&entry.file_name));
        // A picture, a PDF or a video of a library that is not kept whole is
        // listed, and fetched when it is first used — see `ensure_local`.
        let lazy = !offline && !entry.kind.is_text();
        if lazy && !path.exists() {
            super::remember_lazy(
                &path.to_string_lossy(),
                super::Lazy {
                    instance,
                    tenant,
                    version: entry.version,
                    sha256: entry.sha256.clone(),
                },
            );
        } else if !path.exists() {
            let Some(client) = &client else { continue };
            if let Err(error) = download(client, tenant, entry.version, &entry.sha256, &path).await {
                log::warn!("{}: {error}", entry.relative_path);
                continue;
            }
        }
        remember_item(
            &path.to_string_lossy(),
            ServerItem {
                instance,
                tenant,
                library,
                lineage: entry.lineage,
                version: entry.version,
            },
        );
        files.push(SourceFile {
            name: display_name(&entry.file_name),
            path,
            file_type,
            md5_hash: Some(entry.md5.clone()),
            relative_path: Some(entry.relative_path.clone()),
        });
    }
    collect_garbage(&dir, &index, offline);
    files
}

/// Fetches a lazily listed file to where its path says.
pub async fn fetch(path: &str, lazy: &super::Lazy) -> Result<(), String> {
    let client = client_for(lazy.instance).ok_or("the server is not known")?;
    let path = Path::new(path);
    if !path.exists() {
        download(&client, lazy.tenant, lazy.version, &lazy.sha256, path).await?;
    }
    touch(path);
    Ok(())
}

/// Marks a fetched file as used now, which is what keeps it from being let go
/// in a library that is not kept whole.
fn touch(path: &Path) {
    if let Ok(file) = std::fs::File::options().append(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// How long ago anything in a version's directory was last used.
fn unused_for(directory: &Path) -> Option<Duration> {
    std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .filter_map(|entry| entry.metadata().and_then(|metadata| metadata.modified()).ok())
        .filter_map(|modified| SystemTime::now().duration_since(modified).ok())
        .min()
}

/// How many files the cached index of a library names, without asking the
/// server — what the settings page shows.
pub fn cached_count(instance: Uuid, library: Uuid) -> Option<usize> {
    let dir = library_dir(instance, library)?;
    read_index(&dir).map(|index| index.entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cantara_api::{IndexEntry, ItemKind};

    fn entry(lineage: u128, version: u128, path: &str) -> IndexEntry {
        IndexEntry {
            lineage: Uuid::from_u128(lineage),
            version: Uuid::from_u128(version),
            relative_path: path.to_string(),
            file_name: path.rsplit('/').next().unwrap_or(path).to_string(),
            title: String::new(),
            kind: ItemKind::Song,
            size: 0,
            sha256: String::new(),
            md5: String::new(),
            revision: 0,
            updated_at: chrono::DateTime::UNIX_EPOCH,
        }
    }

    fn index(revision: i64, complete: bool, entries: Vec<IndexEntry>, removed: Vec<u128>) -> LibraryIndex {
        LibraryIndex {
            library: Uuid::nil(),
            revision,
            complete,
            entries,
            removed: removed.into_iter().map(Uuid::from_u128).collect(),
        }
    }

    #[test]
    fn a_change_list_is_applied_to_what_is_known() {
        let known = index(5, true, vec![entry(1, 10, "a.song"), entry(2, 20, "b.song")], vec![]);
        let answer = index(7, false, vec![entry(2, 21, "b.song"), entry(3, 30, "c.song")], vec![1]);
        let merged = merge(Some(known), answer);
        assert_eq!(merged.revision, 7);
        let versions: Vec<_> = merged.entries.iter().map(|entry| entry.version.as_u128()).collect();
        assert_eq!(versions, vec![21, 30]);
    }

    #[test]
    fn a_complete_answer_replaces_everything() {
        let known = index(5, true, vec![entry(1, 10, "a.song")], vec![]);
        let answer = index(9, true, vec![entry(4, 40, "d.song")], vec![]);
        let merged = merge(Some(known), answer);
        assert_eq!(merged.entries.len(), 1);
        assert_eq!(merged.entries[0].lineage, Uuid::from_u128(4));
    }

    #[test]
    fn file_names_cannot_leave_their_directory() {
        assert_eq!(safe_file_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(safe_file_name(".hidden"), "hidden");
        assert_eq!(safe_file_name("Amazing Grace.song"), "Amazing Grace.song");
    }
}
