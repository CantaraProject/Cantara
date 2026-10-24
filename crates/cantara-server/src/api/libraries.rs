//! Libraries, their index, their items, and the content of every version.

use axum::extract::{Multipart, Path, Query, Request, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, ETAG};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use cantara_api::{
    ImportReport, IndexEntry, ItemInfo, ItemKind, LibraryIndex, LibraryInfo, ResourceKind, Role,
    SaveLibrary, StoreOutcome, StoredItem, VersionInfo,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::grants::{require, resource_role};
use crate::auth::{member, CurrentUser, Member};
use crate::blobs::ReceivedBlob;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::{self, StoreRequest};

async fn library_info(state: &AppState, member: &Member, library: Uuid, role: Role) -> AppResult<LibraryInfo> {
    let (name, description, revision, item_count, size_bytes): (String, String, i64, i64, i64) = sqlx::query_as(
        "SELECT l.name, l.description, l.revision,
                (SELECT count(*) FROM items i WHERE i.library_uuid = l.uuid AND i.deleted_at IS NULL),
                COALESCE((SELECT sum(v.size) FROM items i JOIN item_versions v ON v.uuid = i.current_version
                           WHERE i.library_uuid = l.uuid AND i.deleted_at IS NULL), 0)::bigint
           FROM libraries l WHERE l.uuid = $1 AND l.tenant_uuid = $2",
    )
    .bind(library)
    .bind(member.tenant)
    .fetch_one(&state.db)
    .await?;
    Ok(LibraryInfo {
        uuid: library,
        name,
        description,
        revision,
        item_count,
        size_bytes,
        role,
    })
}

/// The libraries the user may read.
#[utoipa::path(get, path = "/tenants/{tenant}/libraries", tag = "libraries",
    params(("tenant" = Uuid, Path)), responses((status = 200, body = [LibraryInfo])))]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
) -> AppResult<Json<Vec<LibraryInfo>>> {
    let member = member(&state, &user, tenant).await?;
    let all: Vec<Uuid> = sqlx::query_scalar("SELECT uuid FROM libraries WHERE tenant_uuid = $1 ORDER BY name")
        .bind(tenant)
        .fetch_all(&state.db)
        .await?;
    let mut libraries = Vec::new();
    for library in all {
        if let Some(role) = resource_role(&state, &member, ResourceKind::Library, library).await? {
            libraries.push(library_info(&state, &member, library, role).await?);
        }
    }
    Ok(Json(libraries))
}

/// Creates a library. Only admins may; who else may use it is said with
/// grants afterwards.
#[utoipa::path(post, path = "/tenants/{tenant}/libraries", tag = "libraries", request_body = SaveLibrary,
    params(("tenant" = Uuid, Path)), responses((status = 200, body = LibraryInfo)))]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
    Json(request): Json<SaveLibrary>,
) -> AppResult<Json<LibraryInfo>> {
    let member = member(&state, &user, tenant).await?;
    member.require_admin()?;
    if request.name.trim().is_empty() {
        return Err(AppError::BadRequest("a library needs a name".to_string()));
    }
    let library = Uuid::now_v7();
    sqlx::query("INSERT INTO libraries (uuid, tenant_uuid, name, description) VALUES ($1, $2, $3, $4)")
        .bind(library)
        .bind(tenant)
        .bind(request.name.trim())
        .bind(request.description.trim())
        .execute(&state.db)
        .await?;
    Ok(Json(library_info(&state, &member, library, Role::Manage).await?))
}

