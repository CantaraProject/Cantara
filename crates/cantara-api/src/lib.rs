//! What a Cantara server and its clients say to one another.
//!
//! Every request body and every answer of the server's HTTP API is a type in
//! this crate, and so is every message on a shared presentation's socket. The
//! server serialises them, the desktop program and the browser build
//! deserialise them, and because both sides are compiled from the same
//! definitions they cannot drift apart. The OpenAPI document the server
//! publishes is generated from these types as well (feature `openapi`).
//!
//! Nothing here *does* anything: no I/O, no validation beyond what serde does.
//! See `docs/specs/0008-server-version.md` for what the API is for.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Where the API lives on a server. Every path below is relative to this.
pub const API_PREFIX: &str = "/api/v1";

/// The cookie a browser session is kept in.
pub const SESSION_COOKIE: &str = "cantara_session";

/// The client id the desktop program presents in the authorisation flow.
pub const DESKTOP_CLIENT_ID: &str = "cantara-desktop";

// ── Errors ───────────────────────────────────────────────────────────────────

/// What every failed request answers with, whatever its status code.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiError {
    /// A stable, machine-readable reason — `not_found`, `version_conflict`.
    pub code: String,
    /// The same, for a person.
    pub message: String,
    /// For a `version_conflict`: the version the item has now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<Uuid>,
}

// ── The server ───────────────────────────────────────────────────────────────

/// What anybody may ask a server before logging in: `GET /info`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ServerInfo {
    /// The name the operator gave this server.
    pub name: String,
    /// The version of `cantara-server`.
    pub version: String,
    /// Whether this server serves one organisation only. Clients hide the
    /// choice of tenant then.
    pub single_tenant: bool,
    /// Whether people may create an account without an invitation.
    pub registration_open: bool,
    /// Whether accounts with a password exist on this server at all.
    pub password_login: bool,
    /// The OpenID Connect providers one can log in with.
    pub oidc_providers: Vec<OidcProviderInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OidcProviderInfo {
    pub id: String,
    pub name: String,
}

// ── Users and logging in ─────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
    /// The token of an invitation. With one, an account can be created even
    /// where registration is closed — for the address the invitation names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation: Option<String>,
}

/// A person, as far as anybody else needs to know.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserInfo {
    pub uuid: Uuid,
    pub email: String,
    pub display_name: String,
    /// May create tenants and see every one of them. The account set up from
    /// the server's environment is one.
    pub is_server_admin: bool,
}

/// A person named as the author of something.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserRef {
    pub uuid: Uuid,
    pub display_name: String,
}

/// `GET /me`: who is logged in, and where they belong.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Me {
    pub user: UserInfo,
    pub tenants: Vec<TenantMembership>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TenantMembership {
    pub tenant: TenantInfo,
    /// Member of the tenant's `admins` group.
    pub is_admin: bool,
    pub groups: Vec<GroupRef>,
}

/// `POST /auth/token`: the end of the authorisation flow a desktop program
/// goes through — the code it was handed, and the verifier only it knows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TokenRequest {
    pub code: String,
    pub code_verifier: String,
    pub redirect_uri: String,
    /// What the token is to be called in the list of the user's tokens —
    /// usually the name of the computer.
    #[serde(default)]
    pub device_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TokenResponse {
    /// Sent as `Authorization: Bearer …` from now on. Shown once.
    pub access_token: String,
    pub user: UserInfo,
}

/// One of the tokens a user has handed out to programs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiTokenInfo {
    pub uuid: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

// ── Tenants, groups, invitations ─────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TenantInfo {
    pub uuid: Uuid,
    pub name: String,
    pub slug: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateTenant {
    pub name: String,
    pub slug: String,
}

