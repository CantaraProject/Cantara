//! Cantara as a client of a Cantara server.
//!
//! A server holds an organisation's libraries, designs, slide divisions and
//! running orders (`crates/cantara-server`, `docs/specs/0008-server-version.md`).
//! This program logs in to one or several of them, and a library on a server is
//! then one more kind of repository — [`RepositoryType::Server`] — that answers
//! with the same list of [`SourceFile`]s every other kind does. Nothing that
//! reads the library has to know a server is involved.
//!
//! What this module holds:
//!
//! * [`ServerInstance`] — a server this program is logged in to, as the
//!   settings keep it;
//! * [`Client`] — the API, for one instance;
//! * the registry of instances and their tokens, which a repository consults
//!   to find out where its server is ([`register`], [`client_for`]);
//! * [`cache`] (native) — the libraries on disk, kept by version uuid, which
//!   is what makes them usable offline;
//! * [`web`] (browser) — the same into the in-memory file system;
//! * [`login`] (native) — the browser-based login (OAuth 2.0 with PKCE to a
//!   loopback port);
//! * [`secrets`] — where a token is kept.
//!
//! [`RepositoryType::Server`]: crate::logic::settings::RepositoryType::Server
//! [`SourceFile`]: crate::logic::sourcefiles::SourceFile

use std::collections::HashMap;
use std::sync::RwLock;

use cantara_api::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(not(target_arch = "wasm32"))]
pub mod cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod login;
pub mod secrets;
pub mod session;
#[cfg(target_arch = "wasm32")]
pub mod web;

/// The instance a browser build of the server's own web app talks to: the
/// server it was loaded from. It has no entry in the settings, no token (the
/// browser's cookie does that), and its address is the page's.
pub const SAME_ORIGIN: Uuid = Uuid::nil();

/// A server this program is logged in to.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ServerInstance {
    /// This program's name for the instance — not the server's. Repositories
    /// refer to it.
    pub uuid: Uuid,
    /// Where the server is, without a trailing slash.
    pub url: String,
    /// What the server calls itself.
    pub name: String,
    /// The organisation on the server this program acts in.
    #[serde(default)]
    pub tenant: Option<Uuid>,
    #[serde(default)]
    pub tenant_name: String,
    /// Who is logged in, for showing.
    #[serde(default)]
    pub user_email: String,
    #[serde(default)]
    pub user_name: String,
    /// The token, where the system has no keyring to keep it in. `None` while
    /// it is in the keyring — see [`secrets`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_fallback: Option<String>,
}

/// What a repository needs to reach its server.
#[derive(Clone, Debug)]
struct Access {
    url: String,
    token: Option<String>,
}

static ACCESS: RwLock<Option<HashMap<Uuid, Access>>> = RwLock::new(None);

/// Tells the registry which instances there are and how to reach them. Called
/// when the settings are loaded and whenever an instance is added or removed.
pub fn register(instances: &[ServerInstance]) {
    let map: HashMap<Uuid, Access> = instances
        .iter()
        .map(|instance| {
            let token = instance
                .token_fallback
                .clone()
                .or_else(|| secrets::load(instance.uuid));
            (
                instance.uuid,
                Access {
                    url: instance.url.clone(),
                    token,
                },
            )
        })
        .collect();
    if let Ok(mut access) = ACCESS.write() {
        *access = Some(map);
    }
}

/// The API of an instance, if it is known.
pub fn client_for(instance: Uuid) -> Option<Client> {
    if instance == SAME_ORIGIN {
        return same_origin_url().map(|url| Client::new(&url, None));
    }
    let access = ACCESS.read().ok()?.as_ref()?.get(&instance)?.clone();
    Some(Client::new(&access.url, access.token))
}

/// The address of the page this build was loaded from.
fn same_origin_url() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()?.location().origin().ok()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Where a server's address, as somebody typed it, is normalised to.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn normalise_url(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let (scheme, rest) = match trimmed.split_once("://") {
        Some((scheme @ ("http" | "https"), rest)) => (scheme, rest),
        Some(_) => return None,
        None => ("https", trimmed),
    };
    // A host is needed, and nothing after it but an optional path.
    let rest = rest.trim_end_matches('/');
    if rest.is_empty() || rest.starts_with('/') || rest.contains(char::is_whitespace) {
        return None;
    }
    Some(format!("{scheme}://{rest}"))
}

