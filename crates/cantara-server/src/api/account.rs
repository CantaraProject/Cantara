//! Logging in and out, the account itself, and the tokens programs hold.

use axum::extract::{Path, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use cantara_api::{
    ApiError, ApiTokenInfo, GroupRef, GroupRole, Me, OidcProviderInfo, RegisterRequest, ServerInfo,
    TenantInfo, TenantMembership, TokenRequest, TokenResponse, UserInfo, SESSION_COOKIE, LoginRequest,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{
    clear_cookie, cookie_value, create_token, session_cookie, CurrentUser, MaybeUser,
};
use crate::bootstrap::default_tenant;
use crate::error::{AppError, AppResult};
use crate::invitations;
use crate::secrets::{check_password_rules, hash_password, hash_token, pkce_matches, verify_password};
use crate::state::AppState;

pub fn user_info(user: &CurrentUser) -> UserInfo {
    UserInfo {
        uuid: user.uuid,
        email: user.email.clone(),
        display_name: user.display_name.clone(),
        is_server_admin: user.is_server_admin,
    }
}

/// Everything `GET /me` says.
pub async fn load_me(state: &AppState, user: &CurrentUser) -> AppResult<Me> {
    let tenants: Vec<(Uuid, String, String)> = if user.is_server_admin {
        sqlx::query_as("SELECT uuid, name, slug FROM tenants ORDER BY name")
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as(
            "SELECT t.uuid, t.name, t.slug FROM tenants t JOIN memberships m ON m.tenant_uuid = t.uuid
              WHERE m.user_uuid = $1 ORDER BY t.name",
        )
        .bind(user.uuid)
        .fetch_all(&state.db)
        .await?
    };

    let groups: Vec<(Uuid, Uuid, String, bool, String)> = sqlx::query_as(
        "SELECT g.tenant_uuid, g.uuid, g.name, g.system, m.role FROM groups g
           JOIN group_members m ON m.group_uuid = g.uuid
          WHERE m.user_uuid = $1 ORDER BY g.name",
    )
    .bind(user.uuid)
    .fetch_all(&state.db)
    .await?;

    let tenants = tenants
        .into_iter()
        .map(|(uuid, name, slug)| {
            let mine: Vec<_> = groups.iter().filter(|group| group.0 == uuid).collect();
            TenantMembership {
                tenant: TenantInfo { uuid, name, slug },
                is_admin: user.is_server_admin || mine.iter().any(|group| group.3),
                groups: mine
                    .iter()
                    .map(|group| GroupRef {
                        uuid: group.1,
                        name: group.2.clone(),
                        role: parse_group_role(&group.4),
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(Me {
        user: user_info(user),
        tenants,
    })
}

pub fn parse_group_role(value: &str) -> GroupRole {
    if value == "manager" { GroupRole::Manager } else { GroupRole::Member }
}

pub fn group_role_str(role: GroupRole) -> &'static str {
    match role {
        GroupRole::Member => "member",
        GroupRole::Manager => "manager",
    }
}

/// What anybody may know about this server.
#[utoipa::path(get, path = "/info", tag = "account", responses((status = 200, body = ServerInfo)))]
pub async fn info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json(ServerInfo {
        name: state.config.name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        single_tenant: state.config.single_tenant,
        registration_open: state.config.registration_open,
        password_login: state.config.password_login,
        sharing_enabled: state.config.sharing_enabled,
        oidc_providers: crate::pages::all_providers(&state)
            .await
            .into_iter()
            .map(|(id, name)| OidcProviderInfo { id, name })
            .collect(),
    })
}

/// Checks an e-mail address and a password. Shared by the API and the login
/// page.
pub async fn check_login(state: &AppState, email: &str, password: &str) -> AppResult<CurrentUser> {
    if !state.config.password_login {
        return Err(AppError::Forbidden);
    }
    let key = format!("login:{}", email.trim().to_lowercase());
    if !state.limiter.allows(&key) {
        return Err(AppError::TooManyRequests);
    }

    let row: Option<(Uuid, String, String, bool, Option<String>)> = sqlx::query_as(
        "SELECT uuid, email, display_name, is_server_admin, password_hash FROM users
          WHERE lower(email) = lower($1) AND NOT disabled",
    )
    .bind(email.trim())
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some((uuid, email, display_name, is_server_admin, Some(hash))) if verify_password(password, &hash) => {
            state.limiter.succeeded(&key);
            Ok(CurrentUser {
                uuid,
                email,
                display_name,
                is_server_admin,
            })
        }
        _ => {
            state.limiter.failed(&key);
            Err(AppError::Unauthorized)
        }
    }
}

fn with_cookie(state: &AppState, token: &str, body: impl IntoResponse) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(SET_COOKIE, session_cookie(state, token));
    response
}

/// Logs in with a password; the answer sets the session cookie.
#[utoipa::path(post, path = "/auth/login", tag = "account", request_body = LoginRequest,
    responses((status = 200, body = Me), (status = 401, body = ApiError)))]
pub async fn login(State(state): State<AppState>, Json(request): Json<LoginRequest>) -> AppResult<Response> {
    let user = check_login(&state, &request.email, &request.password).await?;
    let token = create_token(&state, user.uuid, "session", "").await?;
    Ok(with_cookie(&state, &token, Json(load_me(&state, &user).await?)))
}

/// Creates an account and the user who owns it. Shared by the API and the
/// registration page.
pub async fn create_account(state: &AppState, request: &RegisterRequest) -> AppResult<CurrentUser> {
    let email = request.email.trim();
    if !email.contains('@') {
        return Err(AppError::BadRequest("that is not an e-mail address".to_string()));
    }
    if request.display_name.trim().is_empty() {
        return Err(AppError::BadRequest("a name is needed".to_string()));
    }
    check_password_rules(&request.password)?;

    let invitation = match &request.invitation {
        Some(token) => Some(invitations::open(state, token).await?),
        None => None,
    };
    if let Some(invitation) = &invitation {
        if !invitation.email.eq_ignore_ascii_case(email) {
            return Err(AppError::BadRequest(
                "the invitation is for another e-mail address".to_string(),
            ));
        }
    } else if !state.config.registration_open {
        return Err(AppError::Forbidden);
    }

    let user = CurrentUser {
        uuid: Uuid::now_v7(),
        email: email.to_string(),
        display_name: request.display_name.trim().to_string(),
        is_server_admin: false,
    };
    sqlx::query("INSERT INTO users (uuid, email, display_name, password_hash) VALUES ($1, $2, $3, $4)")
        .bind(user.uuid)
        .bind(&user.email)
        .bind(&user.display_name)
        .bind(hash_password(&request.password)?)
        .execute(&state.db)
        .await
        .map_err(|error| match AppError::from(error) {
            AppError::Conflict(_) => AppError::Conflict("an account with that address exists".to_string()),
            other => other,
        })?;

    admit(state, &user, invitation.as_ref()).await?;
    Ok(user)
}

/// Lets a new account in wherever it was invited to — or, on a server with
/// one tenant, into that one.
pub async fn admit(state: &AppState, user: &CurrentUser, invitation: Option<&invitations::Open>) -> AppResult<()> {
    if let Some(invitation) = invitation {
        invitations::accept(state, invitation, user).await
    } else if state.config.single_tenant {
        if let Some(tenant) = default_tenant(&state.db).await? {
            sqlx::query("INSERT INTO memberships (tenant_uuid, user_uuid) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(tenant)
                .bind(user.uuid)
                .execute(&state.db)
                .await?;
        }
        Ok(())
    } else {
        Ok(())
    }
}

/// Creates an account. Open to anybody where registration is open, and to
/// the holder of an invitation everywhere.
#[utoipa::path(post, path = "/auth/register", tag = "account", request_body = RegisterRequest,
    responses((status = 200, body = Me), (status = 403, body = ApiError)))]
