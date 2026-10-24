//! Logging in through an OpenID Connect provider.
//!
//! `/auth/oidc/{provider}` sends the browser to the provider with a fresh
//! state, nonce and PKCE challenge, remembered in `oidc_states`;
//! `/auth/oidc/callback` takes the code back, checks the ID token and finds or
//! makes the account. An account is found by the provider's subject, or —
//! the first time — by a verified e-mail address the server knows; a new one
//! is only made where registration is open or an invitation for that address
//! is waiting.

use std::collections::HashMap;
use std::sync::Mutex;

use axum::extract::{Path, Query, State};
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{Duration, Utc};
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{create_token, session_cookie, CurrentUser};
use crate::config::OidcProvider;
use crate::error::{AppError, AppResult};
use crate::pages;
use crate::state::AppState;

/// The providers, and what each one said about itself the first time it was
/// asked — discovery is a network round trip, and its answer rarely changes.
pub struct OidcClients {
    providers: Vec<OidcProvider>,
    metadata: Mutex<HashMap<String, CoreProviderMetadata>>,
}

impl OidcClients {
    pub fn new(providers: Vec<OidcProvider>) -> OidcClients {
        OidcClients {
            providers,
            metadata: Mutex::new(HashMap::new()),
        }
    }
}

fn http_client() -> AppResult<openidconnect::reqwest::Client> {
    openidconnect::reqwest::ClientBuilder::new()
        // Following redirects would let a provider point the server anywhere.
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
        .map_err(AppError::internal)
}

fn redirect_url(state: &AppState) -> AppResult<RedirectUrl> {
    RedirectUrl::new(format!("{}/auth/oidc/callback", state.config.public_url)).map_err(AppError::internal)
}

async fn metadata(state: &AppState, provider: &OidcProvider) -> AppResult<CoreProviderMetadata> {
    if let Some(found) = state.oidc.metadata.lock().map_err(AppError::internal)?.get(&provider.id) {
        return Ok(found.clone());
    }
    let issuer = IssuerUrl::new(provider.issuer.clone()).map_err(AppError::internal)?;
    let discovered = CoreProviderMetadata::discover_async(issuer, &http_client()?)
        .await
        .map_err(|error| AppError::internal(format!("discovering {}: {error}", provider.issuer)))?;
    state
        .oidc
        .metadata
        .lock()
        .map_err(AppError::internal)?
        .insert(provider.id.clone(), discovered.clone());
    Ok(discovered)
}

fn provider(state: &AppState, id: &str) -> AppResult<OidcProvider> {
    state
        .oidc
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .cloned()
        .ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
pub struct StartQuery {
    pub next: Option<String>,
}

/// Sends the browser to the provider.
pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<StartQuery>,
) -> AppResult<Response> {
    let provider = provider(&state, &id)?;
    let metadata = metadata(&state, &provider).await?;
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(provider.client_id.clone()),
        provider.client_secret.clone().map(ClientSecret::new),
    )
    .set_redirect_uri(redirect_url(&state)?);

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, csrf, nonce) = client
        .authorize_url(CoreAuthenticationFlow::AuthorizationCode, CsrfToken::new_random, Nonce::new_random)
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(challenge)
        .url();

    sqlx::query("DELETE FROM oidc_states WHERE expires_at < now()")
        .execute(&state.db)
        .await?;
    sqlx::query(
        "INSERT INTO oidc_states (state, provider, nonce, pkce_verifier, next, expires_at) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(csrf.secret())
    .bind(&provider.id)
    .bind(nonce.secret())
    .bind(verifier.secret())
    .bind(pages::safe_next(query.next.as_deref()))
    .bind(Utc::now() + Duration::minutes(10))
    .execute(&state.db)
    .await?;

    Ok((StatusCode::SEE_OTHER, [(LOCATION, url.to_string())]).into_response())
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Where the provider sends the browser back to.
pub async fn callback(State(state): State<AppState>, Query(query): Query<CallbackQuery>) -> Response {
    match finish(&state, query).await {
        Ok((user, next)) => match create_token(&state, user.uuid, "session", "").await {
            Ok(token) => (
                StatusCode::SEE_OTHER,
                [(LOCATION, next), (SET_COOKIE, session_cookie(&state, &token).to_str().unwrap_or_default().to_string())],
            )
                .into_response(),
            Err(error) => pages::error_page(&error.to_string()),
        },
        Err(error) => pages::error_page(&match error {
            AppError::Forbidden => "Für dieses Konto gibt es auf diesem Server keinen Zugang. \
                                    Bitte lass dich einladen. / There is no access to this server for that account."
                .to_string(),
            other => other.to_string(),
        }),
    }
}

async fn finish(state: &AppState, query: CallbackQuery) -> AppResult<(CurrentUser, String)> {
    if let Some(error) = query.error {
        return Err(AppError::BadRequest(format!("the provider refused: {error}")));
    }
    let (code, csrf) = query
        .code
        .zip(query.state)
        .ok_or_else(|| AppError::BadRequest("the provider sent no code".to_string()))?;
    let (provider_id, nonce, verifier, next): (String, String, String, String) = sqlx::query_as(
        "DELETE FROM oidc_states WHERE state = $1 AND expires_at > now() RETURNING provider, nonce, pkce_verifier, next",
    )
    .bind(&csrf)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::BadRequest("the login took too long or was not started here".to_string()))?;

    let provider = provider(state, &provider_id)?;
    let metadata = metadata(state, &provider).await?;
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(provider.client_id.clone()),
        provider.client_secret.clone().map(ClientSecret::new),
    )
    .set_redirect_uri(redirect_url(state)?);

    let http = http_client()?;
    let response = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(AppError::internal)?
        .set_pkce_verifier(PkceCodeVerifier::new(verifier))
        .request_async(&http)
        .await
        .map_err(|error| AppError::BadRequest(format!("the provider did not accept the code: {error}")))?;
    let id_token = response
        .id_token()
        .ok_or_else(|| AppError::BadRequest("the provider sent no ID token".to_string()))?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &Nonce::new(nonce))
        .map_err(|error| AppError::BadRequest(format!("the ID token is not valid: {error}")))?;

    let subject = claims.subject().as_str().to_string();
    let email = claims.email().map(|email| email.as_str().to_string());
    let verified = claims.email_verified().unwrap_or(false);
    let name = claims
        .name()
        .and_then(|name| name.get(None))
        .map(|name| name.as_str().to_string())
        .or_else(|| email.clone())
        .unwrap_or_else(|| "Unbenannt".to_string());

    let user = find_or_create(state, &provider.id, &subject, email.as_deref(), verified, &name).await?;
    Ok((user, next))
}

