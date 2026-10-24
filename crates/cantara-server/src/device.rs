//! Logging in a device that has no browser of its own — a tablet on a music
//! stand, a TV behind the stage, a computer whose browser cannot reach back
//! to it (RFC 8628, the device authorization grant).
//!
//! The device asks for a pair of codes (`POST /api/v1/auth/device`) and shows
//! the short one with an address. Somebody opens that address anywhere, logs
//! in, types the code in and says yes (`/device`). The device, which has been
//! asking all along (`POST /api/v1/auth/device/token`), is then given a token
//! like any other program.

use axum::extract::{Form, Query, State};
use axum::http::header::LOCATION;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cantara_api::{
    ApiError, DeviceAuthorization, DeviceAuthorizationRequest, DeviceTokenRequest, TokenResponse,
    DESKTOP_CLIENT_ID,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::account::user_info;
use crate::auth::{create_token, CurrentUser, MaybeUser};
use crate::error::{AppError, AppResult};
use crate::pages::escape;
use crate::secrets::{hash_token, random_token};
use crate::state::AppState;

/// How long a pair of codes is good for.
const VALID_MINUTES: i64 = 10;
/// How often a device may ask, in seconds.
const INTERVAL: i64 = 5;
/// Letters that cannot be mistaken for one another or for digits, as RFC 8628
/// suggests: no vowels (no words), no I/O/0/1.
const ALPHABET: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";

fn user_code() -> AppResult<String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(AppError::internal)?;
    let letters: String = bytes
        .iter()
        .map(|byte| ALPHABET[*byte as usize % ALPHABET.len()] as char)
        .collect();
    Ok(format!("{}-{}", &letters[..4], &letters[4..]))
}

/// What somebody typed, the way the codes are stored: capitals, one dash.
pub fn normalise_user_code(typed: &str) -> String {
    let letters: String = typed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if letters.len() == 8 {
        format!("{}-{}", &letters[..4], &letters[4..])
    } else {
        letters
    }
}

/// A device asks to be logged in.
#[utoipa::path(post, path = "/auth/device", tag = "account", request_body = DeviceAuthorizationRequest,
    responses((status = 200, body = DeviceAuthorization)))]
pub async fn authorize(
    State(state): State<AppState>,
    Json(request): Json<DeviceAuthorizationRequest>,
) -> AppResult<Json<DeviceAuthorization>> {
    if request.client_id != DESKTOP_CLIENT_ID {
        return Err(AppError::BadRequest("unknown client".to_string()));
    }
    sqlx::query("DELETE FROM device_codes WHERE expires_at < now()")
        .execute(&state.db)
        .await?;

    let device_code = random_token()?;
    // A clash of user codes is possible, if unlikely; a second draw settles it.
    let mut code = user_code()?;
    for _ in 0..5 {
        let inserted = sqlx::query(
            "INSERT INTO device_codes (device_code_hash, user_code, device_name, expires_at)
             VALUES ($1, $2, $3, $4) ON CONFLICT (user_code) DO NOTHING",
        )
        .bind(hash_token(&device_code))
        .bind(&code)
        .bind(request.device_name.trim().chars().take(100).collect::<String>())
        .bind(Utc::now() + Duration::minutes(VALID_MINUTES))
        .execute(&state.db)
        .await?;
        if inserted.rows_affected() == 1 {
            let verification_uri = format!("{}/device", state.config.public_url);
            return Ok(Json(DeviceAuthorization {
                device_code,
                verification_uri_complete: format!("{verification_uri}?code={code}"),
                user_code: code,
                verification_uri,
                expires_in: VALID_MINUTES * 60,
                interval: INTERVAL,
            }));
        }
        code = user_code()?;
    }
    Err(AppError::internal("no free user code"))
}

/// The device asks whether it has been let in.
#[utoipa::path(post, path = "/auth/device/token", tag = "account", request_body = DeviceTokenRequest,
    responses((status = 200, body = TokenResponse), (status = 400, body = ApiError)))]