/// One library.
#[utoipa::path(get, path = "/tenants/{tenant}/libraries/{library}", tag = "libraries",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path)), responses((status = 200, body = LibraryInfo)))]
pub async fn get(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<LibraryInfo>> {
    let member = member(&state, &user, tenant).await?;
    let role = require(&state, &member, ResourceKind::Library, library, Role::Read).await?;
    Ok(Json(library_info(&state, &member, library, role).await?))
}

/// Renames a library or changes its description.
#[utoipa::path(patch, path = "/tenants/{tenant}/libraries/{library}", tag = "libraries", request_body = SaveLibrary,
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path)), responses((status = 200, body = LibraryInfo)))]
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library)): Path<(Uuid, Uuid)>,
    Json(request): Json<SaveLibrary>,
) -> AppResult<Json<LibraryInfo>> {
    let member = member(&state, &user, tenant).await?;
    let role = require(&state, &member, ResourceKind::Library, library, Role::Manage).await?;
    if request.name.trim().is_empty() {
        return Err(AppError::BadRequest("a library needs a name".to_string()));
    }
    sqlx::query("UPDATE libraries SET name = $3, description = $4 WHERE uuid = $1 AND tenant_uuid = $2")
        .bind(library)
        .bind(tenant)
        .bind(request.name.trim())
        .bind(request.description.trim())
        .execute(&state.db)
        .await?;
    Ok(Json(library_info(&state, &member, library, role).await?))
}

/// Deletes a library with everything in it.
#[utoipa::path(delete, path = "/tenants/{tenant}/libraries/{library}", tag = "libraries",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path)), responses((status = 204)))]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let member = member(&state, &user, tenant).await?;
    member.require_admin()?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM grants WHERE tenant_uuid = $1 AND resource_kind = 'library' AND resource_uuid = $2")
        .bind(tenant)
        .bind(library)
        .execute(&mut *tx)
        .await?;
    let deleted = sqlx::query("DELETE FROM libraries WHERE uuid = $1 AND tenant_uuid = $2")
        .bind(library)
        .bind(tenant)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct IndexQuery {
    /// The revision the client last saw. Without it, the whole library.
    pub since: Option<i64>,
}

type IndexRow = (Uuid, Uuid, String, String, String, String, i64, String, String, i64, DateTime<Utc>);

/// What is in a library — all of it, or what changed since a revision. This
/// is the whole of a client's sync.
#[utoipa::path(get, path = "/tenants/{tenant}/libraries/{library}/index", tag = "libraries",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path), IndexQuery),
    responses((status = 200, body = LibraryIndex)))]
pub async fn index(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library)): Path<(Uuid, Uuid)>,
    Query(query): Query<IndexQuery>,
) -> AppResult<Json<LibraryIndex>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Library, library, Role::Read).await?;

    // Read in one transaction, so that the revision answered with is the one
    // the entries are as of.
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM libraries WHERE uuid = $1")
        .bind(library)
        .fetch_one(&mut *tx)
        .await?;
    let since = query.since.filter(|since| *since <= revision);
    let rows: Vec<IndexRow> = sqlx::query_as(
        "SELECT i.lineage_uuid, v.uuid, v.relative_path, v.file_name, v.title, v.kind, v.size, v.sha256, v.md5,
                i.revision, v.created_at
           FROM items i JOIN item_versions v ON v.uuid = i.current_version
          WHERE i.library_uuid = $1 AND i.deleted_at IS NULL AND i.revision > $2
          ORDER BY v.relative_path",
    )
    .bind(library)
    .bind(since.unwrap_or(-1))
    .fetch_all(&mut *tx)
    .await?;
    let removed: Vec<Uuid> = match since {
        Some(since) => {
            sqlx::query_scalar(
                "SELECT lineage_uuid FROM items WHERE library_uuid = $1 AND deleted_at IS NOT NULL AND revision > $2",
            )
            .bind(library)
            .bind(since)
            .fetch_all(&mut *tx)
            .await?
        }
        None => Vec::new(),
    };
    tx.commit().await?;

    Ok(Json(LibraryIndex {
        library,
        revision,
        complete: since.is_none(),
        entries: rows
            .into_iter()
            .map(
                |(lineage, version, relative_path, file_name, title, kind, size, sha256, md5, revision, updated_at)| IndexEntry {
                    lineage,
                    version,
                    relative_path,
                    file_name,
                    title,
                    kind: ItemKind::parse(&kind),
                    size,
                    sha256,
                    md5,
                    revision,
                    updated_at,
                },
            )
            .collect(),
        removed,
    }))
}

/// What a multipart upload carried.
struct Upload {
    file: Option<(String, ReceivedBlob)>,
    relative_path: Option<String>,
    base_version: Option<Uuid>,
}