pub async fn register(State(state): State<AppState>, Json(request): Json<RegisterRequest>) -> AppResult<Response> {
    let user = create_account(&state, &request).await?;
    let token = create_token(&state, user.uuid, "session", "").await?;
    Ok(with_cookie(&state, &token, Json(load_me(&state, &user).await?)))
}

/// Ends the browser session.
#[utoipa::path(post, path = "/auth/logout", tag = "account", responses((status = 204)))]
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE) {
        sqlx::query("DELETE FROM auth_tokens WHERE token_hash = $1 AND kind = 'session'")
            .bind(hash_token(&token))
            .execute(&state.db)
            .await?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(SET_COOKIE, clear_cookie());
    Ok(response)
}

/// Who is logged in.
#[utoipa::path(get, path = "/me", tag = "account",
    responses((status = 200, body = Me), (status = 401, body = ApiError)))]
pub async fn me(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Me>> {
    Ok(Json(load_me(&state, &user).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateMe {
    pub display_name: Option<String>,
    pub password: Option<String>,
    /// Needed to change the password of an account that has one.
    pub current_password: Option<String>,
}

/// Changes the user's name or password.
#[utoipa::path(patch, path = "/me", tag = "account", request_body = UpdateMe,
    responses((status = 200, body = UserInfo)))]
pub async fn update_me(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(update): Json<UpdateMe>,
) -> AppResult<Json<UserInfo>> {
    let mut user = user;
    if let Some(name) = update.display_name.map(|name| name.trim().to_string()) {
        if name.is_empty() {
            return Err(AppError::BadRequest("a name is needed".to_string()));
        }
        sqlx::query("UPDATE users SET display_name = $2 WHERE uuid = $1")
            .bind(user.uuid)
            .bind(&name)
            .execute(&state.db)
            .await?;
        user.display_name = name;
    }
    if let Some(password) = update.password {
        check_password_rules(&password)?;
        let stored: Option<String> = sqlx::query_scalar("SELECT password_hash FROM users WHERE uuid = $1")
            .bind(user.uuid)
            .fetch_one(&state.db)
            .await?;
        if let Some(stored) = stored {
            let current = update.current_password.unwrap_or_default();
            if !verify_password(&current, &stored) {
                return Err(AppError::Forbidden);
            }
        }
        sqlx::query("UPDATE users SET password_hash = $2 WHERE uuid = $1")
            .bind(user.uuid)
            .bind(hash_password(&password)?)
            .execute(&state.db)
            .await?;
    }
    Ok(Json(user_info(&user)))
}

/// Everything the server holds about the user, for taking it elsewhere.
#[utoipa::path(get, path = "/me/export", tag = "account", responses((status = 200, body = Object)))]
pub async fn export_me(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<serde_json::Value>> {
    let me = load_me(&state, &user).await?;
    let tokens = list_tokens(&state, &user).await?;
    let selections: Vec<(Uuid, String, serde_json::Value)> =
        sqlx::query_as("SELECT uuid, name, manifest FROM selections WHERE owner_uuid = $1")
            .bind(user.uuid)
            .fetch_all(&state.db)
            .await?;
    let identities: Vec<(String, String)> =
        sqlx::query_as("SELECT provider, subject FROM oidc_identities WHERE user_uuid = $1")
            .bind(user.uuid)
            .fetch_all(&state.db)
            .await?;
    Ok(Json(serde_json::json!({
        "account": me,
        "tokens": tokens,
        "identities": identities,
        "selections": selections
            .into_iter()
            .map(|(uuid, name, manifest)| serde_json::json!({ "uuid": uuid, "name": name, "manifest": manifest }))
            .collect::<Vec<_>>(),
    })))
}

/// Deletes the account. What the user stored stays with the organisation it
/// belongs to, without their name on it.
#[utoipa::path(delete, path = "/me", tag = "account", responses((status = 204)))]
pub async fn delete_me(State(state): State<AppState>, user: CurrentUser) -> AppResult<Response> {
    sqlx::query("DELETE FROM users WHERE uuid = $1")
        .bind(user.uuid)
        .execute(&state.db)
        .await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(SET_COOKIE, clear_cookie());
    Ok(response)
}

async fn list_tokens(state: &AppState, user: &CurrentUser) -> AppResult<Vec<ApiTokenInfo>> {
    let rows: Vec<(Uuid, String, DateTime<Utc>, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT uuid, name, created_at, last_used_at, expires_at FROM auth_tokens
          WHERE user_uuid = $1 AND kind = 'api' ORDER BY created_at DESC",
    )
    .bind(user.uuid)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(uuid, name, created_at, last_used_at, expires_at)| ApiTokenInfo {
            uuid,
            name,
            created_at,
            last_used_at,
            expires_at,
        })
        .collect())
}

/// The tokens the user has given to programs.
#[utoipa::path(get, path = "/me/tokens", tag = "account", responses((status = 200, body = [ApiTokenInfo])))]
pub async fn tokens(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<ApiTokenInfo>>> {
    Ok(Json(list_tokens(&state, &user).await?))
}

/// Takes a token back. The program holding it is logged out.
#[utoipa::path(delete, path = "/me/tokens/{token}", tag = "account", responses((status = 204)),
    params(("token" = Uuid, Path)))]
