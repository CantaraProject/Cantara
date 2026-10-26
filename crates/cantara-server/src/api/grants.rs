//! Who else may read, change or share a resource.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use cantara_api::{CreateGrant, GrantInfo, Grantee, ResourceKind, Role};
use uuid::Uuid;

use crate::auth::{kind_str, member, parse_role, role_str, CurrentUser, Member};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// The role `member` has on a resource, all things considered: being an
/// admin, owning it, and grants. `None` also for a resource that does not
/// exist in the member's tenant.
pub async fn resource_role(state: &AppState, member: &Member, kind: ResourceKind, resource: Uuid) -> AppResult<Option<Role>> {
    let (exists, owner): (bool, Option<Uuid>) = match kind {
        ResourceKind::Library => {
            let exists = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM libraries WHERE uuid = $1 AND tenant_uuid = $2)")
                .bind(resource)
                .bind(member.tenant)
                .fetch_one(&state.db)
                .await?;
            (exists, None)
        }
        ResourceKind::Selection | ResourceKind::Session => {
            let table = if kind == ResourceKind::Selection { "selections" } else { "sessions" };
            let owner: Option<Option<Uuid>> =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT owner_uuid FROM {table} WHERE uuid = $1 AND tenant_uuid = $2")))
                    .bind(resource)
                    .bind(member.tenant)
                    .fetch_optional(&state.db)
                    .await?;
            (owner.is_some(), owner.flatten())
        }
        ResourceKind::Settings => (resource == member.tenant, None),
    };
    if !exists {
        return Ok(None);
    }
    if owner == Some(member.user.uuid) {
        return Ok(Some(Role::Manage));
    }
    member.role_on(state, kind, resource).await
}

/// Like [`resource_role`], refusing below `needed`.
pub async fn require(state: &AppState, member: &Member, kind: ResourceKind, resource: Uuid, needed: Role) -> AppResult<Role> {
    match resource_role(state, member, kind, resource).await? {
        None => Err(AppError::NotFound),
        Some(role) if role < needed => Err(AppError::Forbidden),
        Some(role) => Ok(role),
    }
}

fn parse_kind(kind: &str) -> AppResult<ResourceKind> {
    match kind {
        "library" => Ok(ResourceKind::Library),
        "selection" => Ok(ResourceKind::Selection),
        "settings" => Ok(ResourceKind::Settings),
        "session" => Ok(ResourceKind::Session),
        _ => Err(AppError::NotFound),
    }
}

/// Who a resource is shared with.
#[utoipa::path(get, path = "/tenants/{tenant}/grants/{kind}/{resource}", tag = "grants",
    params(("tenant" = Uuid, Path), ("kind" = String, Path), ("resource" = Uuid, Path)),
    responses((status = 200, body = [GrantInfo])))]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, kind, resource)): Path<(Uuid, String, Uuid)>,
) -> AppResult<Json<Vec<GrantInfo>>> {
    let kind = parse_kind(&kind)?;
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, kind, resource, Role::Read).await?;
    let rows: Vec<(Uuid, Option<Uuid>, Option<Uuid>, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT gr.uuid, gr.user_uuid, gr.group_uuid, u.display_name, g.name, gr.role
           FROM grants gr
           LEFT JOIN users u ON u.uuid = gr.user_uuid
           LEFT JOIN groups g ON g.uuid = gr.group_uuid
          WHERE gr.tenant_uuid = $1 AND gr.resource_kind = $2 AND gr.resource_uuid = $3
          ORDER BY gr.created_at",
    )
    .bind(tenant)
    .bind(kind_str(kind))
    .bind(resource)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .filter_map(|(uuid, user, group, user_name, group_name, role)| {
                let (grantee, grantee_name) = match (user, group) {
                    (Some(user), _) => (Grantee::User(user), user_name.unwrap_or_default()),
                    (_, Some(group)) => (Grantee::Group(group), group_name.unwrap_or_default()),
                    _ => return None,
                };
                Some(GrantInfo {
                    uuid,
                    grantee,
                    grantee_name,
                    role: parse_role(&role),
                })
            })
            .collect(),
    ))
}

/// Shares a resource with a member or a group of the tenant — or changes how
/// far an existing share goes.
#[utoipa::path(post, path = "/tenants/{tenant}/grants/{kind}/{resource}", tag = "grants", request_body = CreateGrant,
    params(("tenant" = Uuid, Path), ("kind" = String, Path), ("resource" = Uuid, Path)),
    responses((status = 200, body = GrantInfo)))]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, kind, resource)): Path<(Uuid, String, Uuid)>,
    Json(request): Json<CreateGrant>,
) -> AppResult<Json<GrantInfo>> {
    let kind = parse_kind(&kind)?;
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, kind, resource, Role::Manage).await?;

    let (user_uuid, group_uuid, grantee_name): (Option<Uuid>, Option<Uuid>, String) = match request.grantee {
        Grantee::User(target) => {
            let name: Option<String> = sqlx::query_scalar(
                "SELECT u.display_name FROM users u JOIN memberships m ON m.user_uuid = u.uuid
                  WHERE u.uuid = $1 AND m.tenant_uuid = $2",
            )
            .bind(target)
            .bind(tenant)
            .fetch_optional(&state.db)
            .await?;
            (Some(target), None, name.ok_or(AppError::BadRequest("not a member of this tenant".to_string()))?)
        }
        Grantee::Group(group) => {
            let name: Option<String> = sqlx::query_scalar("SELECT name FROM groups WHERE uuid = $1 AND tenant_uuid = $2")
                .bind(group)
                .bind(tenant)
                .fetch_optional(&state.db)
                .await?;
            (None, Some(group), name.ok_or(AppError::BadRequest("not a group of this tenant".to_string()))?)
        }
    };

    let conflict = if user_uuid.is_some() {
        "(resource_kind, resource_uuid, user_uuid) WHERE user_uuid IS NOT NULL"
    } else {
        "(resource_kind, resource_uuid, group_uuid) WHERE group_uuid IS NOT NULL"
    };
    let uuid: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO grants (uuid, tenant_uuid, resource_kind, resource_uuid, user_uuid, group_uuid, role)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT {conflict} DO UPDATE SET role = EXCLUDED.role
         RETURNING uuid"
    )))
    .bind(Uuid::now_v7())
    .bind(tenant)
    .bind(kind_str(kind))
    .bind(resource)
    .bind(user_uuid)
    .bind(group_uuid)
    .bind(role_str(request.role))
    .fetch_one(&state.db)
    .await?;

    Ok(Json(GrantInfo {
        uuid,
        grantee: request.grantee,
        grantee_name,
        role: request.role,
    }))
}

/// Stops sharing.
#[utoipa::path(delete, path = "/tenants/{tenant}/grants/{kind}/{resource}/{grant}", tag = "grants",
    params(("tenant" = Uuid, Path), ("kind" = String, Path), ("resource" = Uuid, Path), ("grant" = Uuid, Path)),
    responses((status = 204)))]
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, kind, resource, grant)): Path<(Uuid, String, Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let kind = parse_kind(&kind)?;
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, kind, resource, Role::Manage).await?;
    let deleted = sqlx::query(
        "DELETE FROM grants WHERE uuid = $1 AND tenant_uuid = $2 AND resource_kind = $3 AND resource_uuid = $4",
    )
    .bind(grant)
    .bind(tenant)
    .bind(kind_str(kind))
    .bind(resource)
    .execute(&state.db)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
