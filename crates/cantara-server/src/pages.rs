//! The few pages the server draws itself: logging in, registering, accepting
//! an invitation, and allowing a desktop program to act for the user.
//!
//! They are plain HTML forms rather than part of the web build, because they
//! have to work before the web build is loaded and for programs that have no
//! web build at all — the desktop program opens `/authorize` in the system
//! browser. The web build sends people here to log in, too, so there is one
//! login, not two.

use axum::extract::{Form, Path, Query, State};
use axum::http::header::{ACCEPT_LANGUAGE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use cantara_api::{RegisterRequest, DESKTOP_CLIENT_ID};
use chrono::{Duration, Utc};
use serde::Deserialize;

use crate::api::account::{check_login, create_account};
use crate::auth::{create_token, session_cookie, MaybeUser};
use crate::error::AppError;
use crate::invitations;
use crate::secrets::{hash_token, random_token};
use crate::state::AppState;

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    De,
    En,
}

fn lang(headers: &HeaderMap) -> Lang {
    let accept = headers
        .get(ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if accept.trim_start().to_lowercase().starts_with("de") { Lang::De } else { Lang::En }
}

fn t(lang: Lang, de: &'static str, en: &'static str) -> &'static str {
    if lang == Lang::De { de } else { en }
}

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Where to go after logging in, if it is somewhere on this server. Anything
/// else would make the login page a way to send people elsewhere with this
/// server's name on the link.
pub fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(next) if next.starts_with('/') && !next.starts_with("//") && !next.contains('\\') => next.to_string(),
        _ => "/".to_string(),
    }
}

