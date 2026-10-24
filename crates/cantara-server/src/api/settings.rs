//! The designs and slide divisions a tenant shares.
//!
//! Every member may read them; who may change them is a grant on the
//! `settings` resource of the tenant (its id is the tenant's). Versioned like
//! items: a save is a new version, and a save from an outdated version is
//! refused.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use cantara_api::{ResourceKind, Role, SaveSettings, SettingsEntry, SettingsKind, UserRef};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::grants::require;
use crate::auth::{member, CurrentUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

fn parse_kind(kind: &str) -> AppResult<SettingsKind> {
    match kind {
        "design" => Ok(SettingsKind::Design),
        "slide_settings" => Ok(SettingsKind::SlideSettings),
        _ => Err(AppError::NotFound),
    }
}

type EntryRow = (Uuid, Uuid, String, String, serde_json::Value, DateTime<Utc>, Option<Uuid>, Option<String>, bool);

const ENTRY_QUERY: &str = "SELECT e.lineage_uuid, v.uuid, e.kind, v.name, v.body, v.created_at, u.uuid, u.display_name,
            v.package_sha256 IS NOT NULL
       FROM settings_entries e JOIN settings_versions v ON v.uuid = e.current_version
       LEFT JOIN users u ON u.uuid = v.created_by
      WHERE e.tenant_uuid = $1 AND e.deleted_at IS NULL";

fn entry(row: EntryRow) -> SettingsEntry {
    let (lineage, version, kind, name, body, updated_at, by, by_name, has_package) = row;
    SettingsEntry {
        lineage,
        version,
        kind: parse_kind(&kind).unwrap_or(SettingsKind::Design),
        name,
        body,
        updated_at,
        updated_by: by.zip(by_name).map(|(uuid, display_name)| UserRef { uuid, display_name }),
        has_package,
    }
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ListQuery {
    /// `design` or `slide_settings`; both without it.
    pub kind: Option<String>,
}

/// The tenant's designs and slide divisions.
#[utoipa::path(get, path = "/tenants/{tenant}/settings", tag = "settings",
    params(("tenant" = Uuid, Path), ListQuery), responses((status = 200, body = [SettingsEntry])))]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Vec<SettingsEntry>>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Settings, tenant, Role::Read).await?;
    let kind = query.kind.as_deref().map(parse_kind).transpose()?;
    let rows: Vec<EntryRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{ENTRY_QUERY} AND ($2::text IS NULL OR e.kind = $2) ORDER BY e.kind, v.name"
    )))
    .bind(tenant)
    .bind(kind.map(SettingsKind::as_str))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(entry).collect()))
}

async fn get_entry(state: &AppState, tenant: Uuid, lineage: Uuid) -> AppResult<SettingsEntry> {
    let row: EntryRow = sqlx::query_as(sqlx::AssertSqlSafe(format!("{ENTRY_QUERY} AND e.lineage_uuid = $2")))
        .bind(tenant)
        .bind(lineage)
        .fetch_one(&state.db)
        .await?;
    Ok(entry(row))
}

/// Adds a design or a slide division.
#[utoipa::path(post, path = "/tenants/{tenant}/settings/{kind}", tag = "settings", request_body = SaveSettings,
    params(("tenant" = Uuid, Path), ("kind" = String, Path)), responses((status = 200, body = SettingsEntry)))]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, kind)): Path<(Uuid, String)>,
    Json(request): Json<SaveSettings>,
) -> AppResult<Json<SettingsEntry>> {
    let kind = parse_kind(&kind)?;
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Settings, tenant, Role::Write).await?;
    if request.name.trim().is_empty() || !request.body.is_object() {
        return Err(AppError::BadRequest("a name and a JSON object are needed".to_string()));
    }
    let lineage = Uuid::now_v7();
    let entry_uuid = Uuid::now_v7();
    let version = Uuid::now_v7();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO settings_entries (uuid, tenant_uuid, kind, lineage_uuid, current_version) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(entry_uuid)
    .bind(tenant)
    .bind(kind.as_str())
    .bind(lineage)
    .bind(version)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO settings_versions (uuid, entry_uuid, name, body, created_by) VALUES ($1, $2, $3, $4, $5)")
        .bind(version)
        .bind(entry_uuid)
        .bind(request.name.trim())
        .bind(&request.body)
        .bind(user.uuid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(get_entry(&state, tenant, lineage).await?))
}

/// Saves a new version of a design or slide division.
#[utoipa::path(put, path = "/tenants/{tenant}/settings/{kind}/{lineage}", tag = "settings", request_body = SaveSettings,
    params(("tenant" = Uuid, Path), ("kind" = String, Path), ("lineage" = Uuid, Path)),
    responses((status = 200, body = SettingsEntry), (status = 409, body = cantara_api::ApiError)))]
