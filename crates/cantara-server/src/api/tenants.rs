//! Tenants, their members, their groups, and invitations into them.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use cantara_api::{
    CreateInvitation, CreateTenant, GroupInfo, GroupMemberInfo, GroupRef, GroupRole, InvitationInfo,
    InvitationPreview, MemberInfo, SaveGroup, SaveGroupMember, TenantInfo, UserInfo, UserRef,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::account::{group_role_str, parse_group_role};
use crate::auth::{member, CurrentUser, Member};
use crate::bootstrap::{create_tenant, ADMINS_GROUP};
use crate::error::{AppError, AppResult};
use crate::invitations;
use crate::secrets::{hash_token, random_token};
use crate::state::AppState;

async fn tenant_info(state: &AppState, tenant: Uuid) -> AppResult<TenantInfo> {
    let (uuid, name, slug): (Uuid, String, String) =
        sqlx::query_as("SELECT uuid, name, slug FROM tenants WHERE uuid = $1")
            .bind(tenant)
            .fetch_one(&state.db)
            .await?;
    Ok(TenantInfo { uuid, name, slug })
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The tenants the user belongs to — every tenant, for a server admin.
#[utoipa::path(get, path = "/tenants", tag = "tenants", responses((status = 200, body = [TenantInfo])))]
pub async fn list(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<TenantInfo>>> {
    let me = crate::api::account::load_me(&state, &user).await?;
    Ok(Json(me.tenants.into_iter().map(|membership| membership.tenant).collect()))
}

/// Creates a tenant. Only a server admin may, and becomes its first admin.
#[utoipa::path(post, path = "/tenants", tag = "tenants", request_body = CreateTenant,
    responses((status = 200, body = TenantInfo)))]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<CreateTenant>,
) -> AppResult<Json<TenantInfo>> {
    if !user.is_server_admin {
        return Err(AppError::Forbidden);
    }
    if request.name.trim().is_empty() || !valid_slug(&request.slug) {
        return Err(AppError::BadRequest(
            "a tenant needs a name and a slug of lowercase letters, digits and dashes".to_string(),
        ));
    }
    let tenant = create_tenant(&state.db, request.name.trim(), &request.slug, Some(user.uuid)).await?;
    Ok(Json(tenant_info(&state, tenant).await?))
}

/// Renames a tenant.
#[utoipa::path(patch, path = "/tenants/{tenant}", tag = "tenants", request_body = CreateTenant,
    params(("tenant" = Uuid, Path)), responses((status = 200, body = TenantInfo)))]
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
    Json(request): Json<CreateTenant>,
) -> AppResult<Json<TenantInfo>> {
    member(&state, &user, tenant).await?.require_admin()?;
    if request.name.trim().is_empty() || !valid_slug(&request.slug) {
        return Err(AppError::BadRequest("a name and a valid slug are needed".to_string()));
    }
    sqlx::query("UPDATE tenants SET name = $2, slug = $3 WHERE uuid = $1")
        .bind(tenant)
        .bind(request.name.trim())
        .bind(&request.slug)
        .execute(&state.db)
        .await?;
    Ok(Json(tenant_info(&state, tenant).await?))
}

// ── Members ──────────────────────────────────────────────────────────────────

/// The tenant's members. Every member may see who else is one — they are who
/// a selection can be shared with.
#[utoipa::path(get, path = "/tenants/{tenant}/members", tag = "tenants",
    params(("tenant" = Uuid, Path)), responses((status = 200, body = [MemberInfo])))]
pub async fn members(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
) -> AppResult<Json<Vec<MemberInfo>>> {
    member(&state, &user, tenant).await?;
    let rows: Vec<(Uuid, String, String, bool, DateTime<Utc>, bool)> = sqlx::query_as(
        "SELECT u.uuid, u.email, u.display_name, u.is_server_admin, m.joined_at,
                EXISTS (SELECT 1 FROM group_members gm JOIN groups g ON g.uuid = gm.group_uuid
                         WHERE g.tenant_uuid = m.tenant_uuid AND g.system AND gm.user_uuid = u.uuid)
           FROM memberships m JOIN users u ON u.uuid = m.user_uuid
          WHERE m.tenant_uuid = $1 ORDER BY u.display_name",
    )
    .bind(tenant)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(uuid, email, display_name, is_server_admin, joined_at, is_admin)| MemberInfo {
                user: UserInfo {
                    uuid,
                    email,
                    display_name,
                    is_server_admin,
                },
                is_admin,
                joined_at,
            })
            .collect(),
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateMember {
    pub is_admin: bool,
}

