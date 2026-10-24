//! Presentations running on the server, shared by link or with members.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::header::ORIGIN;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use cantara_api::{
    CreateSession, ResourceKind, Role, SessionAccess, SessionClientMessage, SessionInfo, SessionLink,
    SessionServerMessage, UserRef,
};
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::grants::{require, resource_role};
use crate::auth::{member, origin_of, CurrentUser, MaybeUser};
use crate::error::{AppError, AppResult};
use crate::live::{Event, Live};
use crate::secrets::random_token;
use crate::state::AppState;

type SessionRow = (
    Uuid,
    String,
    Option<Uuid>,
    Option<String>,
    DateTime<Utc>,
    i64,
    bool,
    bool,
    String,
    String,
);

const SESSION_QUERY: &str = "SELECT s.uuid, s.name, u.uuid, u.display_name, s.created_at, s.revision,
            s.state IS NOT NULL, s.anonymous_control, s.watch_token, s.control_token
       FROM sessions s LEFT JOIN users u ON u.uuid = s.owner_uuid
      WHERE s.tenant_uuid = $1 AND s.ended_at IS NULL";

fn link(state: &AppState, token: &str) -> String {
    format!("{}/s/{token}", state.config.public_url)
}

fn info(state: &AppState, row: SessionRow, role: Role) -> SessionInfo {
    let (uuid, name, owner, owner_name, created_at, revision, running, anonymous_control, watch, control) = row;
    let shareable = role == Role::Manage;
    SessionInfo {
        uuid,
        name,
        owner: UserRef {
            uuid: owner.unwrap_or_default(),
            display_name: owner_name.unwrap_or_default(),
        },
        created_at,
        revision,
        running,
        anonymous_control,
        viewers: state.live.viewers(uuid),
        role,
        watch_link: shareable.then(|| link(state, &watch)),
        control_link: shareable.then(|| link(state, &control)),
    }
}

async fn session_info(state: &AppState, tenant: Uuid, session: Uuid, role: Role) -> AppResult<SessionInfo> {
    let row: SessionRow = sqlx::query_as(sqlx::AssertSqlSafe(format!("{SESSION_QUERY} AND s.uuid = $2")))
        .bind(tenant)
        .bind(session)
        .fetch_one(&state.db)
        .await?;
    Ok(info(state, row, role))
}

/// The presentations running in the tenant that the user may watch.
#[utoipa::path(get, path = "/tenants/{tenant}/sessions", tag = "sessions",
    params(("tenant" = Uuid, Path)), responses((status = 200, body = [SessionInfo])))]
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
) -> AppResult<Json<Vec<SessionInfo>>> {
    let member = member(&state, &user, tenant).await?;
    let rows: Vec<SessionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!("{SESSION_QUERY} ORDER BY s.created_at DESC")))
        .bind(tenant)
        .fetch_all(&state.db)
        .await?;
    let mut sessions = Vec::new();
    for row in rows {
        if let Some(role) = resource_role(&state, &member, ResourceKind::Session, row.0).await? {
            sessions.push(info(&state, row, role));
        }
    }
    Ok(Json(sessions))
}

/// Opens a session to present into. Whoever opens it owns it.
#[utoipa::path(post, path = "/tenants/{tenant}/sessions", tag = "sessions", request_body = CreateSession,
    params(("tenant" = Uuid, Path)), responses((status = 200, body = SessionInfo)))]
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(tenant): Path<Uuid>,
    Json(request): Json<CreateSession>,
) -> AppResult<Json<SessionInfo>> {
    member(&state, &user, tenant).await?;
    if !state.config.sharing_enabled {
        return Err(AppError::Forbidden);
    }
    let name = if request.name.trim().is_empty() { "Präsentation" } else { request.name.trim() };
    let uuid = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sessions (uuid, tenant_uuid, owner_uuid, name, anonymous_control, watch_token, control_token)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(uuid)
    .bind(tenant)
    .bind(user.uuid)
    .bind(name)
    .bind(request.anonymous_control)
    .bind(random_token()?)
    .bind(random_token()?)
    .execute(&state.db)
    .await?;
    Ok(Json(session_info(&state, tenant, uuid, Role::Manage).await?))
}

/// One session.
#[utoipa::path(get, path = "/tenants/{tenant}/sessions/{session}", tag = "sessions",
    params(("tenant" = Uuid, Path), ("session" = Uuid, Path)), responses((status = 200, body = SessionInfo)))]
pub async fn get(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, session)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<SessionInfo>> {
    let member = member(&state, &user, tenant).await?;
    let role = require(&state, &member, ResourceKind::Session, session, Role::Read).await?;
    Ok(Json(session_info(&state, tenant, session, role).await?))
}

/// Replaces both links; whoever held the old ones is shut out from the next
/// time they connect.
#[utoipa::path(post, path = "/tenants/{tenant}/sessions/{session}/links", tag = "sessions",
    params(("tenant" = Uuid, Path), ("session" = Uuid, Path)), responses((status = 200, body = SessionInfo)))]