/// A member of a tenant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MemberInfo {
    pub user: UserInfo,
    pub is_admin: bool,
    pub joined_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GroupRef {
    pub uuid: Uuid,
    pub name: String,
    pub role: GroupRole,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GroupInfo {
    pub uuid: Uuid,
    pub name: String,
    /// The `admins` group. It cannot be renamed or deleted.
    pub system: bool,
    pub member_count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SaveGroup {
    pub name: String,
}

/// What somebody is in a group.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum GroupRole {
    #[default]
    Member,
    /// May invite people into the group and remove them from it.
    Manager,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GroupMemberInfo {
    pub user: UserInfo,
    pub role: GroupRole,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SaveGroupMember {
    pub user: Uuid,
    #[serde(default)]
    pub role: GroupRole,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateInvitation {
    pub email: String,
    /// The group the invited person joins. Without one they only join the
    /// tenant — which only an admin may invite to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Uuid>,
    #[serde(default)]
    pub role: GroupRole,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InvitationInfo {
    pub uuid: Uuid,
    pub email: String,
    pub group: Option<GroupRef>,
    pub invited_by: Option<UserRef>,
    pub expires_at: DateTime<Utc>,
    /// The link to pass on. Only in the answer to creating the invitation —
    /// the server keeps nothing but a hash of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Whether the server sent the link by e-mail itself.
    #[serde(default)]
    pub mailed: bool,
}

/// What an invitation link shows before it is accepted.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InvitationPreview {
    pub tenant: TenantInfo,
    pub group: Option<String>,
    pub email: String,
    pub expires_at: DateTime<Utc>,
}

// ── Permissions ──────────────────────────────────────────────────────────────

/// How far somebody may go with a resource. Each role includes the ones
/// before it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Role {
    Read,
    Write,
    /// May also say who else may read and write.
    Manage,
}

/// The kinds of resource permissions are given on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ResourceKind {
    Library,
    Selection,
    /// The tenant's designs and slide settings, as one collection. Its id is
    /// the tenant's.
    Settings,
    /// A presentation running on the server.
    Session,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "type", content = "uuid", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Grantee {
    User(Uuid),
    Group(Uuid),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GrantInfo {
    pub uuid: Uuid,
    pub grantee: Grantee,
    /// The user's or group's name, for showing the list.
    pub grantee_name: String,
    pub role: Role,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateGrant {
    pub grantee: Grantee,
    pub role: Role,
}

// ── Libraries and their items ────────────────────────────────────────────────

/// A library: what a client shows as one repository.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LibraryInfo {
    pub uuid: Uuid,
    pub name: String,
    pub description: String,
    /// Counts up with every change to the library. See [`LibraryIndex`].
    pub revision: i64,
    pub item_count: i64,
    pub size_bytes: i64,
    /// What the asking user may do with it.
    pub role: Role,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SaveLibrary {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// What a file is, in the terms Cantara presents it in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ItemKind {
    Song,
    Image,
    Video,
    Pdf,
    Markdown,
    Other,
}

impl ItemKind {
    /// What a file name promises. Mirrors `SourceFileType::of` in the
    /// program, which a server cannot depend on; a test there keeps the two
    /// in step.
    pub fn of(file_name: &str) -> ItemKind {
        let lower = file_name.to_lowercase();
        const SUFFIXES: &[(&str, ItemKind)] = &[
            (".song.yml", ItemKind::Song),
            (".song.yaml", ItemKind::Song),
            (".song", ItemKind::Song),
            (".ccli", ItemKind::Song),
            (".png", ItemKind::Image),
            (".jpg", ItemKind::Image),
            (".jpeg", ItemKind::Image),
            (".pdf", ItemKind::Pdf),
            (".md", ItemKind::Markdown),
            (".mp4", ItemKind::Video),
            (".m4v", ItemKind::Video),
            (".webm", ItemKind::Video),
            (".ogv", ItemKind::Video),
            (".mov", ItemKind::Video),
        ];
        SUFFIXES
            .iter()
            .find(|(suffix, _)| lower.ends_with(suffix))
            .map(|(_, kind)| *kind)
            .unwrap_or(ItemKind::Other)
    }

    /// Whether the content is text a person edits, whose metadata the server
    /// reads out of it.
    pub fn is_text(self) -> bool {
        matches!(self, ItemKind::Song | ItemKind::Markdown)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Song => "song",
            ItemKind::Image => "image",
            ItemKind::Video => "video",
            ItemKind::Pdf => "pdf",
            ItemKind::Markdown => "markdown",
            ItemKind::Other => "other",
        }
    }

    pub fn parse(value: &str) -> ItemKind {
        match value {
            "song" => ItemKind::Song,
            "image" => ItemKind::Image,
            "video" => ItemKind::Video,
            "pdf" => ItemKind::Pdf,
            "markdown" => ItemKind::Markdown,
            _ => ItemKind::Other,
        }
    }
}

/// One item as the index lists it: enough to decide whether the cached copy
/// is current, and to fetch it when it is not.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IndexEntry {
    /// The item, across all its versions. A song's `uuid` tag.
    pub lineage: Uuid,
    /// The current version. Never reused: whatever is cached under this id is
    /// exactly what the server has under it.
    pub version: Uuid,
    /// Where the item sits in the library — `Lieder/Amazing Grace.song`.
    pub relative_path: String,
    pub file_name: String,
    pub title: String,
    pub kind: ItemKind,
    pub size: i64,
    pub sha256: String,
    pub md5: String,
    /// The library revision this entry last changed in.
    pub revision: i64,
    pub updated_at: DateTime<Utc>,
}

/// `GET /tenants/{t}/libraries/{l}/index[?since=n]`.
///
/// Asked without `since`, it lists every item and `complete` is true. Asked
/// with the revision a client last saw, it lists only what changed since and
/// what was removed since; a client applies that to what it has. A server that
/// cannot answer incrementally (the revision is too old) answers completely.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LibraryIndex {
    pub library: Uuid,
    pub revision: i64,
    pub complete: bool,
    pub entries: Vec<IndexEntry>,
    /// Lineages removed since `since`.
    pub removed: Vec<Uuid>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VersionInfo {
    pub uuid: Uuid,
    pub lineage: Uuid,
    pub relative_path: String,
    pub file_name: String,
    pub kind: ItemKind,
    pub size: i64,
    pub sha256: String,
    pub md5: String,
    pub title: String,
    /// The song's tags, where the item is a song.
    pub tags: BTreeMap<String, String>,
    pub created_by: Option<UserRef>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemInfo {
    pub lineage: Uuid,
    pub library: Uuid,
    pub current: VersionInfo,
    pub deleted: bool,
}

/// What storing a file did.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum StoreOutcome {
    /// A new item.
    Created,
    /// A new version of an item that was there.
    NewVersion,
    /// The content is the one the item already has; nothing was written.
    Unchanged,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StoredItem {
    pub outcome: StoreOutcome,
    pub item: ItemInfo,
    /// Another item of the library with the very same content, if there is
    /// one. Reported rather than merged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub possible_duplicate_of: Option<Uuid>,
}

/// `POST /tenants/{t}/libraries/{l}/import`: a whole repository as a ZIP.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportReport {
    pub created: usize,
    pub new_versions: usize,
    pub unchanged: usize,
    /// Files in the archive Cantara does not read.
    pub skipped: Vec<String>,
    /// Pairs of paths with identical content.
    pub possible_duplicates: Vec<(String, String)>,
}

// ── Designs and slide settings ───────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SettingsKind {
    /// A `PresentationDesign`.
    Design,
    /// A `SongSlideSettings`.
    SlideSettings,
}

impl SettingsKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SettingsKind::Design => "design",
            SettingsKind::SlideSettings => "slide_settings",
        }
    }
}