pub async fn save(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, kind, lineage)): Path<(Uuid, String, Uuid)>,
    Json(request): Json<SaveSettings>,
) -> AppResult<Json<SettingsEntry>> {
    let kind = parse_kind(&kind)?;
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Settings, tenant, Role::Write).await?;
    if request.name.trim().is_empty() || !request.body.is_object() {
        return Err(AppError::BadRequest("a name and a JSON object are needed".to_string()));
    }

    let mut tx = state.db.begin().await?;
    let (entry_uuid, current): (Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT uuid, current_version FROM settings_entries
          WHERE tenant_uuid = $1 AND lineage_uuid = $2 AND kind = $3 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(tenant)
    .bind(lineage)
    .bind(kind.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if request.base_version.is_some_and(|base| Some(base) != current) {
        return Err(AppError::VersionConflict { current });
    }
    let version = Uuid::now_v7();
    sqlx::query("INSERT INTO settings_versions (uuid, entry_uuid, name, body, created_by) VALUES ($1, $2, $3, $4, $5)")
        .bind(version)
        .bind(entry_uuid)
        .bind(request.name.trim())
        .bind(&request.body)
        .bind(user.uuid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE settings_entries SET current_version = $2 WHERE uuid = $1")
        .bind(entry_uuid)
        .bind(version)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(get_entry(&state, tenant, lineage).await?))
}

/// Removes a design or slide division.
#[utoipa::path(delete, path = "/tenants/{tenant}/settings/{kind}/{lineage}", tag = "settings",
    params(("tenant" = Uuid, Path), ("kind" = String, Path), ("lineage" = Uuid, Path)), responses((status = 204)))]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, kind, lineage)): Path<(Uuid, String, Uuid)>,
) -> AppResult<StatusCode> {
    let kind = parse_kind(&kind)?;
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Settings, tenant, Role::Write).await?;
    let deleted = sqlx::query(
        "UPDATE settings_entries SET deleted_at = now()
          WHERE tenant_uuid = $1 AND lineage_uuid = $2 AND kind = $3 AND deleted_at IS NULL",
    )
    .bind(tenant)
    .bind(lineage)
    .bind(kind.as_str())
    .execute(&state.db)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Attaches the design's package — the `.cantara-design.zip` with its fonts
/// and background picture — to its current version, so that whoever takes the
/// design over gets what it needs to look right.
#[utoipa::path(put, path = "/tenants/{tenant}/settings/{kind}/{lineage}/package", tag = "settings",
    params(("tenant" = Uuid, Path), ("kind" = String, Path), ("lineage" = Uuid, Path)),
    request_body(content_type = "application/zip", content = Vec<u8>),
    responses((status = 200, body = SettingsEntry)))]
pub async fn put_package(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, kind, lineage)): Path<(Uuid, String, Uuid)>,
    body: axum::body::Body,
) -> AppResult<Json<SettingsEntry>> {
    use futures_util::StreamExt;

    let kind = parse_kind(&kind)?;
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Settings, tenant, Role::Write).await?;
    let version: Uuid = sqlx::query_scalar(
        "SELECT current_version FROM settings_entries
          WHERE tenant_uuid = $1 AND lineage_uuid = $2 AND kind = $3 AND deleted_at IS NULL",
    )
    .bind(tenant)
    .bind(lineage)
    .bind(kind.as_str())
    .fetch_one(&state.db)
    .await?;

    let mut incoming = state.blobs.incoming(state.config.max_upload_bytes)?;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        incoming.write(&chunk.map_err(|error| AppError::BadRequest(error.to_string()))?)?;
    }
    let received = incoming.finish()?;
    let sha256 = received.sha256.clone();
    let size = received.size as i64;
    let blobs = state.blobs.clone();
    tokio::task::spawn_blocking(move || blobs.store(received))
        .await
        .map_err(AppError::internal)??;

    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO blobs (sha256, size) VALUES ($1, $2) ON CONFLICT (sha256) DO UPDATE SET created_at = now()")
        .bind(&sha256)
        .bind(size)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE settings_versions SET package_sha256 = $2 WHERE uuid = $1")
        .bind(version)
        .bind(&sha256)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(get_entry(&state, tenant, lineage).await?))
}

/// The package of a design's current version.
#[utoipa::path(get, path = "/tenants/{tenant}/settings/{kind}/{lineage}/package", tag = "settings",
    params(("tenant" = Uuid, Path), ("kind" = String, Path), ("lineage" = Uuid, Path)),
    responses((status = 200, content_type = "application/zip"), (status = 404)))]
pub async fn get_package(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, kind, lineage)): Path<(Uuid, String, Uuid)>,
    request: axum::extract::Request,
) -> AppResult<axum::response::Response> {
    let kind = parse_kind(&kind)?;
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Settings, tenant, Role::Read).await?;
    let sha256: Option<String> = sqlx::query_scalar(
        "SELECT v.package_sha256 FROM settings_entries e JOIN settings_versions v ON v.uuid = e.current_version
          WHERE e.tenant_uuid = $1 AND e.lineage_uuid = $2 AND e.kind = $3 AND e.deleted_at IS NULL",
    )
    .bind(tenant)
    .bind(lineage)
    .bind(kind.as_str())
    .fetch_one(&state.db)
    .await?;
    let sha256 = sha256.ok_or(AppError::NotFound)?;
    crate::api::libraries::serve_blob(&state, &sha256, "design.cantara-design.zip", request).await
}