async fn admins_group(state: &AppState, tenant: Uuid) -> AppResult<Uuid> {
    Ok(sqlx::query_scalar("SELECT uuid FROM groups WHERE tenant_uuid = $1 AND system")
        .bind(tenant)
        .fetch_one(&state.db)
        .await?)
}

async fn is_member_of(state: &AppState, tenant: Uuid, user: Uuid) -> AppResult<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memberships WHERE tenant_uuid = $1 AND user_uuid = $2)")
        .bind(tenant)
        .bind(user)
        .fetch_one(&state.db)
        .await?)
}

/// Makes a member an admin, or no longer one.
#[utoipa::path(patch, path = "/tenants/{tenant}/members/{user}", tag = "tenants", request_body = UpdateMember,
    params(("tenant" = Uuid, Path), ("user" = Uuid, Path)), responses((status = 204)))]
pub async fn update_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, target)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateMember>,
) -> AppResult<StatusCode> {
    member(&state, &user, tenant).await?.require_admin()?;
    if !is_member_of(&state, tenant, target).await? {
        return Err(AppError::NotFound);
    }
    let admins = admins_group(&state, tenant).await?;
    if request.is_admin {
        sqlx::query("INSERT INTO group_members (group_uuid, user_uuid, role) VALUES ($1, $2, 'manager') ON CONFLICT DO NOTHING")
            .bind(admins)
            .bind(target)
            .execute(&state.db)
            .await?;
    } else {
        refuse_last_admin(&state, admins, target).await?;
        sqlx::query("DELETE FROM group_members WHERE group_uuid = $1 AND user_uuid = $2")
            .bind(admins)
            .bind(target)
            .execute(&state.db)
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// A tenant without an admin could not be administered by anybody but a
/// server admin, so the last one stays.
async fn refuse_last_admin(state: &AppState, admins: Uuid, leaving: Uuid) -> AppResult<()> {
    let others: i64 = sqlx::query_scalar("SELECT count(*) FROM group_members WHERE group_uuid = $1 AND user_uuid <> $2")
        .bind(admins)
        .bind(leaving)
        .fetch_one(&state.db)
        .await?;
    let is_admin: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM group_members WHERE group_uuid = $1 AND user_uuid = $2)")
        .bind(admins)
        .bind(leaving)
        .fetch_one(&state.db)
        .await?;
    if is_admin && others == 0 {
        return Err(AppError::Conflict("the tenant's last admin cannot leave that role".to_string()));
    }
    Ok(())
}

/// Removes somebody from the tenant, its groups, and everything shared with
/// them there.
#[utoipa::path(delete, path = "/tenants/{tenant}/members/{user}", tag = "tenants",
    params(("tenant" = Uuid, Path), ("user" = Uuid, Path)), responses((status = 204)))]
pub async fn remove_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, target)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let acting = member(&state, &user, tenant).await?;
    // Anybody may leave; only an admin may remove somebody else.
    if target != user.uuid {
        acting.require_admin()?;
    }
    refuse_last_admin(&state, admins_group(&state, tenant).await?, target).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "DELETE FROM group_members WHERE user_uuid = $2
            AND group_uuid IN (SELECT uuid FROM groups WHERE tenant_uuid = $1)",
    )
    .bind(tenant)
    .bind(target)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM grants WHERE tenant_uuid = $1 AND user_uuid = $2")
        .bind(tenant)
        .bind(target)
        .execute(&mut *tx)
        .await?;
    let removed = sqlx::query("DELETE FROM memberships WHERE tenant_uuid = $1 AND user_uuid = $2")
        .bind(tenant)
        .bind(target)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if removed.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ── Groups ───────────────────────────────────────────────────────────────────

/// The tenant's groups.
#[utoipa::path(get, path = "/tenants/{tenant}/groups", tag = "groups",
    params(("tenant" = Uuid, Path)), responses((status = 200, body = [GroupInfo])))]
