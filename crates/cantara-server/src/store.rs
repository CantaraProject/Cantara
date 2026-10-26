//! Storing a file in a library: which item it belongs to, whether it is a new
//! version of it, and what is written.
//!
//! The rules, from Q6 and Q7 of `docs/specs/0008-server-version.md`:
//!
//! 1. A song's `uuid` tag names its item. A song without one is given one,
//!    written into the file.
//! 2. Otherwise the same path in the same library is the same item.
//! 3. The same content under another path is reported as a likely duplicate,
//!    not merged.
//! 4. A save that says which version it started from is refused if the item
//!    has moved on since (`version_conflict`).
//! 5. Content that is what the item has already is not stored again.

use std::collections::BTreeMap;

use cantara_api::{ItemInfo, ItemKind, StoreOutcome, StoredItem, UserRef, VersionInfo};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::blobs::ReceivedBlob;
use crate::error::{AppError, AppResult};
use crate::songs;
use crate::state::AppState;

/// What is being stored.
pub struct StoreRequest {
    pub tenant: Uuid,
    pub library: Uuid,
    pub relative_path: String,
    pub content: ReceivedBlob,
    /// The version the client's edit started from.
    pub base_version: Option<Uuid>,
    /// The item this is meant to be a version of — `PUT …/items/{lineage}`.
    pub lineage: Option<Uuid>,
    pub user: Option<Uuid>,
}

/// A path as the library keeps it: `/`-separated, no leading slash, nothing
/// that climbs out.
pub fn normalise_path(path: &str) -> AppResult<String> {
    let parts: Vec<&str> = path
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() || parts.iter().any(|part| *part == ".." || part.starts_with('.')) {
        return Err(AppError::BadRequest(format!("`{path}` cannot be a path in a library")));
    }
    Ok(parts.join("/"))
}

fn file_name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

struct ExistingItem {
    uuid: Uuid,
    lineage: Uuid,
    current_version: Option<Uuid>,
    current_sha: Option<String>,
    relative_path: String,
    deleted: bool,
}

type ItemRow = (Uuid, Uuid, Option<Uuid>, Option<String>, String, bool);

const ITEM_QUERY: &str = "SELECT i.uuid, i.lineage_uuid, i.current_version, v.sha256, i.relative_path, i.deleted_at IS NOT NULL
       FROM items i LEFT JOIN item_versions v ON v.uuid = i.current_version
      WHERE i.library_uuid = $1";

fn existing_item(row: ItemRow) -> ExistingItem {
    let (uuid, lineage, current_version, current_sha, relative_path, deleted) = row;
    ExistingItem {
        uuid,
        lineage,
        current_version,
        current_sha,
        relative_path,
        deleted,
    }
}

async fn by_lineage(state: &AppState, library: Uuid, lineage: Uuid) -> AppResult<Option<ExistingItem>> {
    let row: Option<ItemRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!("{ITEM_QUERY} AND i.lineage_uuid = $2")))
        .bind(library)
        .bind(lineage)
        .fetch_optional(&state.db)
        .await?;
    Ok(row.map(existing_item))
}

async fn by_live_path(state: &AppState, library: Uuid, path: &str) -> AppResult<Option<ExistingItem>> {
    let row: Option<ItemRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!("{ITEM_QUERY} AND i.relative_path = $2 AND i.deleted_at IS NULL")))
        .bind(library)
        .bind(path)
        .fetch_optional(&state.db)
        .await?;
    Ok(row.map(existing_item))
}