/// A design or a slide division the tenant shares.
///
/// The body is what the program serialises the setting as. The server does
/// not look inside it, so a newer program's fields survive an older server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SettingsEntry {
    pub lineage: Uuid,
    pub version: Uuid,
    pub kind: SettingsKind,
    pub name: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub body: serde_json::Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<UserRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SaveSettings {
    pub name: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub body: serde_json::Value,
    /// The version this edit started from. A save that names a version the
    /// entry has since moved on from is refused with `version_conflict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_version: Option<Uuid>,
}

// ── Saved selections ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SelectionSummary {
    pub uuid: Uuid,
    pub name: String,
    /// The day of the service, where the selection is for one.
    pub date: Option<NaiveDate>,
    pub note: String,
    pub owner: UserRef,
    pub updated_at: DateTime<Utc>,
    pub item_count: usize,
    /// What the asking user may do with it.
    pub role: Role,
}

/// A saved running order.
///
/// The manifest is the `selection.json` of a `.cantara.zip` (format
/// `cantara.selection`), whose items may name a server item by `lineage` and
/// `item_version` instead of by a file in an archive.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SelectionDetail {
    pub summary: SelectionSummary,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub manifest: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SaveSelection {
    pub name: String,
    #[serde(default)]
    pub date: Option<NaiveDate>,
    #[serde(default)]
    pub note: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub manifest: serde_json::Value,
}

// ── Presentations running on the server ──────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateSession {
    pub name: String,
    /// Whether somebody holding the control link may control without logging
    /// in. Off by default.
    #[serde(default)]
    pub anonymous_control: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionInfo {
    pub uuid: Uuid,
    pub name: String,
    pub owner: UserRef,
    pub created_at: DateTime<Utc>,
    pub revision: i64,
    /// Whether anybody has put a presentation into it yet.
    pub running: bool,
    pub anonymous_control: bool,
    pub viewers: usize,
    /// What the asking user may do: `read` watches, `write` controls,
    /// `manage` may also share and end it.
    pub role: Role,
    /// The links to pass on. Only for somebody who may share the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_link: Option<String>,
}

