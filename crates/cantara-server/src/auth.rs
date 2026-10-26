//! Who is asking, and what they may do.
//!
//! A request is authenticated by one of two things: the session cookie a
//! browser got when logging in, or an `Authorization: Bearer` token a program
//! was given (`POST /auth/token`). Both are rows of `auth_tokens`, kept as
//! hashes.
//!
//! Then, for anything that belongs to an organisation, [`member`] turns the
//! user and the tenant named in the path into a [`Member`] — or refuses. That
//! is the one place tenant isolation is decided: a handler that has a `Member`
//! for tenant A has nothing in its hands that reaches tenant B, and every query
//! it makes names `member.tenant`.

use axum::extract::FromRequestParts;
use axum::http::header::{AUTHORIZATION, COOKIE, ORIGIN};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method};
use cantara_api::{ResourceKind, Role, SESSION_COOKIE};
use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::secrets::{hash_token, random_token};
use crate::state::AppState;

/// The logged-in user.
#[derive(Clone, Debug)]
pub struct CurrentUser {
    pub uuid: Uuid,
    pub email: String,
    pub display_name: String,
    pub is_server_admin: bool,
}

/// The logged-in user, if there is one.
pub struct MaybeUser(pub Option<CurrentUser>);

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|token| token.trim().to_string())
}

/// The user a token belongs to, if it is valid. Keeps a used token alive: a
/// browser session lasts [`Config::session_days`](crate::config::Config) from
/// its last use, not from logging in.
pub async fn user_of_token(state: &AppState, token: &str) -> AppResult<Option<CurrentUser>> {
    let row: Option<(Uuid, String, String, bool, Uuid, String)> = sqlx::query_as(
        "SELECT u.uuid, u.email, u.display_name, u.is_server_admin, t.uuid, t.kind
           FROM auth_tokens t JOIN users u ON u.uuid = t.user_uuid
          WHERE t.token_hash = $1
            AND NOT u.disabled
            AND (t.expires_at IS NULL OR t.expires_at > now())",
    )
    .bind(hash_token(token))
    .fetch_optional(&state.db)
    .await?;

    let Some((uuid, email, display_name, is_server_admin, token_uuid, kind)) = row else {
        return Ok(None);
    };

    // Written at most every few minutes, so that reading does not turn every
    // request into a write.
    let extend = if kind == "session" {
        Some(Utc::now() + Duration::days(state.config.session_days))
    } else {
        None
    };
    sqlx::query(
        "UPDATE auth_tokens
            SET last_used_at = now(), expires_at = COALESCE($2, expires_at)
          WHERE uuid = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '5 minutes')",
    )
    .bind(token_uuid)
    .bind(extend)
    .execute(&state.db)
    .await?;

    Ok(Some(CurrentUser {
        uuid,
        email,
        display_name,
        is_server_admin,
    }))
}

/// Whether a request that changes something and carries the session cookie
/// comes from this server's own pages.
///
/// The cookie is `SameSite=Lax`, which already keeps it off a form posted from
/// elsewhere; this is the second lock. A program sending a bearer token is not
/// a browser that can be tricked, and is not asked.
fn same_origin(state: &AppState, parts: &Parts) -> bool {
    if matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    match parts.headers.get(ORIGIN).and_then(|origin| origin.to_str().ok()) {
        None => true,
        Some(origin) => origin == origin_of(&state.config.public_url),
    }
}

pub fn origin_of(url: &str) -> String {
    url::Url::parse(url)
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_else(|_| url.to_string())
}

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        if let Some(token) = bearer(&parts.headers) {
            return Ok(MaybeUser(user_of_token(state, &token).await?));
        }
        if let Some(token) = cookie_value(&parts.headers, SESSION_COOKIE) {
            if !same_origin(state, parts) {
                return Err(AppError::Forbidden);
            }
            return Ok(MaybeUser(user_of_token(state, &token).await?));
        }
        Ok(MaybeUser(None))
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        MaybeUser::from_request_parts(parts, state)
            .await?
            .0
            .ok_or(AppError::Unauthorized)
    }
}