pub async fn store(state: &AppState, request: StoreRequest) -> AppResult<StoredItem> {
    let relative_path = normalise_path(&request.relative_path)?;
    let file_name = file_name_of(&relative_path);
    let kind = ItemKind::of(&file_name);
    if kind == ItemKind::Other {
        return Err(AppError::Unsupported(format!("Cantara does not read `{file_name}`")));
    }

    let mut content = request.content;
    let mut text = if kind.is_text() { Some(content.read_all()?) } else { None };
    let mut meta = match &text {
        Some(bytes) => songs::metadata(&file_name, kind, bytes),
        None => songs::Metadata {
            title: songs::display_name(&file_name),
            ..songs::Metadata::default()
        },
    };

    // Which item is this?
    let existing = if let Some(lineage) = request.lineage {
        let item = by_lineage(state, request.library, lineage).await?.ok_or(AppError::NotFound)?;
        if meta.uuid.is_some_and(|uuid| uuid != lineage) {
            return Err(AppError::BadRequest(
                "the song carries the uuid of another item".to_string(),
            ));
        }
        Some(item)
    } else if let Some(uuid) = meta.uuid {
        by_lineage(state, request.library, uuid).await?
    } else {
        by_live_path(state, request.library, &relative_path).await?
    };

    // The path may not belong to another living item.
    if let Some(holder) = by_live_path(state, request.library, &relative_path).await?
        && existing.as_ref().is_none_or(|item| item.uuid != holder.uuid) {
            return Err(AppError::Conflict(format!("`{relative_path}` is another item already")));
        }

    if let (Some(base), Some(item)) = (request.base_version, &existing)
        && !item.deleted && item.current_version != Some(base) {
            return Err(AppError::VersionConflict {
                current: item.current_version,
            });
        }

    let lineage = existing
        .as_ref()
        .map(|item| item.lineage)
        .or(meta.uuid)
        .unwrap_or_else(Uuid::now_v7);

    // A song that does not carry its identity is given it.
    if kind == ItemKind::Song && meta.uuid.is_none()
        && let Some(original) = text.as_deref().and_then(|bytes| std::str::from_utf8(bytes).ok())
            && let Some(tagged) = songs::with_uuid_tag(&file_name, original, lineage) {
                content = state.blobs.receive_bytes(tagged.as_bytes())?;
                meta = songs::metadata(&file_name, kind, tagged.as_bytes());
                text = Some(tagged.into_bytes());
            }
    drop(text);

    if let Some(item) = &existing
        && !item.deleted && item.current_sha.as_deref() == Some(content.sha256.as_str()) && item.relative_path == relative_path {
            return Ok(StoredItem {
                outcome: StoreOutcome::Unchanged,
                item: item_info(state, request.tenant, request.library, item.lineage).await?,
                possible_duplicate_of: None,
            });
        }

    let duplicate: Option<Uuid> = sqlx::query_scalar(
        "SELECT i.lineage_uuid FROM items i JOIN item_versions v ON v.uuid = i.current_version
          WHERE i.library_uuid = $1 AND i.deleted_at IS NULL AND v.sha256 = $2 AND i.lineage_uuid <> $3
          LIMIT 1",
    )
    .bind(request.library)
    .bind(&content.sha256)
    .bind(lineage)
    .fetch_optional(&state.db)
    .await?;

    let sha256 = content.sha256.clone();
    let md5 = content.md5.clone();
    let size = content.size as i64;
    state.blobs.store(content).await?;

    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO blobs (sha256, size) VALUES ($1, $2) ON CONFLICT (sha256) DO UPDATE SET created_at = now()")
        .bind(&sha256)
        .bind(size)
        .execute(&mut *tx)
        .await?;
    let revision: i64 = sqlx::query_scalar(
        "UPDATE libraries SET revision = revision + 1 WHERE uuid = $1 AND tenant_uuid = $2 RETURNING revision",
    )
    .bind(request.library)
    .bind(request.tenant)
    .fetch_one(&mut *tx)
    .await?;

    let version = Uuid::now_v7();
    let item_uuid = match &existing {
        Some(item) => {
            sqlx::query(
                "UPDATE items SET current_version = $2, relative_path = $3, revision = $4, deleted_at = NULL
                  WHERE uuid = $1",
            )
            .bind(item.uuid)
            .bind(version)
            .bind(&relative_path)
            .bind(revision)
            .execute(&mut *tx)
            .await?;
            item.uuid
        }
        None => {
            let uuid = Uuid::now_v7();
            sqlx::query(
                "INSERT INTO items (uuid, tenant_uuid, library_uuid, lineage_uuid, relative_path, current_version, revision)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(uuid)
            .bind(request.tenant)
            .bind(request.library)
            .bind(lineage)
            .bind(&relative_path)
            .bind(version)
            .bind(revision)
            .execute(&mut *tx)
            .await?;
            uuid
        }
    };
    sqlx::query(
        "INSERT INTO item_versions (uuid, item_uuid, relative_path, file_name, kind, sha256, md5, size, title, tags, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(version)
    .bind(item_uuid)
    .bind(&relative_path)
    .bind(&file_name)
    .bind(kind.as_str())
    .bind(&sha256)
    .bind(&md5)
    .bind(size)
    .bind(&meta.title)
    .bind(serde_json::to_value(&meta.tags).map_err(AppError::internal)?)
    .bind(request.user)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(StoredItem {
        outcome: if existing.is_some() { StoreOutcome::NewVersion } else { StoreOutcome::Created },
        item: item_info(state, request.tenant, request.library, lineage).await?,
        possible_duplicate_of: duplicate,
    })
}

/// Removes an item from the library. Its versions stay, so that it can be
/// brought back and so that a client that asks what changed is told it went.
pub async fn delete(state: &AppState, tenant: Uuid, library: Uuid, lineage: Uuid) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    let revision: i64 = sqlx::query_scalar(
        "UPDATE libraries SET revision = revision + 1 WHERE uuid = $1 AND tenant_uuid = $2 RETURNING revision",
    )
    .bind(library)
    .bind(tenant)
    .fetch_one(&mut *tx)
    .await?;
    let deleted = sqlx::query(
        "UPDATE items SET deleted_at = now(), revision = $3
          WHERE library_uuid = $1 AND lineage_uuid = $2 AND deleted_at IS NULL",
    )
    .bind(library)
    .bind(lineage)
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

type VersionRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
    serde_json::Value,
    Option<Uuid>,
    Option<String>,
    DateTime<Utc>,
);

const VERSION_COLUMNS: &str = "v.uuid, i.lineage_uuid, v.relative_path, v.file_name, v.kind, v.size, v.sha256, v.md5,
     v.title, v.tags, u.uuid, u.display_name, v.created_at";

fn version_info(row: VersionRow) -> VersionInfo {
    let (uuid, lineage, relative_path, file_name, kind, size, sha256, md5, title, tags, by, by_name, created_at) = row;
    VersionInfo {
        uuid,
        lineage,
        relative_path,
        file_name,
        kind: ItemKind::parse(&kind),
        size,
        sha256,
        md5,
        title,
        tags: serde_json::from_value::<BTreeMap<String, String>>(tags).unwrap_or_default(),
        created_by: by.zip(by_name).map(|(uuid, display_name)| UserRef { uuid, display_name }),
        created_at,
    }
}

pub async fn item_info(state: &AppState, tenant: Uuid, library: Uuid, lineage: Uuid) -> AppResult<ItemInfo> {
    let row: (VersionRow, bool) = {
        let sql = format!(
            "SELECT {VERSION_COLUMNS}, i.deleted_at IS NOT NULL
               FROM items i JOIN item_versions v ON v.uuid = i.current_version
               LEFT JOIN users u ON u.uuid = v.created_by
              WHERE i.tenant_uuid = $1 AND i.library_uuid = $2 AND i.lineage_uuid = $3"
        );
        let (a, b, c, d, e, f, g, h, i, j, k, l, m, deleted): (
            Uuid, Uuid, String, String, String, i64, String, String, String, serde_json::Value, Option<Uuid>, Option<String>, DateTime<Utc>, bool,
        ) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(tenant)
            .bind(library)
            .bind(lineage)
            .fetch_one(&state.db)
            .await?;
        ((a, b, c, d, e, f, g, h, i, j, k, l, m), deleted)
    };
    Ok(ItemInfo {
        lineage,
        library,
        current: version_info(row.0),
        deleted: row.1,
    })
}

pub async fn versions(state: &AppState, tenant: Uuid, library: Uuid, lineage: Uuid) -> AppResult<Vec<VersionInfo>> {
    let sql = format!(
        "SELECT {VERSION_COLUMNS}
           FROM items i JOIN item_versions v ON v.item_uuid = i.uuid
           LEFT JOIN users u ON u.uuid = v.created_by
          WHERE i.tenant_uuid = $1 AND i.library_uuid = $2 AND i.lineage_uuid = $3
          ORDER BY v.created_at DESC, v.uuid DESC"
    );
    let rows: Vec<VersionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant)
        .bind(library)
        .bind(lineage)
        .fetch_all(&state.db)
        .await?;
    if rows.is_empty() {
        return Err(AppError::NotFound);
    }
    Ok(rows.into_iter().map(version_info).collect())
}