pub async fn token(State(state): State<AppState>, Json(request): Json<DeviceTokenRequest>) -> AppResult<Json<TokenResponse>> {
    let hash = hash_token(&request.device_code);
    let row: Option<(Option<Uuid>, bool, Option<chrono::DateTime<Utc>>, chrono::DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT user_uuid, denied, last_poll, expires_at, device_name FROM device_codes WHERE device_code_hash = $1",
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;
    let Some((user, denied, last_poll, expires_at, device_name)) = row else {
        return Err(AppError::Device("expired_token", "the code is not known or has expired".to_string()));
    };
    let forget = || sqlx::query("DELETE FROM device_codes WHERE device_code_hash = $1").bind(&hash);
    if expires_at < Utc::now() {
        forget().execute(&state.db).await?;
        return Err(AppError::Device("expired_token", "the code has expired".to_string()));
    }
    if denied {
        forget().execute(&state.db).await?;
        return Err(AppError::Device("access_denied", "the login was declined".to_string()));
    }
    let Some(user) = user else {
        if last_poll.is_some_and(|last| Utc::now() - last < Duration::seconds(INTERVAL)) {
            sqlx::query("UPDATE device_codes SET last_poll = now() WHERE device_code_hash = $1")
                .bind(&hash)
                .execute(&state.db)
                .await?;
            return Err(AppError::Device("slow_down", "asking too often".to_string()));
        }
        sqlx::query("UPDATE device_codes SET last_poll = now() WHERE device_code_hash = $1")
            .bind(&hash)
            .execute(&state.db)
            .await?;
        return Err(AppError::Device("authorization_pending", "waiting for the user".to_string()));
    };
    forget().execute(&state.db).await?;

    let (email, display_name, is_server_admin): (String, String, bool) =
        sqlx::query_as("SELECT email, display_name, is_server_admin FROM users WHERE uuid = $1 AND NOT disabled")
            .bind(user)
            .fetch_one(&state.db)
            .await?;
    let user = CurrentUser {
        uuid: user,
        email,
        display_name,
        is_server_admin,
    };
    let name = if device_name.is_empty() { "Cantara".to_string() } else { device_name };
    let access_token = create_token(&state, user.uuid, "api", &name).await?;
    Ok(Json(TokenResponse {
        access_token,
        user: user_info(&user),
    }))
}

// ── The page the code is typed into ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct CodeQuery {
    pub code: Option<String>,
}

fn page(body: &str) -> Response {
    crate::pages::plain_page("Gerät verbinden / Connect a device", body)
}

pub async fn device_page(user: MaybeUser, Query(query): Query<CodeQuery>) -> Response {
    let code = query.code.unwrap_or_default();
    if user.0.is_none() {
        let next = format!("/device?code={code}");
        let encoded: String = url::form_urlencoded::byte_serialize(next.as_bytes()).collect();
        return (StatusCode::SEE_OTHER, [(LOCATION, format!("/login?next={encoded}"))]).into_response();
    }
    page(&format!(
        r#"<h1>Gerät verbinden</h1>
<p>Gib den Code ein, den das Gerät anzeigt. / Enter the code the device shows.</p>
<form method="post" action="/device">
<label for="user_code">Code</label>
<input id="user_code" name="user_code" value="{}" autocomplete="off" autocapitalize="characters" required autofocus>
<button type="submit" name="decision" value="allow">Erlauben / Allow</button>
<button type="submit" name="decision" value="deny" class="secondary">Ablehnen / Deny</button>
</form>"#,
        escape(&code)
    ))
}

#[derive(Deserialize)]
pub struct DecisionForm {
    pub user_code: String,
    pub decision: String,
}

pub async fn device_submit(State(state): State<AppState>, user: MaybeUser, Form(form): Form<DecisionForm>) -> Response {
    let Some(user) = user.0 else {
        return (StatusCode::SEE_OTHER, [(LOCATION, "/login?next=/device".to_string())]).into_response();
    };
    let code = normalise_user_code(&form.user_code);
    let allow = form.decision == "allow";
    let updated = sqlx::query(
        "UPDATE device_codes SET user_uuid = CASE WHEN $2 THEN $3 ELSE NULL END, denied = NOT $2
          WHERE user_code = $1 AND expires_at > now() AND user_uuid IS NULL AND NOT denied",
    )
    .bind(&code)
    .bind(allow)
    .bind(user.uuid)
    .execute(&state.db)
    .await;
    match updated {
        Ok(result) if result.rows_affected() == 1 => page(if allow {
            "<h1>Verbunden</h1><p>Das Gerät ist jetzt angemeldet. / The device is now logged in.</p>"
        } else {
            "<h1>Abgelehnt</h1><p>Das Gerät wurde nicht angemeldet. / The device was not logged in.</p>"
        }),
        Ok(_) => page(&format!(
            "<h1>Unbekannter Code</h1><p class=\"error\">Den Code {} gibt es nicht (mehr). / That code is not known or has expired.</p>\
             <a class=\"button\" href=\"/device\">Noch einmal / Again</a>",
            escape(&code)
        )),
        Err(error) => crate::pages::error_page(&error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_codes_are_easy_to_type_and_read_back_however_typed() {
        let code = user_code().unwrap();
        assert_eq!(code.len(), 9);
        assert!(code.chars().all(|c| c == '-' || ALPHABET.contains(&(c as u8))));
        assert_eq!(normalise_user_code("wdjb mjht"), "WDJB-MJHT");
        assert_eq!(normalise_user_code("WDJB-MJHT"), "WDJB-MJHT");
    }
}