/// What a link to a session lets its holder do.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SessionAccess {
    Watch,
    Control,
}

/// `GET /links/{token}`: what a session link leads to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionLink {
    pub session: Uuid,
    pub name: String,
    pub access: SessionAccess,
    /// Whether the holder has to log in before connecting.
    pub login_required: bool,
}

/// What the server sends on a session's socket.
///
/// The state is the program's `RunningPresentation`, serialised; the server
/// keeps it as JSON and never reads inside it. A client that connects is sent
/// the whole of it once, and every change after that as the top-level fields
/// that changed — moving to the next slide is a `position` of a few bytes, not
/// the service with all its slides again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionServerMessage {
    /// Everything there is, on connecting and whenever a client asks.
    Snapshot {
        revision: i64,
        /// `None` while nothing has been presented yet.
        state: Option<serde_json::Value>,
        access: SessionAccess,
    },
    /// The fields of the state that changed, as of `revision`.
    Patch {
        revision: i64,
        changes: serde_json::Map<String, serde_json::Value>,
    },
    /// How many are connected.
    Viewers { count: usize },
    /// The presentation was ended. The socket closes after this.
    Ended,
    /// A message of the client's that was refused.
    Error { message: String },
}

/// What a client sends on a session's socket. Only a client that may control
/// the session is listened to; a watcher's messages are refused.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionClientMessage {
    /// A whole new state — starting the presentation, or replacing it.
    Replace { state: serde_json::Value },
    /// Some top-level fields of the state, changed.
    Patch {
        changes: serde_json::Map<String, serde_json::Value>,
    },
    /// Send the whole state again.
    Resync,
    /// End the presentation for everybody.
    End,
}

/// The fields of `next` that differ from `previous`, at the top level.
///
/// What a client sends as a [`SessionClientMessage::Patch`]; what the server
/// and every other client apply with [`apply_patch`]. A field that is absent in
/// `next` is sent as `null`.
pub fn diff_top_level(
    previous: &serde_json::Value,
    next: &serde_json::Value,
) -> serde_json::Map<String, serde_json::Value> {
    let mut changes = serde_json::Map::new();
    let (Some(previous), Some(next)) = (previous.as_object(), next.as_object()) else {
        return changes;
    };
    for (key, value) in next {
        if previous.get(key) != Some(value) {
            changes.insert(key.clone(), value.clone());
        }
    }
    for key in previous.keys() {
        if !next.contains_key(key) {
            changes.insert(key.clone(), serde_json::Value::Null);
        }
    }
    changes
}

/// Applies a patch made by [`diff_top_level`].
pub fn apply_patch(state: &mut serde_json::Value, changes: &serde_json::Map<String, serde_json::Value>) {
    if !state.is_object() {
        *state = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(object) = state.as_object_mut() {
        for (key, value) in changes {
            object.insert(key.clone(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_patch_carries_only_what_changed_and_restores_the_whole() {
        let before = json!({ "presentation": [1, 2, 3], "position": { "chapter": 0 }, "is_black_screen": false });
        let after = json!({ "presentation": [1, 2, 3], "position": { "chapter": 1 }, "is_black_screen": false });

        let changes = diff_top_level(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes["position"], json!({ "chapter": 1 }));

        let mut applied = before.clone();
        apply_patch(&mut applied, &changes);
        assert_eq!(applied, after);
    }

    #[test]
    fn kinds_follow_the_suffix_ignoring_case() {
        assert_eq!(ItemKind::of("Amazing Grace.song.yml"), ItemKind::Song);
        assert_eq!(ItemKind::of("LIED.CCLI"), ItemKind::Song);
        assert_eq!(ItemKind::of("Banner.PNG"), ItemKind::Image);
        assert_eq!(ItemKind::of("notes.txt"), ItemKind::Other);
        assert_eq!(ItemKind::parse(ItemKind::Video.as_str()), ItemKind::Video);
    }

    #[test]
    fn messages_are_tagged_by_type() {
        let message = SessionClientMessage::Patch {
            changes: serde_json::Map::new(),
        };
        let text = serde_json::to_string(&message).unwrap();
        assert_eq!(text, r#"{"type":"patch","changes":{}}"#);
    }
}
