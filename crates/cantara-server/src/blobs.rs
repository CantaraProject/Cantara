//! Where file content is kept, addressed by the content's hash.
//!
//! Two places it can be:
//!
//! * **a directory** — `{root}/ab/abcdef…` holds the bytes whose SHA-256 is
//!   `abcdef…`. In the container the root is a volume (`CANTARA_BLOB_DIR`,
//!   `/data/blobs`), so a backup is `pg_dump` plus a copy of that directory;
//! * **an object store** — S3 or anything that speaks its protocol (MinIO,
//!   Hetzner, Wasabi, …), under `{prefix}ab/abcdef…` in a bucket
//!   (`CANTARA_S3_*`). And, for the tests, one held in memory.
//!
//! Either way, two versions with the same content share it, and content
//! arrives the same way: as a stream, written to a temporary file on this
//! machine while it is hashed. Only once it is complete — and its hash known
//! — is it moved to where it is kept, so a 2 GB video never has to fit into
//! memory and a half-received upload never appears under a hash.
//!
//! Content is served with range requests from both, which is what lets a
//! browser seek in a video. From a directory `tower-http` does that; from an
//! object store this module does, asking the store for exactly the bytes
//! asked for.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, IF_NONE_MATCH, RANGE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::TryStreamExt;
use md5::Context as Md5;
use object_store::path::Path as ObjectPath;
use object_store::{GetOptions, GetRange, ObjectStore, ObjectStoreExt, PutPayload, WriteMultipart};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::error::{AppError, AppResult};

/// Content up to this size goes to an object store in one request; larger
/// content in parts, as it is read.
const SINGLE_PUT_LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Clone)]
enum Backend {
    Directory(PathBuf),
    Object { store: Arc<dyn ObjectStore>, prefix: String },
}

#[derive(Clone)]
pub struct BlobStore {
    /// Where content is received before it is kept.
    tmp: PathBuf,
    backend: Backend,
}

impl std::fmt::Debug for BlobStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.describe())
    }
}

/// Content on its way into the store: a temporary file, and what is known
/// about it so far.
pub struct IncomingBlob {
    file: NamedTempFile,
    sha256: Sha256,
    md5: Md5,
    size: u64,
    limit: u64,
}

/// Content that has been received completely.
pub struct ReceivedBlob {
    pub file: NamedTempFile,
    pub sha256: String,
    pub md5: String,
    pub size: u64,
}

fn shard(sha256: &str) -> &str {
    sha256.get(..2).unwrap_or("00")
}

impl BlobStore {
    /// Content kept in a directory.
    pub fn open(root: &Path) -> std::io::Result<BlobStore> {
        std::fs::create_dir_all(root.join("tmp"))?;
        Ok(BlobStore {
            tmp: root.join("tmp"),
            backend: Backend::Directory(root.to_path_buf()),
        })
    }

    /// Content kept in an object store, received through `tmp` on this
    /// machine.
    pub fn object(store: Arc<dyn ObjectStore>, prefix: &str, tmp: &Path) -> std::io::Result<BlobStore> {
        std::fs::create_dir_all(tmp)?;
        let prefix = prefix.trim_matches('/');
        Ok(BlobStore {
            tmp: tmp.to_path_buf(),
            backend: Backend::Object {
                store,
                prefix: if prefix.is_empty() { String::new() } else { format!("{prefix}/") },
            },
        })
    }

    /// For the log.
    pub fn describe(&self) -> String {
        match &self.backend {
            Backend::Directory(root) => format!("the directory {}", root.display()),
            Backend::Object { store, prefix } => format!("{store} under `{prefix}`"),
        }
    }

    fn file_of(root: &Path, sha256: &str) -> PathBuf {
        root.join(shard(sha256)).join(sha256)
    }

    fn object_of(prefix: &str, sha256: &str) -> ObjectPath {
        ObjectPath::from(format!("{prefix}{}/{sha256}", shard(sha256)))
    }

    pub fn incoming(&self, limit: u64) -> AppResult<IncomingBlob> {
        Ok(IncomingBlob {
            file: NamedTempFile::new_in(&self.tmp)?,
            sha256: Sha256::new(),
            md5: Md5::new(),
            size: 0,
            limit,
        })
    }

    /// Content that is already in memory — a song after its `uuid` tag was
    /// written into it.
    pub fn receive_bytes(&self, bytes: &[u8]) -> AppResult<ReceivedBlob> {
        let mut incoming = self.incoming(u64::MAX)?;
        incoming.write(bytes)?;
        incoming.finish()
    }