pub async fn revoke_token(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(token): Path<Uuid>,
) -> AppResult<StatusCode> {
    let deleted = sqlx::query("DELETE FROM auth_tokens WHERE uuid = $1 AND user_uuid = $2 AND kind = 'api'")
        .bind(token)
        .bind(user.uuid)
        .execute(&state.db)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The end of the authorisation flow: a desktop program exchanges the code it
/// was redirected with, and the verifier only it knows, for a token.
#[utoipa::path(post, path = "/auth/token", tag = "account", request_body = TokenRequest,
    responses((status = 200, body = TokenResponse), (status = 400, body = ApiError)))]
pub async fn token(State(state): State<AppState>, Json(request): Json<TokenRequest>) -> AppResult<Json<TokenResponse>> {
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "DELETE FROM auth_codes WHERE code_hash = $1 AND expires_at > now()
         RETURNING user_uuid, redirect_uri, code_challenge",
    )
    .bind(hash_token(&request.code))
    .fetch_optional(&state.db)
    .await?;

    let invalid = || AppError::BadRequest("the code is not valid".to_string());
    let (user_uuid, redirect_uri, challenge) = row.ok_or_else(invalid)?;
    if redirect_uri != request.redirect_uri || !pkce_matches(&request.code_verifier, &challenge) {
        return Err(invalid());
    }

    let (email, display_name, is_server_admin): (String, String, bool) =
        sqlx::query_as("SELECT email, display_name, is_server_admin FROM users WHERE uuid = $1 AND NOT disabled")
            .bind(user_uuid)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(invalid)?;
    let user = CurrentUser {
        uuid: user_uuid,
        email,
        display_name,
        is_server_admin,
    };
    let name = if request.device_name.trim().is_empty() {
        "Cantara".to_string()
    } else {
        request.device_name.trim().chars().take(100).collect()
    };
    let access_token = create_token(&state, user.uuid, "api", &name).await?;
    Ok(Json(TokenResponse {
        access_token,
        user: user_info(&user),
    }))
}

/// The user behind a request that may or may not be logged in.
pub fn optional(user: MaybeUser) -> Option<CurrentUser> {
    user.0
}