fn page(title: &str, body: &str) -> Response {
    Html(format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Cantara</title>
<style>
:root {{ color-scheme: light dark; --accent: #1e6fb8; }}
body {{ font-family: system-ui, sans-serif; display: grid; place-items: center; min-height: 100vh; margin: 0;
       background: light-dark(#f4f5f7, #16181b); color: light-dark(#1b1d20, #e7e9ec); }}
main {{ width: min(26rem, calc(100vw - 2rem)); background: light-dark(#fff, #212428); padding: 2rem;
       border-radius: 12px; box-shadow: 0 2px 16px rgb(0 0 0 / 0.12); }}
h1 {{ font-size: 1.4rem; margin-top: 0; }}
label {{ display: block; margin: 0.8rem 0 0.3rem; font-size: 0.9rem; }}
input {{ width: 100%; box-sizing: border-box; padding: 0.6rem; border-radius: 6px; font: inherit;
        border: 1px solid light-dark(#c8ccd1, #3a3f45); background: light-dark(#fff, #16181b); color: inherit; }}
button, .button {{ display: block; width: 100%; margin-top: 1.2rem; padding: 0.7rem; border: 0; border-radius: 6px;
        background: var(--accent); color: #fff; font: inherit; cursor: pointer; text-align: center; text-decoration: none; }}
.secondary {{ background: transparent; color: var(--accent); border: 1px solid var(--accent); }}
.error {{ background: light-dark(#fde8e8, #4a1f1f); padding: 0.6rem; border-radius: 6px; }}
.muted {{ opacity: 0.7; font-size: 0.9rem; }}
hr {{ border: 0; border-top: 1px solid light-dark(#e1e4e8, #33373c); margin: 1.5rem 0; }}
</style></head>
<body><main>{body}</main></body></html>"#,
        title = escape(title),
    ))
    .into_response()
}

pub fn error_page(message: &str) -> Response {
    let mut response = page("Fehler", &format!("<h1>Cantara</h1><p class=\"error\">{}</p><a class=\"button\" href=\"/login\">Login</a>", escape(message)));
    *response.status_mut() = StatusCode::BAD_REQUEST;
    response
}

fn redirect(to: &str) -> Response {
    (StatusCode::SEE_OTHER, [(LOCATION, to.to_string())]).into_response()
}

// ── Login ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct NextQuery {
    pub next: Option<String>,
}

fn login_form(state: &AppState, lang: Lang, next: &str, error: Option<&str>) -> Response {
    let mut body = format!("<h1>{}</h1>", escape(&state.config.name));
    if let Some(error) = error {
        body += &format!("<p class=\"error\">{}</p>", escape(error));
    }
    if state.config.password_login {
        body += &format!(
            r#"<form method="post" action="/login">
<input type="hidden" name="next" value="{next}">
<label for="email">{email}</label><input id="email" name="email" type="email" autocomplete="username" required autofocus>
<label for="password">{password}</label><input id="password" name="password" type="password" autocomplete="current-password" required>
<button type="submit">{login}</button></form>"#,
            next = escape(next),
            email = t(lang, "E-Mail-Adresse", "E-mail address"),
            password = t(lang, "Passwort", "Password"),
            login = t(lang, "Anmelden", "Log in"),
        );
    }
    if !state.config.oidc_providers.is_empty() {
        if state.config.password_login {
            body += "<hr>";
        }
        for provider in &state.config.oidc_providers {
            let encoded: String = url::form_urlencoded::byte_serialize(next.as_bytes()).collect();
            body += &format!(
                "<a class=\"button secondary\" href=\"/auth/oidc/{}?next={}\">{} {}</a>",
                escape(&provider.id),
                encoded,
                t(lang, "Anmelden mit", "Log in with"),
                escape(&provider.name),
            );
        }
    }
    if state.config.registration_open && state.config.password_login {
        let encoded: String = url::form_urlencoded::byte_serialize(next.as_bytes()).collect();
        body += &format!(
            "<p class=\"muted\"><a href=\"/register?next={encoded}\">{}</a></p>",
            t(lang, "Noch kein Konto? Registrieren", "No account yet? Register"),
        );
    }
    page(t(lang, "Anmelden", "Log in"), &body)
}

pub async fn login_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: MaybeUser,
    Query(query): Query<NextQuery>,
) -> Response {
    let next = safe_next(query.next.as_deref());
    if user.0.is_some() {
        return redirect(&next);
    }
    login_form(&state, lang(&headers), &next, None)
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    pub next: Option<String>,
}

pub async fn login_submit(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<LoginForm>) -> Response {
    let lang = lang(&headers);
    let next = safe_next(form.next.as_deref());
    match check_login(&state, &form.email, &form.password).await {
        Ok(user) => match create_token(&state, user.uuid, "session", "").await {
            Ok(token) => {
                let mut response = redirect(&next);
                response.headers_mut().insert(SET_COOKIE, session_cookie(&state, &token));
                response
            }
            Err(error) => error_page(&error.to_string()),
        },
        Err(AppError::TooManyRequests) => login_form(
            &state,
            lang,
            &next,
            Some(t(lang, "Zu viele Versuche. Bitte in einer Minute noch einmal.", "Too many attempts. Try again in a minute.")),
        ),
        Err(_) => login_form(
            &state,
            lang,
            &next,
            Some(t(lang, "E-Mail-Adresse oder Passwort stimmen nicht.", "The e-mail address or the password is wrong.")),
        ),
    }
}

// ── Registration ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RegisterQuery {
    pub next: Option<String>,
    pub invitation: Option<String>,
}

fn register_form(lang: Lang, query: &RegisterQuery, email: &str, error: Option<&str>) -> Response {
    let mut body = format!("<h1>{}</h1>", t(lang, "Konto anlegen", "Create an account"));
    if let Some(error) = error {
        body += &format!("<p class=\"error\">{}</p>", escape(error));
    }
    body += &format!(
        r#"<form method="post" action="/register">
<input type="hidden" name="next" value="{next}">
<input type="hidden" name="invitation" value="{invitation}">
<label for="display_name">{name}</label><input id="display_name" name="display_name" required autofocus>
<label for="email">{email_label}</label><input id="email" name="email" type="email" value="{email}" autocomplete="username" required>
<label for="password">{password}</label><input id="password" name="password" type="password" minlength="10" autocomplete="new-password" required>
<p class="muted">{rule}</p>
<button type="submit">{create}</button></form>"#,
        next = escape(&safe_next(query.next.as_deref())),
        invitation = escape(query.invitation.as_deref().unwrap_or_default()),
        name = t(lang, "Name", "Name"),
        email_label = t(lang, "E-Mail-Adresse", "E-mail address"),
        email = escape(email),
        password = t(lang, "Passwort", "Password"),
        rule = t(lang, "Mindestens 10 Zeichen.", "At least 10 characters."),
        create = t(lang, "Konto anlegen", "Create account"),
    );
    page(t(lang, "Konto anlegen", "Create an account"), &body)
}

pub async fn register_page(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<RegisterQuery>) -> Response {
    let lang = lang(&headers);
    let email = match &query.invitation {
        Some(token) => match invitations::open(&state, token).await {
            Ok(invitation) => invitation.email,
            Err(_) => return error_page(t(lang, "Die Einladung ist nicht mehr gültig.", "The invitation is no longer valid.")),
        },
        None if state.config.registration_open => String::new(),
        None => return error_page(t(lang, "Auf diesem Server kann man sich nur mit Einladung registrieren.", "This server only takes new accounts by invitation.")),
    };
    register_form(lang, &query, &email, None)
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub display_name: String,
    pub email: String,
    pub password: String,
    pub next: Option<String>,
    pub invitation: Option<String>,
}

pub async fn register_submit(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<RegisterForm>) -> Response {
    let lang = lang(&headers);
    let invitation = form.invitation.clone().filter(|token| !token.is_empty());
    let request = RegisterRequest {
        email: form.email.clone(),
        password: form.password.clone(),
        display_name: form.display_name.clone(),
        invitation: invitation.clone(),
    };
    match create_account(&state, &request).await {
        Ok(user) => match create_token(&state, user.uuid, "session", "").await {
            Ok(token) => {
                let mut response = redirect(&safe_next(form.next.as_deref()));
                response.headers_mut().insert(SET_COOKIE, session_cookie(&state, &token));
                response
            }
            Err(error) => error_page(&error.to_string()),
        },
        Err(error) => register_form(
            lang,
            &RegisterQuery {
                next: form.next,
                invitation,
            },
            &form.email,
            Some(&error.to_string()),
        ),
    }
}

// ── Invitations ──────────────────────────────────────────────────────────────

pub async fn invite_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: MaybeUser,
    Path(token): Path<String>,
) -> Response {
    let lang = lang(&headers);
    let Ok(invitation) = invitations::open(&state, &token).await else {
        return error_page(t(lang, "Die Einladung ist nicht mehr gültig.", "The invitation is no longer valid."));
    };
    let target = match &invitation.group {
        Some((_, group)) => format!("{} · {}", invitation.tenant.name, group),
        None => invitation.tenant.name.clone(),
    };
    let mut body = format!(
        "<h1>{}</h1><p>{} <strong>{}</strong>.</p>",
        t(lang, "Einladung", "Invitation"),
        t(lang, "Du bist eingeladen zu", "You are invited to"),
        escape(&target),
    );
    let here = format!("/invite/{token}");
    let encoded: String = url::form_urlencoded::byte_serialize(here.as_bytes()).collect();
    match user.0 {
        Some(user) if user.email.eq_ignore_ascii_case(&invitation.email) => {
            body += &format!(
                "<form method=\"post\" action=\"/invite/{}\"><button type=\"submit\">{}</button></form>",
                escape(&token),
                t(lang, "Einladung annehmen", "Accept the invitation"),
            );
        }
        Some(_) => {
            body += &format!(
                "<p class=\"error\">{} {}.</p>",
                t(lang, "Die Einladung gilt für", "The invitation is for"),
                escape(&invitation.email),
            );
        }
        None => {
            body += &format!(
                "<a class=\"button\" href=\"/register?invitation={}&next={encoded}\">{}</a>\
                 <a class=\"button secondary\" href=\"/login?next={encoded}\">{}</a>",
                escape(&token),
                t(lang, "Konto anlegen", "Create an account"),
                t(lang, "Ich habe schon ein Konto", "I already have an account"),
            );
        }
    }
    page(t(lang, "Einladung", "Invitation"), &body)
}

pub async fn invite_accept(State(state): State<AppState>, user: MaybeUser, Path(token): Path<String>) -> Response {
    let Some(user) = user.0 else {
        return redirect(&format!("/login?next=/invite/{token}"));
    };
    match invitations::open(&state, &token).await {
        Ok(invitation) => match invitations::accept(&state, &invitation, &user).await {
            Ok(()) => redirect("/"),
            Err(error) => error_page(&error.to_string()),
        },
        Err(error) => error_page(&error.to_string()),
    }
}

// ── Authorising a desktop program ────────────────────────────────────────────

#[derive(Deserialize, Clone)]
pub struct AuthorizeQuery {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub code_challenge_method: Option<String>,
    pub state: Option<String>,
    pub device_name: Option<String>,
}

/// A desktop program may only be sent back to itself: a port on the loopback
/// interface (RFC 8252, section 7.3).
fn valid_loopback(redirect: &str) -> bool {
    let Ok(url) = url::Url::parse(redirect) else {
        return false;
    };
    url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
        && url.port().is_some()
}

fn check_authorize(query: &AuthorizeQuery) -> Result<(), &'static str> {
    if query.client_id != DESKTOP_CLIENT_ID {
        return Err("unknown client");
    }
    if !valid_loopback(&query.redirect_uri) {
        return Err("the redirect has to go to the loopback interface");
    }
    if query.code_challenge.len() < 43 || query.code_challenge_method.as_deref().unwrap_or("S256") != "S256" {
        return Err("a PKCE challenge (S256) is needed");
    }
    Ok(())
}

pub async fn authorize_page(
    headers: HeaderMap,
    user: MaybeUser,
    Query(query): Query<AuthorizeQuery>,
) -> Response {
    let lang = lang(&headers);
    if let Err(reason) = check_authorize(&query) {
        return error_page(reason);
    }
    let Some(user) = user.0 else {
        let here = format!(
            "/authorize?{}",
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("client_id", &query.client_id)
                .append_pair("redirect_uri", &query.redirect_uri)
                .append_pair("code_challenge", &query.code_challenge)
                .append_pair("code_challenge_method", "S256")
                .append_pair("state", query.state.as_deref().unwrap_or_default())
                .append_pair("device_name", query.device_name.as_deref().unwrap_or_default())
                .finish()
        );
        let encoded: String = url::form_urlencoded::byte_serialize(here.as_bytes()).collect();
        return redirect(&format!("/login?next={encoded}"));
    };

    let device = query.device_name.clone().filter(|name| !name.trim().is_empty()).unwrap_or_else(|| "Cantara".to_string());
    let hidden = |name: &str, value: &str| format!("<input type=\"hidden\" name=\"{name}\" value=\"{}\">", escape(value));
    let body = format!(
        "<h1>{title}</h1><p>{text}</p><p><strong>{device}</strong></p><p class=\"muted\">{as_user} {user}</p>\
         <form method=\"post\" action=\"/authorize\">{fields}<button type=\"submit\">{allow}</button></form>\
         <a class=\"button secondary\" href=\"{deny_url}\">{deny}</a>",
        title = t(lang, "Cantara verbinden", "Connect Cantara"),
        text = t(
            lang,
            "Cantara auf diesem Computer möchte auf die Bibliotheken, Designs und Abläufe zugreifen, die du auf diesem Server sehen darfst:",
            "Cantara on this computer asks to use the libraries, designs and running orders you may see on this server:"
        ),
        device = escape(&device),
        as_user = t(lang, "Angemeldet als", "Logged in as"),
        user = escape(&user.email),
        fields = [
            hidden("client_id", &query.client_id),
            hidden("redirect_uri", &query.redirect_uri),
            hidden("code_challenge", &query.code_challenge),
            hidden("code_challenge_method", "S256"),
            hidden("state", query.state.as_deref().unwrap_or_default()),
            hidden("device_name", &device),
        ]
        .concat(),
        allow = t(lang, "Erlauben", "Allow"),
        deny = t(lang, "Ablehnen", "Deny"),
        deny_url = escape(&with_query(&query.redirect_uri, &[("error", "access_denied"), ("state", query.state.as_deref().unwrap_or_default())])),
    );
    page(t(lang, "Cantara verbinden", "Connect Cantara"), &body)
}

fn with_query(base: &str, pairs: &[(&str, &str)]) -> String {
    match url::Url::parse(base) {
        Ok(mut url) => {
            {
                let mut query = url.query_pairs_mut();
                for (key, value) in pairs {
                    query.append_pair(key, value);
                }
            }
            url.to_string()
        }
        Err(_) => base.to_string(),
    }
}

pub async fn authorize_submit(State(state): State<AppState>, user: MaybeUser, Form(form): Form<AuthorizeQuery>) -> Response {
    if let Err(reason) = check_authorize(&form) {
        return error_page(reason);
    }
    let Some(user) = user.0 else {
        return redirect("/login");
    };
    let code = match random_token() {
        Ok(code) => code,
        Err(error) => return error_page(&error.to_string()),
    };
    let stored = sqlx::query(
        "INSERT INTO auth_codes (code_hash, user_uuid, redirect_uri, code_challenge, expires_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(hash_token(&code))
    .bind(user.uuid)
    .bind(&form.redirect_uri)
    .bind(&form.code_challenge)
    .bind(Utc::now() + Duration::minutes(2))
    .execute(&state.db)
    .await;
    if let Err(error) = stored {
        return error_page(&error.to_string());
    }
    redirect(&with_query(
        &form.redirect_uri,
        &[("code", &code), ("state", form.state.as_deref().unwrap_or_default())],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_stays_on_this_server() {
        assert_eq!(safe_next(Some("/settings")), "/settings");
        assert_eq!(safe_next(Some("//evil.example")), "/");
        assert_eq!(safe_next(Some("https://evil.example")), "/");
        assert_eq!(safe_next(Some("/\\evil.example")), "/");
        assert_eq!(safe_next(None), "/");
    }

    #[test]
    fn only_loopback_redirects_are_accepted() {
        assert!(valid_loopback("http://127.0.0.1:53123/callback"));
        assert!(valid_loopback("http://localhost:4000/"));
        assert!(!valid_loopback("http://127.0.0.1/callback"));
        assert!(!valid_loopback("https://127.0.0.1:4000/"));
        assert!(!valid_loopback("http://evil.example:4000/"));
    }
}