pub async fn renew_links(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, session)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<SessionInfo>> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Session, session, Role::Manage).await?;
    sqlx::query("UPDATE sessions SET watch_token = $3, control_token = $4 WHERE uuid = $1 AND tenant_uuid = $2")
        .bind(session)
        .bind(tenant)
        .bind(random_token()?)
        .bind(random_token()?)
        .execute(&state.db)
        .await?;
    Ok(Json(session_info(&state, tenant, session, Role::Manage).await?))
}

/// Ends the presentation for everybody.
#[utoipa::path(delete, path = "/tenants/{tenant}/sessions/{session}", tag = "sessions",
    params(("tenant" = Uuid, Path), ("session" = Uuid, Path)), responses((status = 204)))]
pub async fn end(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((tenant, session)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let member = member(&state, &user, tenant).await?;
    require(&state, &member, ResourceKind::Session, session, Role::Manage).await?;
    state.live.end(&state.db, session).await?;
    Ok(StatusCode::NO_CONTENT)
}

struct LinkTarget {
    session: Uuid,
    name: String,
    access: SessionAccess,
    anonymous_control: bool,
}

async fn resolve_link(state: &AppState, token: &str) -> AppResult<LinkTarget> {
    let row: Option<(Uuid, String, bool, String)> = sqlx::query_as(
        "SELECT uuid, name, anonymous_control, watch_token FROM sessions
          WHERE (watch_token = $1 OR control_token = $1) AND ended_at IS NULL",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await?;
    let (session, name, anonymous_control, watch) = row.ok_or(AppError::NotFound)?;
    Ok(LinkTarget {
        session,
        name,
        access: if watch == token { SessionAccess::Watch } else { SessionAccess::Control },
        anonymous_control,
    })
}

/// What a session link leads to. Needs no login.
#[utoipa::path(get, path = "/links/{token}", tag = "sessions",
    params(("token" = String, Path)), responses((status = 200, body = SessionLink)))]
pub async fn link_info(State(state): State<AppState>, Path(token): Path<String>) -> AppResult<Json<SessionLink>> {
    let target = resolve_link(&state, &token).await?;
    Ok(Json(SessionLink {
        session: target.session,
        name: target.name,
        access: target.access,
        login_required: target.access == SessionAccess::Control && !target.anonymous_control,
    }))
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct SocketQuery {
    /// The token of a session link. Without one, the user's membership
    /// decides.
    pub link: Option<String>,
}

/// The socket a presenting device, a console or a viewer connects to. The
/// messages are `SessionServerMessage` and `SessionClientMessage` of the
/// `cantara-api` crate, as JSON text frames.
#[utoipa::path(get, path = "/sessions/{session}/socket", tag = "sessions",
    params(("session" = Uuid, Path), SocketQuery), responses((status = 101)))]
pub async fn socket(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
    Query(query): Query<SocketQuery>,
    headers: HeaderMap,
    user: MaybeUser,
    upgrade: WebSocketUpgrade,
) -> AppResult<Response> {
    let access = access_of(&state, session, query.link.as_deref(), &headers, user).await?;
    let live = state.live.get(&state.db, session).await?;
    Ok(upgrade.on_upgrade(move |socket| serve(state, live, access, socket)))
}

/// What a request may do with a session: by the link it carries, or by the
/// user's membership where it carries none.
async fn access_of(
    state: &AppState,
    session: Uuid,
    link: Option<&str>,
    headers: &HeaderMap,
    user: MaybeUser,
) -> AppResult<SessionAccess> {
    // A page elsewhere may make a request here and the browser would send the
    // cookie along; such a request is treated as anonymous.
    let foreign = headers
        .get(ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .is_some_and(|origin| origin != origin_of(&state.config.public_url));
    let user = if foreign { None } else { user.0 };

    match link {
        Some(token) => {
            let target = resolve_link(state, token).await?;
            if target.session != session {
                return Err(AppError::NotFound);
            }
            if target.access == SessionAccess::Control && !target.anonymous_control && user.is_none() {
                return Err(AppError::Unauthorized);
            }
            Ok(target.access)
        }
        None => {
            let user = user.ok_or(AppError::Unauthorized)?;
            let tenant: Uuid = sqlx::query_scalar("SELECT tenant_uuid FROM sessions WHERE uuid = $1 AND ended_at IS NULL")
                .bind(session)
                .fetch_one(&state.db)
                .await?;
            let member = member(state, &user, tenant).await?;
            Ok(match require(state, &member, ResourceKind::Session, session, Role::Read).await? {
                Role::Read => SessionAccess::Watch,
                Role::Write | Role::Manage => SessionAccess::Control,
            })
        }
    }
}

/// Hands a picture or document the presentation shows to the server, under
/// the path the slides name it by. Only for whoever may control the session —
/// in practice the computer presenting.
#[utoipa::path(put, path = "/sessions/{session}/media", tag = "sessions",
    params(("session" = Uuid, Path), cantara_api::SessionMediaQuery),
    request_body(content_type = "application/octet-stream", content = Vec<u8>),
    responses((status = 204)))]