/// Starts a browser session for `user` and returns its token.
pub async fn create_token(state: &AppState, user: Uuid, kind: &str, name: &str) -> AppResult<String> {
    let token = random_token()?;
    let expires = (kind == "session").then(|| Utc::now() + Duration::days(state.config.session_days));
    sqlx::query(
        "INSERT INTO auth_tokens (uuid, user_uuid, token_hash, kind, name, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(user)
    .bind(hash_token(&token))
    .bind(kind)
    .bind(name)
    .bind(expires)
    .execute(&state.db)
    .await?;
    Ok(token)
}

/// The `Set-Cookie` value that keeps a browser logged in.
pub fn session_cookie(state: &AppState, token: &str) -> HeaderValue {
    let secure = if state.config.secure_cookies() { "; Secure" } else { "" };
    let max_age = state.config.session_days * 24 * 60 * 60;
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn clear_cookie() -> HeaderValue {
    HeaderValue::from_static("cantara_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

// ── Roles ────────────────────────────────────────────────────────────────────

pub fn role_str(role: Role) -> &'static str {
    match role {
        Role::Read => "read",
        Role::Write => "write",
        Role::Manage => "manage",
    }
}

pub fn parse_role(value: &str) -> Role {
    match value {
        "manage" => Role::Manage,
        "write" => Role::Write,
        _ => Role::Read,
    }
}

pub fn kind_str(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Library => "library",
        ResourceKind::Selection => "selection",
        ResourceKind::Settings => "settings",
        ResourceKind::Session => "session",
    }
}

// ── Membership ───────────────────────────────────────────────────────────────

/// A user acting within one tenant.
#[derive(Clone, Debug)]
pub struct Member {
    pub user: CurrentUser,
    pub tenant: Uuid,
    /// In the tenant's `admins` group — or a server admin, who may act as one
    /// in every tenant.
    pub is_admin: bool,
    pub groups: Vec<Uuid>,
}

/// `user` as a member of `tenant`. A tenant the user does not belong to is
/// reported as not existing, not as forbidden: whether it exists is none of
/// their business.
pub async fn member(state: &AppState, user: &CurrentUser, tenant: Uuid) -> AppResult<Member> {
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM memberships WHERE tenant_uuid = $1 AND user_uuid = $2)",
    )
    .bind(tenant)
    .bind(user.uuid)
    .fetch_one(&state.db)
    .await?;

    let tenant_exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tenants WHERE uuid = $1)")
        .bind(tenant)
        .fetch_one(&state.db)
        .await?;
    if !tenant_exists || (!is_member && !user.is_server_admin) {
        return Err(AppError::NotFound);
    }

    let groups: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT g.uuid, g.system FROM group_members m JOIN groups g ON g.uuid = m.group_uuid
          WHERE g.tenant_uuid = $1 AND m.user_uuid = $2",
    )
    .bind(tenant)
    .bind(user.uuid)
    .fetch_all(&state.db)
    .await?;

    Ok(Member {
        is_admin: user.is_server_admin || groups.iter().any(|(_, system)| *system),
        groups: groups.into_iter().map(|(uuid, _)| uuid).collect(),
        user: user.clone(),
        tenant,
    })
}

impl Member {
    pub fn require_admin(&self) -> AppResult<()> {
        if self.is_admin { Ok(()) } else { Err(AppError::Forbidden) }
    }

    /// The highest role this member has on a resource through grants — to
    /// them or to one of their groups — or through being an admin. Owners of
    /// selections and sessions are handled where those are.
    pub async fn role_on(&self, state: &AppState, kind: ResourceKind, resource: Uuid) -> AppResult<Option<Role>> {
        if self.is_admin {
            return Ok(Some(Role::Manage));
        }
        let roles: Vec<String> = sqlx::query_scalar(
            "SELECT role FROM grants
              WHERE tenant_uuid = $1 AND resource_kind = $2 AND resource_uuid = $3
                AND (user_uuid = $4 OR group_uuid = ANY($5))",
        )
        .bind(self.tenant)
        .bind(kind_str(kind))
        .bind(resource)
        .bind(self.user.uuid)
        .bind(&self.groups)
        .fetch_all(&state.db)
        .await?;
        let granted = roles.iter().map(|role| parse_role(role)).max();
        // Every member may use the tenant's designs and slide settings.
        let floor = (kind == ResourceKind::Settings).then_some(Role::Read);
        Ok(granted.max(floor))
    }

    /// Like [`role_on`](Self::role_on), but refuses when the role is below
    /// `needed`. Without any role the resource is reported as not found.
    pub async fn require(&self, state: &AppState, kind: ResourceKind, resource: Uuid, needed: Role) -> AppResult<Role> {
        match self.role_on(state, kind, resource).await? {
            None => Err(AppError::NotFound),
            Some(role) if role < needed => Err(AppError::Forbidden),
            Some(role) => Ok(role),
        }
    }
}

/// Generates a fresh UUID token, for use in places that only need an opaque
/// id the database does not know about.
pub fn new_uuid() -> Uuid {
    Uuid::now_v7()
}