async fn read_upload(state: &AppState, mut multipart: Multipart) -> AppResult<Upload> {
    let mut upload = Upload {
        file: None,
        relative_path: None,
        base_version: None,
    };
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::BadRequest(error.to_string()))?
    {
        match field.name() {
            Some("file") => {
                let name = field.file_name().unwrap_or_default().to_string();
                let mut incoming = state.blobs.incoming(state.config.max_upload_bytes)?;
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|error| AppError::BadRequest(error.to_string()))?
                {
                    incoming.write(&chunk)?;
                }
                upload.file = Some((name, incoming.finish()?));
            }
            Some("relative_path") => {
                upload.relative_path = Some(field.text().await.map_err(|error| AppError::BadRequest(error.to_string()))?);
            }
            Some("base_version") => {
                let text = field.text().await.map_err(|error| AppError::BadRequest(error.to_string()))?;
                if !text.trim().is_empty() {
                    upload.base_version = Some(
                        Uuid::parse_str(text.trim()).map_err(|_| AppError::BadRequest("base_version is no uuid".to_string()))?,
                    );
                }
            }
            _ => {}
        }
    }
    Ok(upload)
}

/// Stores a file: a new item, or a new version of the item it turns out to
/// be. A multipart form with `file`, and optionally `relative_path` (default:
/// the file's name) and `base_version`.
#[utoipa::path(post, path = "/tenants/{tenant}/libraries/{library}/items", tag = "items",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path)),
    request_body(content_type = "multipart/form-data", content = Object),
    responses((status = 200, body = StoredItem), (status = 409, body = cantara_api::ApiError)))]
pub async fn upload(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library)): Path<(Uuid, Uuid)>,
    multipart: Multipart,
) -> AppResult<Json<StoredItem>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Library, library, Role::Write).await?;
    let upload = read_upload(&state, multipart).await?;
    let (name, content) = upload.file.ok_or_else(|| AppError::BadRequest("no file was sent".to_string()))?;
    Ok(Json(
        store::store(
            &state,
            StoreRequest {
                tenant,
                library,
                relative_path: upload.relative_path.unwrap_or(name),
                content,
                base_version: upload.base_version,
                lineage: None,
                user: Some(user.uuid),
            },
        )
        .await?,
    ))
}

/// A new version of a particular item. Like the upload, but the item is
/// named; `relative_path` moves it.
#[utoipa::path(put, path = "/tenants/{tenant}/libraries/{library}/items/{lineage}", tag = "items",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path), ("lineage" = Uuid, Path)),
    request_body(content_type = "multipart/form-data", content = Object),
    responses((status = 200, body = StoredItem), (status = 409, body = cantara_api::ApiError)))]
pub async fn replace(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library, lineage)): Path<(Uuid, Uuid, Uuid)>,
    multipart: Multipart,
) -> AppResult<Json<StoredItem>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Library, library, Role::Write).await?;
    let current = store::item_info(&state, tenant, library, lineage).await?;
    let upload = read_upload(&state, multipart).await?;
    let (_, content) = upload.file.ok_or_else(|| AppError::BadRequest("no file was sent".to_string()))?;
    Ok(Json(
        store::store(
            &state,
            StoreRequest {
                tenant,
                library,
                relative_path: upload.relative_path.unwrap_or(current.current.relative_path),
                content,
                base_version: upload.base_version,
                lineage: Some(lineage),
                user: Some(user.uuid),
            },
        )
        .await?,
    ))
}

/// One item, as it is now.
#[utoipa::path(get, path = "/tenants/{tenant}/libraries/{library}/items/{lineage}", tag = "items",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path), ("lineage" = Uuid, Path)),
    responses((status = 200, body = ItemInfo)))]
pub async fn item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library, lineage)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<Json<ItemInfo>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Library, library, Role::Read).await?;
    Ok(Json(store::item_info(&state, tenant, library, lineage).await?))
}

/// Every version of an item, newest first.
#[utoipa::path(get, path = "/tenants/{tenant}/libraries/{library}/items/{lineage}/versions", tag = "items",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path), ("lineage" = Uuid, Path)),
    responses((status = 200, body = [VersionInfo])))]
