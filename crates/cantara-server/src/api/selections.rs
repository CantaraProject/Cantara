//! Saved running orders, and who they are shared with.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use cantara_api::{ResourceKind, Role, SaveSelection, SelectionDetail, SelectionSummary, UserRef};
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::api::grants::{require, resource_role};
use crate::auth::{member, CurrentUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

type SelectionRow = (Uuid, String, Option<NaiveDate>, String, Option<Uuid>, Option<String>, DateTime<Utc>, serde_json::Value);

const SELECTION_QUERY: &str = "SELECT s.uuid, s.name, s.date, s.note, u.uuid, u.display_name, s.updated_at, s.manifest
       FROM selections s LEFT JOIN users u ON u.uuid = s.owner_uuid
      WHERE s.tenant_uuid = $1";

fn summary(row: &SelectionRow, role: Role) -> SelectionSummary {
    let (uuid, name, date, note, owner, owner_name, updated_at, manifest) = row;
    SelectionSummary {
        uuid: *uuid,
        name: name.clone(),
        date: *date,
        note: note.clone(),
        owner: UserRef {
            uuid: owner.unwrap_or_default(),
            display_name: owner_name.clone().unwrap_or_default(),
        },
        updated_at: *updated_at,
        item_count: manifest
            .get("items")
            .and_then(|items| items.as_array())
            .map(Vec::len)
            .unwrap_or(0),
        role,
    }
}

/// The manifest has to say what it is; anything else is not a running order.
fn check_manifest(manifest: &serde_json::Value) -> AppResult<()> {
    let format = manifest.get("format").and_then(|format| format.as_str());
    let items = manifest.get("items").map(|items| items.is_array());
    if format != Some("cantara.selection") || items != Some(true) {
        return Err(AppError::BadRequest(
            "the manifest is not a `cantara.selection` with items".to_string(),
        ));
    }
    Ok(())
}

/// The running orders the user may see, the next service first.
#[utoipa::path(get, path = "/tenants/{tenant}/selections", tag = "selections",
    params(("tenant" = Uuid, Path)), responses((status = 200, body = [SelectionSummary])))]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
) -> AppResult<Json<Vec<SelectionSummary>>> {
    let member = member(&state, &user, tenant).await?;
    let rows: Vec<SelectionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECTION_QUERY} ORDER BY s.date DESC NULLS LAST, s.updated_at DESC"
    )))
    .bind(tenant)
    .fetch_all(&state.db)
    .await?;
    let mut selections = Vec::new();
    for row in rows {
        if let Some(role) = resource_role(&state, &member, ResourceKind::Selection, row.0).await? {
            selections.push(summary(&row, role));
        }
    }
    Ok(Json(selections))
}

async fn detail(state: &AppState, tenant: Uuid, selection: Uuid, role: Role) -> AppResult<SelectionDetail> {
    let row: SelectionRow = sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECTION_QUERY} AND s.uuid = $2")))
        .bind(tenant)
        .bind(selection)
        .fetch_one(&state.db)
        .await?;
    Ok(SelectionDetail {
        summary: summary(&row, role),
        manifest: row.7,
    })
}

/// Saves a new running order, owned by the user.
#[utoipa::path(post, path = "/tenants/{tenant}/selections", tag = "selections", request_body = SaveSelection,
    params(("tenant" = Uuid, Path)), responses((status = 200, body = SelectionDetail)))]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
    Json(request): Json<SaveSelection>,
) -> AppResult<Json<SelectionDetail>> {
    member(&state, &user, tenant).await?;
    check_manifest(&request.manifest)?;
    if request.name.trim().is_empty() {
        return Err(AppError::BadRequest("a running order needs a name".to_string()));
    }
    let uuid = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO selections (uuid, tenant_uuid, owner_uuid, name, date, note, manifest)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(uuid)
    .bind(tenant)
    .bind(user.uuid)
    .bind(request.name.trim())
    .bind(request.date)
    .bind(request.note.trim())
    .bind(&request.manifest)
    .execute(&state.db)
    .await?;
    Ok(Json(detail(&state, tenant, uuid, Role::Manage).await?))
}

/// One running order, with everything in it.
#[utoipa::path(get, path = "/tenants/{tenant}/selections/{selection}", tag = "selections",
    params(("tenant" = Uuid, Path), ("selection" = Uuid, Path)), responses((status = 200, body = SelectionDetail)))]
pub async fn get(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, selection)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<SelectionDetail>> {
    let member = member(&state, &user, tenant).await?;
    let role = require(&state, &member, ResourceKind::Selection, selection, Role::Read).await?;
    Ok(Json(detail(&state, tenant, selection, role).await?))
}

/// Saves changes to a running order.
#[utoipa::path(put, path = "/tenants/{tenant}/selections/{selection}", tag = "selections", request_body = SaveSelection,
    params(("tenant" = Uuid, Path), ("selection" = Uuid, Path)), responses((status = 200, body = SelectionDetail)))]
pub async fn save(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, selection)): Path<(Uuid, Uuid)>,
    Json(request): Json<SaveSelection>,
) -> AppResult<Json<SelectionDetail>> {
    let member = member(&state, &user, tenant).await?;
    let role = require(&state, &member, ResourceKind::Selection, selection, Role::Write).await?;
    check_manifest(&request.manifest)?;
    if request.name.trim().is_empty() {
        return Err(AppError::BadRequest("a running order needs a name".to_string()));
    }
    sqlx::query(
        "UPDATE selections SET name = $3, date = $4, note = $5, manifest = $6, updated_at = now()
          WHERE uuid = $1 AND tenant_uuid = $2",
    )
    .bind(selection)
    .bind(tenant)
    .bind(request.name.trim())
    .bind(request.date)
    .bind(request.note.trim())
    .bind(&request.manifest)
    .execute(&state.db)
    .await?;
    Ok(Json(detail(&state, tenant, selection, role).await?))
}

/// Deletes a running order.
#[utoipa::path(delete, path = "/tenants/{tenant}/selections/{selection}", tag = "selections",
    params(("tenant" = Uuid, Path), ("selection" = Uuid, Path)), responses((status = 204)))]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, selection)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Selection, selection, Role::Manage).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM grants WHERE tenant_uuid = $1 AND resource_kind = 'selection' AND resource_uuid = $2")
        .bind(tenant)
        .bind(selection)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM selections WHERE uuid = $1 AND tenant_uuid = $2")
        .bind(selection)
        .bind(tenant)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
