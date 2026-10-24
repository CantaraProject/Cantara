//! Where file content is kept: a directory, addressed by the content's hash.
//!
//! `{root}/ab/abcdef…` holds the bytes whose SHA-256 is `abcdef…`. Two versions
//! with the same content — a song whose title changed in the database only, a
//! picture uploaded into two libraries — share one file. In the container the
//! root is a volume (`CANTARA_BLOB_DIR`, `/data/blobs`), so a backup of the
//! server is `pg_dump` plus a copy of that directory.
//!
//! Content arrives as a stream and is written to a temporary file in the same
//! directory while it is hashed, then renamed into place: a 2 GB video never
//! has to fit into memory, and a half-written upload never appears under a
//! hash.

use std::io::Write;
use std::path::{Path, PathBuf};

use md5::Context as Md5;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct BlobStore {
    root: PathBuf,
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

impl BlobStore {
    pub fn open(root: &Path) -> std::io::Result<BlobStore> {
        std::fs::create_dir_all(root.join("tmp"))?;
        Ok(BlobStore {
            root: root.to_path_buf(),
        })
    }

    pub fn path_of(&self, sha256: &str) -> PathBuf {
        let prefix = sha256.get(..2).unwrap_or("00");
        self.root.join(prefix).join(sha256)
    }

    pub fn incoming(&self, limit: u64) -> AppResult<IncomingBlob> {
        Ok(IncomingBlob {
            file: NamedTempFile::new_in(self.root.join("tmp"))?,
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

    /// Moves received content into its place. Content that is there already
    /// is left as it is; the temporary copy is thrown away.
    pub fn store(&self, blob: ReceivedBlob) -> AppResult<()> {
        let target = self.path_of(&blob.sha256);
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
    }

    pub async fn read(&self, sha256: &str) -> AppResult<Vec<u8>> {
        Ok(tokio::fs::read(self.path_of(sha256)).await?)
    }
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

    #[test]
    fn content_lands_under_its_hash_once() {
        let root = tempfile::tempdir().unwrap();
        let store = BlobStore::open(root.path()).unwrap();

        let first = store.receive_bytes(b"Amazing Grace").unwrap();
        let sha = first.sha256.clone();
        assert_eq!(first.md5, format!("{:x}", md5::compute(b"Amazing Grace")));
        store.store(first).unwrap();
        store.store(store.receive_bytes(b"Amazing Grace").unwrap()).unwrap();

        assert_eq!(std::fs::read(store.path_of(&sha)).unwrap(), b"Amazing Grace");
        assert_eq!(std::fs::read_dir(root.path().join("tmp")).unwrap().count(), 0);
    }

    #[test]
    fn content_beyond_the_limit_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let store = BlobStore::open(root.path()).unwrap();
        let mut incoming = store.incoming(4).unwrap();
        assert!(incoming.write(b"1234").is_ok());
        assert!(matches!(incoming.write(b"5"), Err(AppError::TooLarge)));
    }
}