pub async fn versions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library, lineage)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<Json<Vec<VersionInfo>>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Library, library, Role::Read).await?;
    Ok(Json(store::versions(&state, tenant, library, lineage).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Restore {
    pub version: Uuid,
}

/// Makes an earlier version the current one again — as a new version, so
/// that nothing in the history is lost. Also brings back a deleted item.
#[utoipa::path(post, path = "/tenants/{tenant}/libraries/{library}/items/{lineage}/restore", tag = "items",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path), ("lineage" = Uuid, Path)), request_body = Restore,
    responses((status = 200, body = StoredItem)))]
pub async fn restore(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library, lineage)): Path<(Uuid, Uuid, Uuid)>,
    Json(request): Json<Restore>,
) -> AppResult<Json<StoredItem>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Library, library, Role::Write).await?;
    let old = store::versions(&state, tenant, library, lineage)
        .await?
        .into_iter()
        .find(|version| version.uuid == request.version)
        .ok_or(AppError::NotFound)?;
    let bytes = state.blobs.read(&old.sha256).await?;
    let content = state.blobs.receive_bytes(&bytes)?;
    let mut stored = store::store(
        &state,
        StoreRequest {
            tenant,
            library,
            relative_path: old.relative_path,
            content,
            base_version: None,
            lineage: Some(lineage),
            user: Some(user.uuid),
        },
    )
    .await?;
    if stored.outcome == StoreOutcome::Unchanged {
        stored.outcome = StoreOutcome::NewVersion;
    }
    Ok(Json(stored))
}

/// Removes an item. Its history stays, and it can be restored.
#[utoipa::path(delete, path = "/tenants/{tenant}/libraries/{library}/items/{lineage}", tag = "items",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path), ("lineage" = Uuid, Path)), responses((status = 204)))]
pub async fn delete_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library, lineage)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Library, library, Role::Write).await?;
    store::delete(&state, tenant, library, lineage).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The bytes of one version. Never changes, and says so to every cache on
/// the way; answers range requests, which is what lets a browser seek in a
/// video.
#[utoipa::path(get, path = "/tenants/{tenant}/versions/{version}/content", tag = "items",
    params(("tenant" = Uuid, Path), ("version" = Uuid, Path)),
    responses((status = 200, content_type = "application/octet-stream")))]
pub async fn content(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, version)): Path<(Uuid, Uuid)>,
    request: Request,
) -> AppResult<Response> {
    let member = member(&state, &user, tenant).await?;
    let location = store::locate_version(&state, tenant, version).await?;
    require(&state, &member, ResourceKind::Library, location.library, Role::Read).await?;
    serve_blob(&state, &location.sha256, &location.file_name, request).await
}

pub async fn serve_blob(state: &AppState, sha256: &str, file_name: &str, request: Request) -> AppResult<Response> {
    let mime = mime_guess::from_path(file_name).first_or_octet_stream();
    let mut response = state.blobs.serve(sha256, mime.as_ref(), request).await?;
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"));
    if let Ok(etag) = HeaderValue::from_str(&format!("\"{sha256}\"")) {
        headers.insert(ETAG, etag);
    }
    let encoded: String = url::form_urlencoded::byte_serialize(file_name.as_bytes()).collect();
    if let Ok(disposition) = HeaderValue::from_str(&format!("inline; filename*=UTF-8''{encoded}")) {
        headers.insert(CONTENT_DISPOSITION, disposition);
    }
    Ok(response.into_response())
}

/// Imports a whole repository from a ZIP, the way `RepositoryType::RemoteZip`
/// reads one: every file Cantara reads is stored, at its path inside the
/// archive. An archive whose files all sit in one top folder (a GitHub
/// download) is read as if that folder were the root.
#[utoipa::path(post, path = "/tenants/{tenant}/libraries/{library}/import", tag = "items",
    params(("tenant" = Uuid, Path), ("library" = Uuid, Path)),
    request_body(content_type = "multipart/form-data", content = Object),
    responses((status = 200, body = ImportReport)))]