pub async fn groups(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
) -> AppResult<Json<Vec<GroupInfo>>> {
    member(&state, &user, tenant).await?;
    let rows: Vec<(Uuid, String, bool, i64)> = sqlx::query_as(
        "SELECT g.uuid, g.name, g.system, (SELECT count(*) FROM group_members m WHERE m.group_uuid = g.uuid)
           FROM groups g WHERE g.tenant_uuid = $1 ORDER BY g.system DESC, g.name",
    )
    .bind(tenant)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(uuid, name, system, member_count)| GroupInfo {
                uuid,
                name,
                system,
                member_count,
            })
            .collect(),
    ))
}

async fn group_of(state: &AppState, tenant: Uuid, group: Uuid) -> AppResult<(String, bool)> {
    Ok(sqlx::query_as("SELECT name, system FROM groups WHERE uuid = $1 AND tenant_uuid = $2")
        .bind(group)
        .bind(tenant)
        .fetch_one(&state.db)
        .await?)
}

/// Creates a group.
#[utoipa::path(post, path = "/tenants/{tenant}/groups", tag = "groups", request_body = SaveGroup,
    params(("tenant" = Uuid, Path)), responses((status = 200, body = GroupInfo)))]
pub async fn create_group(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
    Json(request): Json<SaveGroup>,
) -> AppResult<Json<GroupInfo>> {
    member(&state, &user, tenant).await?.require_admin()?;
    let name = request.name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case(ADMINS_GROUP) {
        return Err(AppError::BadRequest("that name cannot be used for a group".to_string()));
    }
    let uuid = Uuid::now_v7();
    sqlx::query("INSERT INTO groups (uuid, tenant_uuid, name) VALUES ($1, $2, $3)")
        .bind(uuid)
        .bind(tenant)
        .bind(name)
        .execute(&state.db)
        .await?;
    Ok(Json(GroupInfo {
        uuid,
        name: name.to_string(),
        system: false,
        member_count: 0,
    }))
}

/// Renames a group.
#[utoipa::path(patch, path = "/tenants/{tenant}/groups/{group}", tag = "groups", request_body = SaveGroup,
    params(("tenant" = Uuid, Path), ("group" = Uuid, Path)), responses((status = 204)))]
pub async fn update_group(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, group)): Path<(Uuid, Uuid)>,
    Json(request): Json<SaveGroup>,
) -> AppResult<StatusCode> {
    member(&state, &user, tenant).await?.require_admin()?;
    let (_, system) = group_of(&state, tenant, group).await?;
    let name = request.name.trim();
    if system || name.is_empty() || name.eq_ignore_ascii_case(ADMINS_GROUP) {
        return Err(AppError::BadRequest("this group cannot be renamed so".to_string()));
    }
    sqlx::query("UPDATE groups SET name = $3 WHERE uuid = $1 AND tenant_uuid = $2")
        .bind(group)
        .bind(tenant)
        .bind(name)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes a group, and whatever was shared with it.
#[utoipa::path(delete, path = "/tenants/{tenant}/groups/{group}", tag = "groups",
    params(("tenant" = Uuid, Path), ("group" = Uuid, Path)), responses((status = 204)))]
pub async fn delete_group(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, group)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    member(&state, &user, tenant).await?.require_admin()?;
    let (_, system) = group_of(&state, tenant, group).await?;
    if system {
        return Err(AppError::BadRequest("the admins group cannot be deleted".to_string()));
    }
    sqlx::query("DELETE FROM groups WHERE uuid = $1 AND tenant_uuid = $2")
        .bind(group)
        .bind(tenant)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Whether `acting` may change who is in `group`: an admin always, a manager
/// of the group for ordinary members. Who manages a group, and who is an
/// admin, is for admins to say.
async fn may_manage_group(state: &AppState, acting: &Member, group: Uuid, system: bool) -> AppResult<bool> {
    if acting.is_admin {
        return Ok(true);
    }
    if system {
        return Ok(false);
    }
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM group_members WHERE group_uuid = $1 AND user_uuid = $2")
        .bind(group)
        .bind(acting.user.uuid)
        .fetch_optional(&state.db)
        .await?;
    Ok(role.as_deref() == Some("manager"))
}