/// A version, found by its uuid alone, if it is in the tenant: what the
/// content endpoint needs.
pub struct VersionLocation {
    pub library: Uuid,
    pub sha256: String,
    pub file_name: String,
    pub relative_path: String,
}

pub async fn locate_version(state: &AppState, tenant: Uuid, version: Uuid) -> AppResult<VersionLocation> {
    let (library, sha256, file_name, relative_path): (Uuid, String, String, String) = sqlx::query_as(
        "SELECT i.library_uuid, v.sha256, v.file_name, v.relative_path
           FROM item_versions v JOIN items i ON i.uuid = v.item_uuid
          WHERE v.uuid = $1 AND i.tenant_uuid = $2",
    )
    .bind(version)
    .bind(tenant)
    .fetch_one(&state.db)
    .await?;
    Ok(VersionLocation {
        library,
        sha256,
        file_name,
        relative_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalised_and_cannot_climb_out() {
        assert_eq!(normalise_path("/Lieder//Amazing Grace.song").unwrap(), "Lieder/Amazing Grace.song");
        assert_eq!(normalise_path("Lieder\\Neu\\Lied.song").unwrap(), "Lieder/Neu/Lied.song");
        assert!(normalise_path("../etc/passwd").is_err());
        assert!(normalise_path("Lieder/.hidden.song").is_err());
        assert!(normalise_path("").is_err());
    }
}