async fn find_or_create(
    state: &AppState,
    provider: &str,
    subject: &str,
    email: Option<&str>,
    verified: bool,
    name: &str,
) -> AppResult<CurrentUser> {
    type UserRow = (Uuid, String, String, bool);
    let known: Option<UserRow> = sqlx::query_as(
        "SELECT u.uuid, u.email, u.display_name, u.is_server_admin FROM oidc_identities i
           JOIN users u ON u.uuid = i.user_uuid
          WHERE i.provider = $1 AND i.subject = $2 AND NOT u.disabled",
    )
    .bind(provider)
    .bind(subject)
    .fetch_optional(&state.db)
    .await?;
    let to_user = |(uuid, email, display_name, is_server_admin): UserRow| CurrentUser {
        uuid,
        email,
        display_name,
        is_server_admin,
    };
    if let Some(row) = known {
        return Ok(to_user(row));
    }

    let email = email.filter(|_| verified).ok_or(AppError::Forbidden)?;
    let invitation: Option<String> = sqlx::query_scalar(
        "SELECT uuid::text FROM invitations WHERE lower(email) = lower($1) AND accepted_at IS NULL AND expires_at > now()
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(email)
    .fetch_optional(&state.db)
    .await?;

    let existing: Option<UserRow> = sqlx::query_as(
        "SELECT uuid, email, display_name, is_server_admin FROM users WHERE lower(email) = lower($1) AND NOT disabled",
    )
    .bind(email)
    .fetch_optional(&state.db)
    .await?;

    let user = match existing {
        // An account that exists is joined to the provider by a verified
        // address — the bootstrap administrator logging in through the
        // organisation's provider for the first time, for one.
        Some(row) => to_user(row),
        None => {
            if invitation.is_none() && !state.config.registration_open {
                return Err(AppError::Forbidden);
            }
            let user = CurrentUser {
                uuid: Uuid::now_v7(),
                email: email.to_string(),
                display_name: name.to_string(),
                is_server_admin: false,
            };
            sqlx::query("INSERT INTO users (uuid, email, display_name) VALUES ($1, $2, $3)")
                .bind(user.uuid)
                .bind(&user.email)
                .bind(&user.display_name)
                .execute(&state.db)
                .await?;
            crate::api::account::admit(state, &user, None).await?;
            user
        }
    };

    sqlx::query("INSERT INTO oidc_identities (uuid, user_uuid, provider, subject) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(user.uuid)
        .bind(provider)
        .bind(subject)
        .execute(&state.db)
        .await?;

    // Invitations waiting for this address are taken up at once.
    let tokens: Vec<Uuid> = sqlx::query_scalar(
        "SELECT uuid FROM invitations WHERE lower(email) = lower($1) AND accepted_at IS NULL AND expires_at > now()",
    )
    .bind(email)
    .fetch_all(&state.db)
    .await?;
    for invitation in tokens {
        accept_by_uuid(state, invitation, &user).await?;
    }
    Ok(user)
}

async fn accept_by_uuid(state: &AppState, invitation: Uuid, user: &CurrentUser) -> AppResult<()> {
    let (tenant, group, role): (Uuid, Option<Uuid>, String) =
        sqlx::query_as("SELECT tenant_uuid, group_uuid, role FROM invitations WHERE uuid = $1")
            .bind(invitation)
            .fetch_one(&state.db)
            .await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO memberships (tenant_uuid, user_uuid) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(tenant)
        .bind(user.uuid)
        .execute(&mut *tx)
        .await?;
    if let Some(group) = group {
        sqlx::query("INSERT INTO group_members (group_uuid, user_uuid, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(group)
            .bind(user.uuid)
            .bind(role)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE invitations SET accepted_at = now() WHERE uuid = $1")
        .bind(invitation)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