pub async fn import(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, library)): Path<(Uuid, Uuid)>,
    multipart: Multipart,
) -> AppResult<Json<ImportReport>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Library, library, Role::Write).await?;
    let upload = read_upload(&state, multipart).await?;
    let (_, archive) = upload.file.ok_or_else(|| AppError::BadRequest("no archive was sent".to_string()))?;

    let blobs = state.blobs.clone();
    let limit = state.config.max_upload_bytes;
    let (files, skipped) = tokio::task::spawn_blocking(move || unpack(&blobs, &archive, limit))
        .await
        .map_err(AppError::internal)??;

    let mut report = ImportReport {
        skipped,
        ..ImportReport::default()
    };
    for (path, content) in files {
        let stored = store::store(
            &state,
            StoreRequest {
                tenant,
                library,
                relative_path: path.clone(),
                content,
                base_version: None,
                lineage: None,
                user: Some(user.uuid),
            },
        )
        .await;
        match stored {
            Ok(stored) => {
                match stored.outcome {
                    StoreOutcome::Created => report.created += 1,
                    StoreOutcome::NewVersion => report.new_versions += 1,
                    StoreOutcome::Unchanged => report.unchanged += 1,
                }
                if stored.possible_duplicate_of.is_some() {
                    report.possible_duplicates.push((path, stored.item.current.relative_path));
                }
            }
            Err(AppError::Conflict(reason) | AppError::BadRequest(reason) | AppError::Unsupported(reason)) => {
                report.skipped.push(format!("{path}: {reason}"));
            }
            Err(other) => return Err(other),
        }
    }
    Ok(Json(report))
}

/// The files of an archive Cantara reads, each received into the blob store's
/// temporary space, and the names of those it does not.
fn unpack(
    blobs: &crate::blobs::BlobStore,
    archive: &ReceivedBlob,
    limit: u64,
) -> AppResult<(Vec<(String, ReceivedBlob)>, Vec<String>)> {
    let file = std::fs::File::open(archive.file.path())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| AppError::BadRequest(format!("not a ZIP archive: {error}")))?;

    let mut names: Vec<String> = Vec::new();
    for index in 0..zip.len() {
        let Ok(entry) = zip.by_index(index) else { continue };
        if entry.is_file()
            && let Some(path) = entry.enclosed_name() {
                names.push(path.to_string_lossy().replace('\\', "/"));
            }
    }
    let root = common_root(&names);

    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| AppError::BadRequest(error.to_string()))?;
        if !entry.is_file() {
            continue;
        }
        let Some(name) = entry.enclosed_name().map(|path| path.to_string_lossy().replace('\\', "/")) else {
            continue;
        };
        let relative = name.strip_prefix(&root).unwrap_or(&name).to_string();
        let hidden = relative.split('/').any(|part| part.starts_with('.') || part == "__MACOSX");
        if hidden {
            continue;
        }
        if ItemKind::of(&relative) == ItemKind::Other {
            skipped.push(relative);
            continue;
        }
        let mut incoming = blobs.incoming(limit)?;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = std::io::Read::read(&mut entry, &mut buffer)?;
            if read == 0 {
                break;
            }
            incoming.write(&buffer[..read])?;
        }
        files.push((relative, incoming.finish()?));
    }
    Ok((files, skipped))
}

/// `Repo-main/` when every name starts with it; empty otherwise.
fn common_root(names: &[String]) -> String {
    let Some(first) = names.first() else {
        return String::new();
    };
    let Some((top, _)) = first.split_once('/') else {
        return String::new();
    };
    let prefix = format!("{top}/");
    if names.iter().all(|name| name.starts_with(&prefix)) {
        prefix
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::common_root;

    #[test]
    fn a_single_top_folder_is_the_root() {
        let names = vec!["Songs-main/a.song".to_string(), "Songs-main/sub/b.song".to_string()];
        assert_eq!(common_root(&names), "Songs-main/");
        let names = vec!["a.song".to_string(), "sub/b.song".to_string()];
        assert_eq!(common_root(&names), "");
    }
}