/// Who is in a group.
#[utoipa::path(get, path = "/tenants/{tenant}/groups/{group}/members", tag = "groups",
    params(("tenant" = Uuid, Path), ("group" = Uuid, Path)), responses((status = 200, body = [GroupMemberInfo])))]
pub async fn group_members(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, group)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Vec<GroupMemberInfo>>> {
    member(&state, &user, tenant).await?;
    group_of(&state, tenant, group).await?;
    let rows: Vec<(Uuid, String, String, bool, String)> = sqlx::query_as(
        "SELECT u.uuid, u.email, u.display_name, u.is_server_admin, m.role
           FROM group_members m JOIN users u ON u.uuid = m.user_uuid
          WHERE m.group_uuid = $1 ORDER BY u.display_name",
    )
    .bind(group)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(uuid, email, display_name, is_server_admin, role)| GroupMemberInfo {
                user: UserInfo {
                    uuid,
                    email,
                    display_name,
                    is_server_admin,
                },
                role: parse_group_role(&role),
            })
            .collect(),
    ))
}

/// Puts a member of the tenant into a group, or changes their role in it.
#[utoipa::path(put, path = "/tenants/{tenant}/groups/{group}/members", tag = "groups", request_body = SaveGroupMember,
    params(("tenant" = Uuid, Path), ("group" = Uuid, Path)), responses((status = 204)))]
pub async fn save_group_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, group)): Path<(Uuid, Uuid)>,
    Json(request): Json<SaveGroupMember>,
) -> AppResult<StatusCode> {
    let acting = member(&state, &user, tenant).await?;
    let (_, system) = group_of(&state, tenant, group).await?;
    if !may_manage_group(&state, &acting, group, system).await?
        || (request.role == GroupRole::Manager && !acting.is_admin)
    {
        return Err(AppError::Forbidden);
    }
    if !is_member_of(&state, tenant, request.user).await? {
        return Err(AppError::BadRequest("only members of the tenant can join its groups".to_string()));
    }
    sqlx::query(
        "INSERT INTO group_members (group_uuid, user_uuid, role) VALUES ($1, $2, $3)
         ON CONFLICT (group_uuid, user_uuid) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(group)
    .bind(request.user)
    .bind(group_role_str(request.role))
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Takes somebody out of a group.
#[utoipa::path(delete, path = "/tenants/{tenant}/groups/{group}/members/{user}", tag = "groups",
    params(("tenant" = Uuid, Path), ("group" = Uuid, Path), ("user" = Uuid, Path)), responses((status = 204)))]
pub async fn remove_group_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, group, target)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let acting = member(&state, &user, tenant).await?;
    let (_, system) = group_of(&state, tenant, group).await?;
    if target != user.uuid && !may_manage_group(&state, &acting, group, system).await? {
        return Err(AppError::Forbidden);
    }
    if system {
        refuse_last_admin(&state, group, target).await?;
    }
    sqlx::query("DELETE FROM group_members WHERE group_uuid = $1 AND user_uuid = $2")
        .bind(group)
        .bind(target)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Invitations ──────────────────────────────────────────────────────────────

/// The invitations not yet accepted: all of them for an admin, those into
/// their groups for a manager.
#[utoipa::path(get, path = "/tenants/{tenant}/invitations", tag = "invitations",
    params(("tenant" = Uuid, Path)), responses((status = 200, body = [InvitationInfo])))]
pub async fn invitations(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
) -> AppResult<Json<Vec<InvitationInfo>>> {
    let acting = member(&state, &user, tenant).await?;
    let rows: Vec<(Uuid, String, Option<Uuid>, Option<String>, String, Option<Uuid>, Option<String>, DateTime<Utc>)> =
        sqlx::query_as(
            "SELECT i.uuid, i.email, g.uuid, g.name, i.role, u.uuid, u.display_name, i.expires_at
               FROM invitations i
               LEFT JOIN groups g ON g.uuid = i.group_uuid
               LEFT JOIN users u ON u.uuid = i.invited_by
              WHERE i.tenant_uuid = $1 AND i.accepted_at IS NULL AND i.expires_at > now()
                AND ($2 OR i.group_uuid IN (SELECT group_uuid FROM group_members WHERE user_uuid = $3 AND role = 'manager'))
              ORDER BY i.created_at DESC",
        )
        .bind(tenant)
        .bind(acting.is_admin)
        .bind(user.uuid)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(uuid, email, group, group_name, role, by, by_name, expires_at)| InvitationInfo {
                uuid,
                email,
                group: group.zip(group_name).map(|(uuid, name)| GroupRef {
                    uuid,
                    name,
                    role: parse_group_role(&role),
                }),
                invited_by: by.zip(by_name).map(|(uuid, display_name)| UserRef { uuid, display_name }),
                expires_at,
                link: None,
                mailed: false,
            })
            .collect(),
    ))
}