    /// Moves received content to where it is kept. Content that is there
    /// already is left as it is; the temporary copy is thrown away.
    pub async fn store(&self, blob: ReceivedBlob) -> AppResult<()> {
        match &self.backend {
            Backend::Directory(root) => {
                let target = Self::file_of(root, &blob.sha256);
                tokio::task::spawn_blocking(move || -> AppResult<()> {
                    if target.exists() {
                        return Ok(());
                    }
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    blob.file
                        .persist(&target)
                        .map_err(|error| AppError::internal(error.error))?;
                    Ok(())
                })
                .await
                .map_err(AppError::internal)?
            }
            Backend::Object { store, prefix } => {
                let location = Self::object_of(prefix, &blob.sha256);
                match store.head(&location).await {
                    Ok(_) => return Ok(()),
                    Err(object_store::Error::NotFound { .. }) => {}
                    Err(error) => return Err(AppError::internal(error)),
                }
                if blob.size <= SINGLE_PUT_LIMIT {
                    let bytes = tokio::fs::read(blob.file.path()).await?;
                    store
                        .put(&location, PutPayload::from(bytes))
                        .await
                        .map_err(AppError::internal)?;
                    return Ok(());
                }
                // In parts, as the file is read: a video is never held whole.
                use tokio::io::AsyncReadExt;
                let upload = store.put_multipart(&location).await.map_err(AppError::internal)?;
                let mut writer = WriteMultipart::new(upload);
                let mut file = tokio::fs::File::open(blob.file.path()).await?;
                let mut buffer = vec![0u8; 1024 * 1024];
                loop {
                    let read = file.read(&mut buffer).await?;
                    if read == 0 {
                        break;
                    }
                    writer.wait_for_capacity(4).await.map_err(AppError::internal)?;
                    writer.write(&buffer[..read]);
                }
                writer.finish().await.map_err(AppError::internal)?;
                Ok(())
            }
        }
    }

    pub async fn read(&self, sha256: &str) -> AppResult<Vec<u8>> {
        match &self.backend {
            Backend::Directory(root) => Ok(tokio::fs::read(Self::file_of(root, sha256)).await?),
            Backend::Object { store, prefix } => {
                let result = store.get(&Self::object_of(prefix, sha256)).await.map_err(object_error)?;
                Ok(result.bytes().await.map_err(AppError::internal)?.to_vec())
            }
        }
    }

    pub async fn exists(&self, sha256: &str) -> bool {
        match &self.backend {
            Backend::Directory(root) => tokio::fs::try_exists(Self::file_of(root, sha256)).await.unwrap_or(false),
            Backend::Object { store, prefix } => store.head(&Self::object_of(prefix, sha256)).await.is_ok(),
        }
    }

    /// Removes content. `false` if it was not there.
    pub async fn remove(&self, sha256: &str) -> AppResult<bool> {
        match &self.backend {
            Backend::Directory(root) => match tokio::fs::remove_file(Self::file_of(root, sha256)).await {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error.into()),
            },
            Backend::Object { store, prefix } => {
                let location = Self::object_of(prefix, sha256);
                if store.head(&location).await.is_err() {
                    return Ok(false);
                }
                store.delete(&location).await.map_err(AppError::internal)?;
                Ok(true)
            }
        }
    }

    /// Answers a request for content: whole, or the range asked for.
    pub async fn serve(&self, sha256: &str, mime: &str, request: Request) -> AppResult<Response> {
        match &self.backend {
            Backend::Directory(root) => {
                let mime = mime.parse().unwrap_or(mime_guess::mime::APPLICATION_OCTET_STREAM);
                Ok(ServeFile::new_with_mime(Self::file_of(root, sha256), &mime)
                    .oneshot(request)
                    .await
                    .map_err(AppError::internal)?
                    .map(Body::new)
                    .into_response())
            }
            Backend::Object { store, prefix } => {
                let location = Self::object_of(prefix, sha256);
                let size = store.head(&location).await.map_err(object_error)?.size;

                // Content never changes under its hash, so a client that has
                // it is told so.
                let etag = format!("\"{sha256}\"");
                if request.headers().get(IF_NONE_MATCH).and_then(|value| value.to_str().ok()) == Some(etag.as_str()) {
                    return Ok(StatusCode::NOT_MODIFIED.into_response());
                }

                let range = match request.headers().get(RANGE).and_then(|value| value.to_str().ok()) {
                    None => None,
                    Some(asked) => match parse_range(asked, size) {
                        Some(range) => Some(range),
                        None => {
                            let mut refused = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                            if let Ok(value) = HeaderValue::from_str(&format!("bytes */{size}")) {
                                refused.headers_mut().insert(CONTENT_RANGE, value);
                            }
                            return Ok(refused);
                        }
                    },
                };

                let options = GetOptions {
                    range: range.clone().map(GetRange::Bounded),
                    ..GetOptions::default()
                };
                let result = store.get_opts(&location, options).await.map_err(object_error)?;
                let stream = result.into_stream().map_err(std::io::Error::other);
                let mut response = Response::new(Body::from_stream(stream));
                let headers = response.headers_mut();
                headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
                if let Ok(value) = HeaderValue::from_str(mime) {
                    headers.insert(CONTENT_TYPE, value);
                }
                let length = range.as_ref().map(|range| range.end - range.start).unwrap_or(size);
                headers.insert(CONTENT_LENGTH, HeaderValue::from(length));
                if let Some(range) = range {
                    *response.status_mut() = StatusCode::PARTIAL_CONTENT;
                    if let Ok(value) = HeaderValue::from_str(&format!("bytes {}-{}/{size}", range.start, range.end - 1)) {
                        response.headers_mut().insert(CONTENT_RANGE, value);
                    }
                }
                Ok(response)
            }
        }
    }
}