pub async fn put_media(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
    Query(query): Query<cantara_api::SessionMediaQuery>,
    headers: HeaderMap,
    user: MaybeUser,
    body: axum::body::Body,
) -> AppResult<StatusCode> {
    if access_of(&state, session, query.link.as_deref(), &headers, user).await? != SessionAccess::Control {
        return Err(AppError::Forbidden);
    }
    if query.path.is_empty() || query.path.len() > 4096 {
        return Err(AppError::BadRequest("a path is needed".to_string()));
    }
    let mut incoming = state.blobs.incoming(state.config.max_upload_bytes)?;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| AppError::BadRequest(error.to_string()))?;
        incoming.write(&chunk)?;
    }
    let received = incoming.finish()?;
    let sha256 = received.sha256.clone();
    let size = received.size as i64;
    let blobs = state.blobs.clone();
    tokio::task::spawn_blocking(move || blobs.store(received))
        .await
        .map_err(AppError::internal)??;
    let file_name = query
        .path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("file")
        .to_string();
    sqlx::query("INSERT INTO blobs (sha256, size) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(&sha256)
        .bind(size)
        .execute(&state.db)
        .await?;
    sqlx::query(
        "INSERT INTO session_media (session_uuid, path, sha256, file_name) VALUES ($1, $2, $3, $4)
         ON CONFLICT (session_uuid, path) DO UPDATE SET sha256 = EXCLUDED.sha256, file_name = EXCLUDED.file_name",
    )
    .bind(session)
    .bind(&query.path)
    .bind(&sha256)
    .bind(&file_name)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A picture or document of the presentation, for whoever may watch it.
#[utoipa::path(get, path = "/sessions/{session}/media", tag = "sessions",
    params(("session" = Uuid, Path), cantara_api::SessionMediaQuery),
    responses((status = 200, content_type = "application/octet-stream"), (status = 404)))]
pub async fn get_media(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
    Query(query): Query<cantara_api::SessionMediaQuery>,
    headers: HeaderMap,
    user: MaybeUser,
    request: axum::extract::Request,
) -> AppResult<Response> {
    access_of(&state, session, query.link.as_deref(), &headers, user).await?;
    let (sha256, file_name): (String, String) =
        sqlx::query_as("SELECT sha256, file_name FROM session_media WHERE session_uuid = $1 AND path = $2")
            .bind(session)
            .bind(&query.path)
            .fetch_one(&state.db)
            .await?;
    crate::api::libraries::serve_blob(&state, &sha256, &file_name, request).await
}

fn text(message: &SessionServerMessage) -> Option<Message> {
    serde_json::to_string(message).ok().map(|text| Message::Text(text.into()))
}

async fn snapshot(live: &Live, access: SessionAccess) -> Option<Message> {
    let (revision, state) = live.snapshot().await;
    text(&SessionServerMessage::Snapshot { revision, state, access })
}

async fn serve(state: AppState, live: Arc<Live>, access: SessionAccess, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let mut events = live.subscribe();
    live.join();

    if let Some(message) = snapshot(&live, access).await
        && sender.send(message).await.is_err() {
            live.leave();
            return;
        }

    loop {
        tokio::select! {
            event = events.recv() => {
                let message = match event {
                    Ok(Event::Replaced { revision, state }) => text(&SessionServerMessage::Snapshot {
                        revision,
                        state: Some(state),
                        access,
                    }),
                    Ok(Event::Patched { revision, changes }) => text(&SessionServerMessage::Patch { revision, changes }),
                    Ok(Event::Viewers(count)) => text(&SessionServerMessage::Viewers { count }),
                    Ok(Event::Ended) => {
                        if let Some(message) = text(&SessionServerMessage::Ended) {
                            let _ = sender.send(message).await;
                        }
                        break;
                    }
                    // Too far behind to catch up change by change: start over.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => snapshot(&live, access).await,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                if let Some(message) = message
                    && sender.send(message).await.is_err() {
                        break;
                    }
            }
            incoming = receiver.next() => {
                let Some(Ok(incoming)) = incoming else { break };
                let Message::Text(body) = incoming else {
                    if matches!(incoming, Message::Close(_)) { break; }
                    continue;
                };
                let reply = match serde_json::from_str::<SessionClientMessage>(body.as_str()) {
                    Err(error) => Some(format!("not a message: {error}")),
                    Ok(SessionClientMessage::Resync) => {
                        if let Some(message) = snapshot(&live, access).await
                            && sender.send(message).await.is_err() { break; }
                        None
                    }
                    Ok(_) if access != SessionAccess::Control => Some("this link may only watch".to_string()),
                    Ok(SessionClientMessage::Replace { state: presented }) => {
                        live.replace(&state.db, presented).await.err().map(|error| error.to_string())
                    }
                    Ok(SessionClientMessage::Patch { changes }) => {
                        live.patch(&state.db, changes).await.err().map(|error| error.to_string())
                    }
                    Ok(SessionClientMessage::End) => {
                        state.live.end(&state.db, live.uuid).await.err().map(|error| error.to_string())
                    }
                };
                if let Some(message) = reply.and_then(|message| text(&SessionServerMessage::Error { message }))
                    && sender.send(message).await.is_err() { break; }
            }
        }
    }
    live.leave();
}