/// Invites an address. An admin may invite into the tenant and into any
/// group; a group manager into their group, as a member.
#[utoipa::path(post, path = "/tenants/{tenant}/invitations", tag = "invitations", request_body = CreateInvitation,
    params(("tenant" = Uuid, Path)), responses((status = 200, body = InvitationInfo)))]
pub async fn invite(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
    Json(request): Json<CreateInvitation>,
) -> AppResult<Json<InvitationInfo>> {
    let acting = member(&state, &user, tenant).await?;
    let email = request.email.trim().to_string();
    if !email.contains('@') {
        return Err(AppError::BadRequest("that is not an e-mail address".to_string()));
    }
    let group = match request.group {
        Some(group) => {
            let (name, system) = group_of(&state, tenant, group).await?;
            if !may_manage_group(&state, &acting, group, system).await?
                || (request.role == GroupRole::Manager && !acting.is_admin)
            {
                return Err(AppError::Forbidden);
            }
            Some(GroupRef {
                uuid: group,
                name,
                role: request.role,
            })
        }
        None => {
            acting.require_admin()?;
            None
        }
    };

    let token = random_token()?;
    let uuid = Uuid::now_v7();
    let expires_at = Utc::now() + Duration::days(invitations::VALID_DAYS);
    sqlx::query(
        "INSERT INTO invitations (uuid, tenant_uuid, email, group_uuid, role, token_hash, invited_by, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(uuid)
    .bind(tenant)
    .bind(&email)
    .bind(request.group)
    .bind(group_role_str(request.role))
    .bind(hash_token(&token))
    .bind(user.uuid)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    Ok(Json(InvitationInfo {
        uuid,
        email,
        group,
        invited_by: Some(UserRef {
            uuid: user.uuid,
            display_name: user.display_name.clone(),
        }),
        expires_at,
        link: Some(format!("{}/invite/{token}", state.config.public_url)),
        mailed: false,
    }))
}

/// Withdraws an invitation.
#[utoipa::path(delete, path = "/tenants/{tenant}/invitations/{invitation}", tag = "invitations",
    params(("tenant" = Uuid, Path), ("invitation" = Uuid, Path)), responses((status = 204)))]
pub async fn withdraw(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, invitation)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let acting = member(&state, &user, tenant).await?;
    let deleted = sqlx::query(
        "DELETE FROM invitations WHERE uuid = $1 AND tenant_uuid = $2 AND ($3 OR invited_by = $4)",
    )
    .bind(invitation)
    .bind(tenant)
    .bind(acting.is_admin)
    .bind(user.uuid)
    .execute(&state.db)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// What an invitation link leads to. Needs no login: it is what is shown
/// before one.
#[utoipa::path(get, path = "/invitations/{token}", tag = "invitations",
    params(("token" = String, Path)), responses((status = 200, body = InvitationPreview)))]
pub async fn preview(State(state): State<AppState>, Path(token): Path<String>) -> AppResult<Json<InvitationPreview>> {
    Ok(Json(invitations::preview(&invitations::open(&state, &token).await?)))
}

/// Accepts an invitation for the logged-in account.
#[utoipa::path(post, path = "/invitations/{token}/accept", tag = "invitations",
    params(("token" = String, Path)), responses((status = 200, body = TenantInfo)))]
pub async fn accept(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(token): Path<String>,
) -> AppResult<Json<TenantInfo>> {
    let invitation = invitations::open(&state, &token).await?;
    invitations::accept(&state, &invitation, &user).await?;
    Ok(Json(invitation.tenant))
}