// ── The API ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ClientError {
    /// The server could not be reached — offline, or the address is wrong.
    Unreachable(String),
    /// The token is no longer valid: log in again.
    Unauthorized,
    /// The server refused, with its reason.
    Refused { status: u16, error: Option<ApiError> },
    /// The answer was not what this program understands.
    Unreadable(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Unreachable(reason) => write!(f, "the server cannot be reached: {reason}"),
            ClientError::Unauthorized => write!(f, "the login at the server is no longer valid"),
            ClientError::Refused { status, error } => match error {
                Some(error) => write!(f, "{} ({status})", error.message),
                None => write!(f, "the server answered {status}"),
            },
            ClientError::Unreadable(reason) => write!(f, "the server's answer cannot be read: {reason}"),
        }
    }
}

/// The API of one server.
#[derive(Clone)]
pub struct Client {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

fn http_client() -> reqwest::Client {
    #[allow(unused_mut)]
    let mut builder = reqwest::Client::builder();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        builder = builder.use_preconfigured_tls(crate::logic::settings::mobile_tls_config());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        builder = builder.user_agent(concat!("Cantara/", env!("CARGO_PKG_VERSION")));
    }
    builder.build().unwrap_or_default()
}

impl Client {
    pub fn new(base: &str, token: Option<String>) -> Client {
        Client {
            base: base.trim_end_matches('/').to_string(),
            token,
            http: http_client(),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}{API_PREFIX}{path}", self.base)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.http.request(method, self.url(path));
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn answer<T: DeserializeOwned>(response: Result<reqwest::Response, reqwest::Error>) -> Result<T, ClientError> {
        let response = response.map_err(|error| ClientError::Unreachable(error.to_string()))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !status.is_success() {
            let error = response.json::<ApiError>().await.ok();
            return Err(ClientError::Refused {
                status: status.as_u16(),
                error,
            });
        }
        if status == reqwest::StatusCode::NO_CONTENT {
            return serde_json::from_value(serde_json::Value::Null)
                .map_err(|error| ClientError::Unreadable(error.to_string()));
        }
        response
            .json::<T>()
            .await
            .map_err(|error| ClientError::Unreadable(error.to_string()))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        Self::answer(self.request(reqwest::Method::GET, path).send().await).await
    }

    async fn send<T: DeserializeOwned, B: Serialize>(&self, method: reqwest::Method, path: &str, body: &B) -> Result<T, ClientError> {
        Self::answer(self.request(method, path).json(body).send().await).await
    }

    pub async fn info(&self) -> Result<ServerInfo, ClientError> {
        self.get("/info").await
    }

    pub async fn me(&self) -> Result<Me, ClientError> {
        self.get("/me").await
    }

    pub async fn logout(&self) -> Result<(), ClientError> {
        let response = self.request(reqwest::Method::POST, "/auth/logout").send().await;
        let response = response.map_err(|error| ClientError::Unreachable(error.to_string()))?;
        if response.status().is_success() { Ok(()) } else { Err(ClientError::Refused { status: response.status().as_u16(), error: None }) }
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub async fn exchange_code(&self, request: &TokenRequest) -> Result<TokenResponse, ClientError> {
        self.send(reqwest::Method::POST, "/auth/token", request).await
    }

    pub async fn libraries(&self, tenant: Uuid) -> Result<Vec<LibraryInfo>, ClientError> {
        self.get(&format!("/tenants/{tenant}/libraries")).await
    }

    pub async fn index(&self, tenant: Uuid, library: Uuid, since: Option<i64>) -> Result<LibraryIndex, ClientError> {
        match since {
            Some(since) => self.get(&format!("/tenants/{tenant}/libraries/{library}/index?since={since}")).await,
            None => self.get(&format!("/tenants/{tenant}/libraries/{library}/index")).await,
        }
    }

    /// The bytes of a version, as a response to be read as it arrives.
    pub async fn content(&self, tenant: Uuid, version: Uuid) -> Result<reqwest::Response, ClientError> {
        let response = self
            .request(reqwest::Method::GET, &format!("/tenants/{tenant}/versions/{version}/content"))
            .send()
            .await
            .map_err(|error| ClientError::Unreachable(error.to_string()))?;
        match response.status() {
            status if status.is_success() => Ok(response),
            reqwest::StatusCode::UNAUTHORIZED => Err(ClientError::Unauthorized),
            status => Err(ClientError::Refused {
                status: status.as_u16(),
                error: None,
            }),
        }
    }

    pub async fn settings(&self, tenant: Uuid) -> Result<Vec<SettingsEntry>, ClientError> {
        self.get(&format!("/tenants/{tenant}/settings")).await
    }

    /// Offers a design or slide division to the organisation. `kind` is
    /// `design` or `slide_settings`.
    pub async fn publish_settings(&self, tenant: Uuid, kind: &str, request: &SaveSettings) -> Result<SettingsEntry, ClientError> {
        self.send(reqwest::Method::POST, &format!("/tenants/{tenant}/settings/{kind}"), request).await
    }

    pub async fn selections(&self, tenant: Uuid) -> Result<Vec<SelectionSummary>, ClientError> {
        self.get(&format!("/tenants/{tenant}/selections")).await
    }

    pub async fn selection(&self, tenant: Uuid, selection: Uuid) -> Result<SelectionDetail, ClientError> {
        self.get(&format!("/tenants/{tenant}/selections/{selection}")).await
    }

    pub async fn create_selection(&self, tenant: Uuid, selection: &SaveSelection) -> Result<SelectionDetail, ClientError> {
        self.send(reqwest::Method::POST, &format!("/tenants/{tenant}/selections"), selection).await
    }

    pub async fn save_selection(&self, tenant: Uuid, uuid: Uuid, selection: &SaveSelection) -> Result<SelectionDetail, ClientError> {
        self.send(reqwest::Method::PUT, &format!("/tenants/{tenant}/selections/{uuid}"), selection).await
    }

    async fn delete(&self, path: &str) -> Result<(), ClientError> {
        let response = self
            .request(reqwest::Method::DELETE, path)
            .send()
            .await
            .map_err(|error| ClientError::Unreachable(error.to_string()))?;
        match response.status() {
            status if status.is_success() => Ok(()),
            reqwest::StatusCode::UNAUTHORIZED => Err(ClientError::Unauthorized),
            status => Err(ClientError::Refused {
                status: status.as_u16(),
                error: response.json::<ApiError>().await.ok(),
            }),
        }
    }

    async fn send_empty<B: Serialize>(&self, method: reqwest::Method, path: &str, body: &B) -> Result<(), ClientError> {
        let response = self
            .request(method, path)
            .json(body)
            .send()
            .await
            .map_err(|error| ClientError::Unreachable(error.to_string()))?;
        match response.status() {
            status if status.is_success() => Ok(()),
            reqwest::StatusCode::UNAUTHORIZED => Err(ClientError::Unauthorized),
            status => Err(ClientError::Refused {
                status: status.as_u16(),
                error: response.json::<ApiError>().await.ok(),
            }),
        }
    }

    // ── Administration ───────────────────────────────────────────────────

    pub async fn members(&self, tenant: Uuid) -> Result<Vec<MemberInfo>, ClientError> {
        self.get(&format!("/tenants/{tenant}/members")).await
    }

    pub async fn set_admin(&self, tenant: Uuid, user: Uuid, is_admin: bool) -> Result<(), ClientError> {
        self.send_empty(reqwest::Method::PATCH, &format!("/tenants/{tenant}/members/{user}"), &serde_json::json!({ "is_admin": is_admin }))
            .await
    }

    pub async fn remove_member(&self, tenant: Uuid, user: Uuid) -> Result<(), ClientError> {
        self.delete(&format!("/tenants/{tenant}/members/{user}")).await
    }

    pub async fn groups(&self, tenant: Uuid) -> Result<Vec<GroupInfo>, ClientError> {
        self.get(&format!("/tenants/{tenant}/groups")).await
    }

    pub async fn create_group(&self, tenant: Uuid, name: &str) -> Result<GroupInfo, ClientError> {
        self.send(reqwest::Method::POST, &format!("/tenants/{tenant}/groups"), &SaveGroup { name: name.to_string() })
            .await
    }

    pub async fn delete_group(&self, tenant: Uuid, group: Uuid) -> Result<(), ClientError> {
        self.delete(&format!("/tenants/{tenant}/groups/{group}")).await
    }

    pub async fn group_members(&self, tenant: Uuid, group: Uuid) -> Result<Vec<GroupMemberInfo>, ClientError> {
        self.get(&format!("/tenants/{tenant}/groups/{group}/members")).await
    }

    pub async fn save_group_member(&self, tenant: Uuid, group: Uuid, member: &SaveGroupMember) -> Result<(), ClientError> {
        self.send_empty(reqwest::Method::PUT, &format!("/tenants/{tenant}/groups/{group}/members"), member)
            .await
    }

    pub async fn remove_group_member(&self, tenant: Uuid, group: Uuid, user: Uuid) -> Result<(), ClientError> {
        self.delete(&format!("/tenants/{tenant}/groups/{group}/members/{user}")).await
    }

    pub async fn invitations(&self, tenant: Uuid) -> Result<Vec<InvitationInfo>, ClientError> {
        self.get(&format!("/tenants/{tenant}/invitations")).await
    }

    pub async fn invite(&self, tenant: Uuid, invitation: &CreateInvitation) -> Result<InvitationInfo, ClientError> {
        self.send(reqwest::Method::POST, &format!("/tenants/{tenant}/invitations"), invitation)
            .await
    }

    pub async fn withdraw_invitation(&self, tenant: Uuid, invitation: Uuid) -> Result<(), ClientError> {
        self.delete(&format!("/tenants/{tenant}/invitations/{invitation}")).await
    }

    pub async fn create_library(&self, tenant: Uuid, library: &SaveLibrary) -> Result<LibraryInfo, ClientError> {
        self.send(reqwest::Method::POST, &format!("/tenants/{tenant}/libraries"), library).await
    }

    pub async fn delete_library(&self, tenant: Uuid, library: Uuid) -> Result<(), ClientError> {
        self.delete(&format!("/tenants/{tenant}/libraries/{library}")).await
    }

    pub async fn grants(&self, tenant: Uuid, kind: &str, resource: Uuid) -> Result<Vec<GrantInfo>, ClientError> {
        self.get(&format!("/tenants/{tenant}/grants/{kind}/{resource}")).await
    }

    pub async fn grant(&self, tenant: Uuid, kind: &str, resource: Uuid, grant: &CreateGrant) -> Result<GrantInfo, ClientError> {
        self.send(reqwest::Method::POST, &format!("/tenants/{tenant}/grants/{kind}/{resource}"), grant)
            .await
    }

    pub async fn revoke_grant(&self, tenant: Uuid, kind: &str, resource: Uuid, grant: Uuid) -> Result<(), ClientError> {
        self.delete(&format!("/tenants/{tenant}/grants/{kind}/{resource}/{grant}")).await
    }

    /// Stores a file in a library — or, where `import` is set, a whole
    /// repository from a ZIP.
    pub async fn upload(
        &self,
        tenant: Uuid,
        library: Uuid,
        file_name: &str,
        bytes: Vec<u8>,
        import: bool,
    ) -> Result<serde_json::Value, ClientError> {
        let part = reqwest::multipart::Part::bytes(bytes).file_name(file_name.to_string());
        let form = reqwest::multipart::Form::new().part("file", part);
        let path = if import {
            format!("/tenants/{tenant}/libraries/{library}/import")
        } else {
            format!("/tenants/{tenant}/libraries/{library}/items")
        };
        Self::answer(self.request(reqwest::Method::POST, &path).multipart(form).send().await).await
    }

    pub async fn create_session(&self, tenant: Uuid, request: &CreateSession) -> Result<SessionInfo, ClientError> {
        self.send(reqwest::Method::POST, &format!("/tenants/{tenant}/sessions"), request).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn link(&self, token: &str) -> Result<SessionLink, ClientError> {
        self.get(&format!("/links/{token}")).await
    }

    /// The address of a session's socket.
    pub fn socket_url(&self, session: Uuid, link: Option<&str>) -> String {
        let base = self
            .base
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        match link {
            Some(link) => format!("{base}{API_PREFIX}/sessions/{session}/socket?link={link}"),
            None => format!("{base}{API_PREFIX}/sessions/{session}/socket"),
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}

// ── Which file is which item ─────────────────────────────────────────────────

/// What a file from a server library is on the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerItem {
    pub instance: Uuid,
    pub tenant: Uuid,
    pub library: Uuid,
    pub lineage: Uuid,
    pub version: Uuid,
}

static ITEMS: RwLock<Option<HashMap<String, ServerItem>>> = RwLock::new(None);

/// Remembers that the file at `path` is `item`. Called by the cache and the
/// web VFS as they hand out files, so that a running order saved to a server
/// can name its elements by lineage instead of by a path only this computer
/// knows.
pub fn remember_item(path: &str, item: ServerItem) {
    if let Ok(mut items) = ITEMS.write() {
        items.get_or_insert_with(HashMap::new).insert(path.to_string(), item);
    }
}

/// What the file at `path` is on a server, if it came from one.
pub fn item_of(path: &str) -> Option<ServerItem> {
    ITEMS.read().ok()?.as_ref()?.get(path).copied()
}

/// The display name of a file, as the rest of the program would give it.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn display_name(file_name: &str) -> String {
    crate::logic::sourcefiles::SourceFileType::display_name(file_name)
}

// ── What the settings do with instances ──────────────────────────────────────

use crate::logic::settings::{Repository, RepositoryType, Settings};

impl Settings {
    /// Adds an instance this program has just logged in to.
    pub fn add_server_instance(&mut self, instance: ServerInstance) {
        self.server_instances.retain(|known| known.uuid != instance.uuid);
        self.server_instances.push(instance);
        register(&self.server_instances);
    }

    /// Forgets an instance: its token, its libraries among the repositories,
    /// and what is cached of them.
    pub fn remove_server_instance(&mut self, uuid: Uuid) {
        self.server_instances.retain(|instance| instance.uuid != uuid);
        self.repositories.retain(|repository| {
            !matches!(repository.repository_type, RepositoryType::Server { instance, .. } if instance == uuid)
        });
        secrets::delete(uuid);
        #[cfg(not(target_arch = "wasm32"))]
        cache::remove_instance(uuid);
        register(&self.server_instances);
    }

    /// Switches the organisation an instance acts in. Its libraries from the
    /// old one are no longer repositories.
    pub fn set_server_tenant(&mut self, uuid: Uuid, tenant: Uuid, tenant_name: String) {
        if let Some(instance) = self.server_instances.iter_mut().find(|instance| instance.uuid == uuid) {
            instance.tenant = Some(tenant);
            instance.tenant_name = tenant_name;
        }
        self.repositories.retain(|repository| {
            !matches!(repository.repository_type, RepositoryType::Server { instance, tenant: used, .. } if instance == uuid && used != tenant)
        });
    }

    /// Whether a server library is among the repositories.
    pub fn uses_server_library(&self, instance: Uuid, library: Uuid) -> bool {
        self.repositories.iter().any(|repository| {
            matches!(repository.repository_type, RepositoryType::Server { instance: i, library: l, .. } if i == instance && l == library)
        })
    }

    /// Makes a server library a repository, or no longer one.
    pub fn use_server_library(&mut self, instance: Uuid, tenant: Uuid, library: &LibraryInfo, used: bool) {
        let present = self.uses_server_library(instance, library.uuid);
        if used && !present {
            self.repositories.push(Repository {
                name: library.name.clone(),
                removable: true,
                writing_permissions: library.role >= Role::Write,
                repository_type: RepositoryType::Server {
                    instance,
                    tenant,
                    library: library.uuid,
                },
            });
        } else if !used && present {
            self.repositories.retain(|repository| {
                !matches!(repository.repository_type, RepositoryType::Server { instance: i, library: l, .. } if i == instance && l == library.uuid)
            });
        }
    }

    /// The instance a repository's server is, if it is on one.
    pub fn server_instance(&self, uuid: Uuid) -> Option<&ServerInstance> {
        self.server_instances.iter().find(|instance| instance.uuid == uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_normalised() {
        assert_eq!(normalise_url("lieder.example.org/").as_deref(), Some("https://lieder.example.org"));
        assert_eq!(normalise_url("http://localhost:8080").as_deref(), Some("http://localhost:8080"));
        assert_eq!(normalise_url("  "), None);
        assert_eq!(normalise_url("https://"), None);
    }

    #[test]
    fn the_socket_follows_the_scheme() {
        let session = Uuid::nil();
        assert!(Client::new("https://a.example", None).socket_url(session, None).starts_with("wss://a.example/api/v1/"));
        assert!(Client::new("http://localhost:8080/", None).socket_url(session, Some("t")).ends_with("/socket?link=t"));
    }

    #[test]
    fn item_kinds_agree_with_the_program() {
        // `cantara-api` cannot depend on the program, so it carries its own
        // list of suffixes. This is what keeps the two lists one.
        use crate::logic::sourcefiles::SourceFileType;
        for name in [
            "a.song", "a.song.yml", "a.song.yaml", "a.ccli", "a.png", "a.jpg", "a.jpeg", "a.pdf", "a.md",
            "a.mp4", "a.m4v", "a.webm", "a.ogv", "a.mov", "a.txt", "a.mkv",
        ] {
            let program = SourceFileType::of(name);
            let api = ItemKind::of(name);
            let expected = match program {
                Some(SourceFileType::Song) => ItemKind::Song,
                Some(SourceFileType::Image) => ItemKind::Image,
                Some(SourceFileType::Video) => ItemKind::Video,
                Some(SourceFileType::Pdf) => ItemKind::Pdf,
                Some(SourceFileType::Markdown) => ItemKind::Markdown,
                Some(SourceFileType::Presentation) | None => ItemKind::Other,
            };
            assert_eq!(api, expected, "{name}");
        }
    }
}