fn object_error(error: object_store::Error) -> AppError {
    match error {
        object_store::Error::NotFound { .. } => AppError::NotFound,
        other => AppError::internal(other),
    }
}

/// The one range of `bytes=…` that a browser asks for, as a half-open range
/// within `size` bytes. `None` for a range that cannot be satisfied; a
/// header asking for several ranges is answered with the first.
pub fn parse_range(header: &str, size: u64) -> Option<std::ops::Range<u64>> {
    let spec = header.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    let range = match (start.trim(), end.trim()) {
        // The last `n` bytes.
        ("", suffix) => {
            let suffix: u64 = suffix.parse().ok()?;
            if suffix == 0 {
                return None;
            }
            size.saturating_sub(suffix)..size
        }
        (start, "") => start.parse().ok()?..size,
        (start, end) => {
            let start: u64 = start.parse().ok()?;
            let end: u64 = end.parse().ok()?;
            if end < start {
                return None;
            }
            start..(end + 1).min(size)
        }
    };
    (range.start < size && range.start < range.end).then_some(range)
}

impl IncomingBlob {
    pub fn write(&mut self, chunk: &[u8]) -> AppResult<()> {
        self.size += chunk.len() as u64;
        if self.size > self.limit {
            return Err(AppError::TooLarge);
        }
        self.sha256.update(chunk);
        self.md5.consume(chunk);
        self.file.write_all(chunk)?;
        Ok(())
    }

    pub fn finish(mut self) -> AppResult<ReceivedBlob> {
        self.file.flush()?;
        Ok(ReceivedBlob {
            file: self.file,
            sha256: hex::encode(self.sha256.finalize()),
            md5: format!("{:x}", self.md5.finalize()),
            size: self.size,
        })
    }
}

impl ReceivedBlob {
    pub fn read_all(&self) -> AppResult<Vec<u8>> {
        Ok(std::fs::read(self.file.path())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stores(root: &Path) -> Vec<BlobStore> {
        vec![
            BlobStore::open(&root.join("directory")).unwrap(),
            BlobStore::object(Arc::new(object_store::memory::InMemory::new()), "cantara", &root.join("tmp")).unwrap(),
        ]
    }

    #[tokio::test]
    async fn content_lands_under_its_hash_once_and_goes_again() {
        let root = tempfile::tempdir().unwrap();
        for store in stores(root.path()) {
            let first = store.receive_bytes(b"Amazing Grace").unwrap();
            let sha = first.sha256.clone();
            assert_eq!(first.md5, format!("{:x}", md5::compute(b"Amazing Grace")));
            store.store(first).await.unwrap();
            store.store(store.receive_bytes(b"Amazing Grace").unwrap()).await.unwrap();

            assert!(store.exists(&sha).await, "{store:?}");
            assert_eq!(store.read(&sha).await.unwrap(), b"Amazing Grace");
            assert!(store.remove(&sha).await.unwrap());
            assert!(!store.exists(&sha).await);
            assert!(!store.remove(&sha).await.unwrap());
        }
    }

    #[tokio::test]
    async fn large_content_goes_to_an_object_store_in_parts() {
        let root = tempfile::tempdir().unwrap();
        let store = BlobStore::object(Arc::new(object_store::memory::InMemory::new()), "", &root.path().join("tmp")).unwrap();
        let content: Vec<u8> = (0..(SINGLE_PUT_LIMIT as usize + 3 * 1024 * 1024)).map(|i| (i % 251) as u8).collect();
        let received = store.receive_bytes(&content).unwrap();
        let sha = received.sha256.clone();
        store.store(received).await.unwrap();
        assert_eq!(store.read(&sha).await.unwrap(), content);
    }

    #[test]
    fn content_beyond_the_limit_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let store = BlobStore::open(root.path()).unwrap();
        let mut incoming = store.incoming(4).unwrap();
        assert!(incoming.write(b"1234").is_ok());
        assert!(matches!(incoming.write(b"5"), Err(AppError::TooLarge)));
    }

    #[test]
    fn ranges_are_read_as_browsers_send_them() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some(0..100));
        assert_eq!(parse_range("bytes=900-", 1000), Some(900..1000));
        assert_eq!(parse_range("bytes=-100", 1000), Some(900..1000));
        assert_eq!(parse_range("bytes=990-2000", 1000), Some(990..1000));
        assert_eq!(parse_range("bytes=0-0, 5-9", 1000), Some(0..1));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("bytes=5-3", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }
}
