//! Invitations: a link that lets one e-mail address into a tenant, and
//! optionally into one of its groups.

use cantara_api::{GroupRole, InvitationPreview, TenantInfo};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::api::account::parse_group_role;
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::secrets::hash_token;
use crate::state::AppState;

/// How long an invitation link is valid.
pub const VALID_DAYS: i64 = 14;

/// An invitation that may still be accepted.
#[derive(Clone, Debug)]
pub struct Open {
    pub uuid: Uuid,
    pub tenant: TenantInfo,
    pub email: String,
    pub group: Option<(Uuid, String)>,
    pub role: GroupRole,
    pub expires_at: DateTime<Utc>,
}

/// The invitation behind `token`, if it is valid.
pub async fn open(state: &AppState, token: &str) -> AppResult<Open> {
    let row: Option<(Uuid, Uuid, String, String, String, Option<Uuid>, Option<String>, String, DateTime<Utc>)> =
        sqlx::query_as(
            "SELECT i.uuid, t.uuid, t.name, t.slug, i.email, g.uuid, g.name, i.role, i.expires_at
               FROM invitations i
               JOIN tenants t ON t.uuid = i.tenant_uuid
               LEFT JOIN groups g ON g.uuid = i.group_uuid
              WHERE i.token_hash = $1 AND i.accepted_at IS NULL AND i.expires_at > now()",
        )
        .bind(hash_token(token))
        .fetch_optional(&state.db)
        .await?;
    let (uuid, tenant, name, slug, email, group, group_name, role, expires_at) = row.ok_or(AppError::NotFound)?;
    Ok(Open {
        uuid,
        tenant: TenantInfo { uuid: tenant, name, slug },
        email,
        group: group.zip(group_name),
        role: parse_group_role(&role),
        expires_at,
    })
}

pub fn preview(invitation: &Open) -> InvitationPreview {
    InvitationPreview {
        tenant: invitation.tenant.clone(),
        group: invitation.group.as_ref().map(|(_, name)| name.clone()),
        email: invitation.email.clone(),
        expires_at: invitation.expires_at,
    }
}

/// Lets `user` in where the invitation says. The address has to be the one
/// invited: an invitation passed on is not an invitation.
pub async fn accept(state: &AppState, invitation: &Open, user: &CurrentUser) -> AppResult<()> {
    if !invitation.email.eq_ignore_ascii_case(&user.email) {
        return Err(AppError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO memberships (tenant_uuid, user_uuid) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(invitation.tenant.uuid)
        .bind(user.uuid)
        .execute(&mut *tx)
        .await?;
    if let Some((group, _)) = &invitation.group {
        sqlx::query(
            "INSERT INTO group_members (group_uuid, user_uuid, role) VALUES ($1, $2, $3)
             ON CONFLICT (group_uuid, user_uuid) DO UPDATE SET role = EXCLUDED.role",
        )
        .bind(group)
        .bind(user.uuid)
        .bind(crate::api::account::group_role_str(invitation.role))
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE invitations SET accepted_at = now() WHERE uuid = $1")
        .bind(invitation.uuid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
