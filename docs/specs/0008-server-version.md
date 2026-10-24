# 0008 — A server version of Cantara

Status: **partly built.** The questions are answered (each one carries its
**Answer**), and the stages of the [work plan](#work-plan) have been built as
far as [What was built](#what-was-built) says. What is still open is listed in
[What is left](#what-is-left). The brief below is unchanged. After it come what
the code says about the brief, the architecture, the questions, and the plan.

## The brief

With this spec, Cantara should be enhenced with a fullstack option that allows the app to be hosted as a webserver. Clients can than connect to it (via Cantara Sync) and use the server's repository items, design settings, slide settings and more.
In addition to that, people can use the web ui of the server to directly start and configure presentations or to explore the repositories (all features of desktop version should be available). Running presentations can also be shared via link (presentation and controle) or with other users.

The following things are needed:
- Authentication, tenants (organizations) and groups: All data is tenant-seperate, users and groups are needed for permission
- PostgreSQL database backend for repository items, settings etc.: A Postgres is intended to save repository items such as files, settings, etc. Every item should have `uuid` (a uuid v7) as primary key. Files should be versionable with each version getting a new uuid. The uuid of songs (tag uuid) will be saved in an additional field `lineage_uuid` to track changes. Where such a uuid doesn't exist yet, a new one wil be assigned.
- Admins (a special group) will be able to configure users, groups and permissions. Group managers can invite other people into their group.
- Item selections can be saved and managed as well as shared with other people and groups (e.g. for church service preparations)
- A running presentation can instantly be shared via links, users and groups.
- OpenID-support
- Clear API for communicating with the server (OpenAPI compatible endpoints)

The locally run Cantara instances should be able to continue to run as they do now, with some additions:
- Login to one or multiple online instances should be possible in the settings. The repositories etc. should then be available in the client
- The client should be able to cache the online repositories via its uuids for offline usage. As the primary key uuid would change with every version update, that ensures the right caching.

Add a new Dockerfile and an example .env as well as an example docker-compose.yml for the server sided deployment and hosting.

## What is already there

Six things in the code decide most of the shape of this.

**1. A repository is a source of files, and nothing more.**
[`Repository`](../../src/logic/settings.rs) is a name, two flags and a
[`RepositoryType`](../../src/logic/settings.rs) — a local folder, a remote ZIP,
a GitHub repository (and `Remote(String)`, which says of itself that it is not
implemented). Every kind ends in the same place: `get_files_async` returns a
list of [`SourceFile`](../../src/logic/sourcefiles.rs), and everything after
that — search, detail view, slides — reads those files. A server repository
is a fourth kind that produces the same list. **Nothing downstream of
`get_files_async` needs to know that a server exists**, and the plan keeps it
that way.

**2. The web build already works without a file system.**
On `wasm32` the downloaded repositories live in an in-memory VFS
(`web-zip://…`, `web-github://…`), settings in `localStorage`, and a
presentation tab and a console tab keep in step through
[`sync.rs`](../../src/logic/sync.rs). The brief's "web UI of the server with
all features of the desktop version" is, to a large degree, this build served
by the server with a `web-server://…` VFS prefix next to the other two.

**3. Identity of an element is already a question the code asks.**
[`element_id`](../../src/logic/element_id.rs) prefers a song's `uuid` tag and
falls back to a hash of `relative_path`. The brief's `lineage_uuid` is the
same idea made permanent: the tag where there is one, a newly assigned UUID
where there is none.

**4. Designs and slide settings are portable already.**
[`settings_io`](../../src/logic/settings_io.rs) writes and reads a design (with
its fonts) and a slide division as files, and
[`selection_io`](../../src/logic/selection_io.rs) writes a running order as
`selection.json` inside a `.cantara.zip`, with its designs and divisions
referenced by position. Those manifests are deliberately their own types,
versioned, tolerant of absent fields — exactly what an API payload has to be.
**The server stores and serves these formats rather than inventing new
ones.**

**5. A running presentation is serialisable, and is already sent over a socket.**
`RunningPresentation` is `Serialize`/`Deserialize`, the web build puts it into
`localStorage`, and the remote console
([`remote_console`](../../src/logic/remote_console.rs),
[`network_server`](../../src/logic/network_server.rs)) and the stream
([`stream::protocol`](../../src/logic/stream/protocol.rs)) already carry it, or
data built from it, across the network. Sharing a presentation through a
server is the same traffic going through a relay instead of a LAN socket.

**6. There is an axum server in the binary already, and a reason it lives in
a separate process.** The console helper (`--remote-console`) exists because
`dioxus_desktop` and `dioxus_liveview` fight over one global event converter.
A server binary that never calls `dioxus::launch` has no such conflict — but
it also means **the server must not be the desktop binary with a flag**, or
the same constraint follows it.

## What this is, in one picture

```
                       ┌──────────────────────── cantara-server ───────────────────────┐
 Browser ──HTTPS──────►│ /           static: the web build (WASM), server mode         │
 (web UI)              │ /api/v1/…   REST + OpenAPI  ──►  PostgreSQL (metadata)        │
                       │ /api/v1/…/ws  live sessions        blob store (file content)  │
 Desktop / Android ───►│ /auth/…     login (local + OIDC)                              │
 (RepositoryType::     └───────────────────────────────────────────────────────────────┘
  Server + cache)
```

* **One server binary** (`cantara-server`): axum, sqlx/Postgres, no Dioxus
  renderer. It stores, authorises and relays; it does not render slides.
* **One API**, used by every client alike — the web build, the desktop and
  later Android. The web UI has no private back door.
* **The web UI is the existing web build**, compiled with a `server-client`
  feature and served by the server. Presenting, the console, search and the
  detail view all run in the browser as they do on GitHub Pages today.
* **The desktop stays what it is.** A server is one more repository type plus
  a list of instances in the settings; without one, nothing changes.

The alternatives, and why not, are in [Q1](#q1-how-the-web-ui-is-built).

## The data model (proposed)

Every table has `uuid uuid primary key` (v7, generated by the server) and
every tenant-owned table a `tenant_uuid` — see [Q3](#q3-one-tenant-per-user-or-many).

| Table | What it holds |
|---|---|
| `tenants` | an organisation (e.g. church congregation): name, slug, settings |
| `users` | a person: display name, first name, last name, e-mail; not tenant-owned (Q3) |
| `memberships` | user × tenant, with `is_admin` |
| `identities` | how a user logs in: `local` (argon2 hash) or `oidc` (issuer + subject) |
| `groups` | tenant-owned; `admins` is a group with `system = true` that cannot be deleted |
| `group_members` | user × group, with `role ∈ {member, manager}` |
| `invitations` | group, e-mail, token hash, expiry, invited by |
| `libraries` | a server repository: what a client sees as one `Repository` |
| `items` | one element across all its versions: `lineage_uuid`, library, `kind`, `current_version_uuid`, `deleted_at` |
| `item_versions` | one version, immutable: `uuid`, `item`, `relative_path`, `file_name`, `content_hash` (SHA-256), `size`, `md5` (for `selection_io`), metadata JSON (title, tags), `created_by`, `created_at` |
| `blobs` | content by `content_hash`, stored once however many versions share it |
| `designs`, `slide_settings` | the JSON of `PresentationDesign` / `SongSlideSettings`, versioned the same way as items |
| `selections` | a saved running order: name, owner, date, the manifest as JSONB referring to `item_versions` |
| `grants` | resource × (user \| group \| link) × `role ∈ {read, write, manage}` |
| `sessions` | a shared running presentation: owner, state revision, link tokens |
| `api_tokens` | hashed tokens for desktop clients, with name, last use, expiry |

**Versioning.** An `item_versions` row is never updated. Saving a song writes
a new version and moves `items.current_version_uuid`. That is what makes the
brief's caching rule hold: a client that has version `0192…` cached has
exactly the bytes the server has under that UUID, forever.

**Lineage.** `items.lineage_uuid` is the song's `uuid` tag, where it has one;
otherwise a new UUID v7 is assigned when the item is first stored. Whether
that UUID is also written *into* the file is [Q6](#q6-is-the-assigned-uuid-written-into-the-song).
`lineage_uuid` is unique per tenant, not globally — two churches may well have
imported the same song file ([Q7](#q7-what-identifies-an-item-that-has-no-uuid-tag)).

**Content.** Metadata in Postgres, bytes in a blob store addressed by hash
([Q5](#q5-where-file-content-is-stored)).

## The API (proposed)

Versioned under `/api/v1`, JSON, described by an OpenAPI 3.1 document at
`/api/v1/openapi.json` that is generated from the handlers (utoipa), so the
document cannot drift from the code. A rendered reference at `/api/docs`.

| Area | Endpoints (sketch) |
|---|---|
| auth | `POST /auth/login`, `GET /auth/oidc/{provider}/start`, `GET /auth/oidc/callback`, `POST /auth/token` (desktop), `POST /auth/logout`, `GET /me` |
| admin | `/tenants/{t}/users`, `/groups`, `/groups/{g}/members`, `/invitations`, `/grants` |
| libraries | `GET /libraries`, `GET /libraries/{l}/index?since={revision}`, `POST /libraries/{l}/items` |
| items | `GET /items/{lineage}`, `GET /items/{lineage}/versions`, `PUT /items/{lineage}` (new version), `GET /versions/{uuid}/content` (immutable, `Cache-Control: immutable`, `ETag` = hash) |
| settings | `/designs`, `/slide-settings` (same pattern as items) |
| selections | `/selections`, `/selections/{s}`, `/selections/{s}/export` (a `.cantara.zip` built by `selection_io`) |
| sessions | `POST /sessions`, `GET /sessions/{s}`, `GET /sessions/{s}/ws`, `/sessions/{s}/links` |

**The index is the sync.** `GET /libraries/{l}/index` returns, per item, the
lineage, the current version UUID, path, name, kind, hash and size — and a
`revision` number for the library. A client that sends `?since=` gets only
what changed, including deletions. That is the whole of "Cantara Sync" on the
read side: compare the index with the cache, fetch the version UUIDs that are
missing, drop the ones nobody references.

## Questions

Each question says what is asked, what is recommended, and what the answer
costs. **Bold** is the recommendation.

### Q1. How the web UI is built

The brief asks for a "fullstack option". Three ways:

* **(a) The existing web build, served by the server, talking to the API.**
  One API for every client; the WASM app already presents, searches and runs
  the console. Cost: the web build's sources gain a server-backed VFS and a
  login screen; nothing renders on the server.
* (b) Dioxus fullstack (SSR + hydration + server functions). Server functions
  are an RPC, not OpenAPI endpoints, so the brief's "clear API" would be a
  second API next to them; and on 0.8.0-alpha it is the least settled part of
  Dioxus.
* (c) LiveView: the desktop UI rendered on the server per user. Every open tab
  is a `VirtualDom` and its state on the server, file dialogs and video have no
  answer, and a slide change is a round trip.

**Recommended: (a).** It is also the only one where the desktop and the browser
are clients of the same thing.

**Answer:** A, but then create a workspace with multiple crates underneath.

### Q2. One crate or a workspace

The server needs the formats (`selection_io`, `settings_io`, `PresentationDesign`,
`SongSlideSettings`, the song parser for metadata) but none of the UI. Today
everything is in one binary crate with no `lib.rs`.

**Recommended: a workspace with three members** — `cantara` (as now),
`crates/cantara-api` (the request/response types, `serde` + `utoipa` derives,
compiles on `wasm32`, shared by server and clients) and `crates/cantara-server`.
The format types the server needs move into the API crate or are reached
through it, not by depending on the UI crate. The cost is one restructuring
stage at the beginning ([S0](#s0-workspace)); the alternative, a `server`
feature on the one crate, puts axum, sqlx and the UI behind `cfg` in the same
files and makes every `cargo check` slower for everybody.

**Answer:** A workspace.

### Q3. One tenant per user, or many

Can one person belong to two organisations — say, two congregations, or a
congregation and a regional association?

**Recommended: yes.** Users are global, `memberships` connect them to tenants,
and the client chooses a tenant per instance after login. Cost: one more table
and a tenant switcher. Choosing "no" makes `users` tenant-owned and the same
e-mail address two accounts.

Related: **is the server multi-tenant at all**, or is one deployment one
organisation? Recommended: multi-tenant in the schema from the first stage
(retrofitting `tenant_uuid` is the expensive direction), with a
`CANTARA_SINGLE_TENANT=true` setting that hides the tenant UI for the common
case of one church on one server.

**Answer:** Follow recommendation.

### Q4. How people log in

* **Local accounts (e-mail + password, argon2id) and OIDC, both.** A small
  church has no identity provider; a diocese does. The first admin is created
  from `.env` (`CANTARA_BOOTSTRAP_ADMIN_EMAIL`, password set on first login).
* OIDC providers are configured per deployment in `.env` (issuer, client id,
  secret); per-tenant providers are a later addition. Recommended: per
  deployment now, the table shaped so per-tenant can follow.
* **Desktop login:** the OAuth 2.0 authorisation-code flow with PKCE against
  the *Cantara server* (which in turn may send the user on to OIDC), redirected
  to a loopback port the desktop opens for the moment — the flow
  `gh auth login` and most desktop apps use. The server hands out a long-lived
  API token, revocable in the web UI. Device-code flow as the fallback for
  Android/TV. Cost: the server is a small OAuth authorisation server; the
  alternative, typing a password into the desktop app, does not work with OIDC.
* **Where the desktop keeps the token:** the operating system's keyring
  (`keyring` crate), falling back to the settings file where there is none.
  The GitHub token is in the settings file in plain text today; the same
  treatment for it is a follow-up, not part of this.
* Is self-registration allowed, or only invitations? **Recommended: only
  invitations and admins**, per tenant switchable later.

**Answer:** Follow recommendation. Self-registration can be enabled.

### Q5. Where file content is stored

The brief says Postgres. Songs are kilobytes; PDFs, pictures and videos are
megabytes to gigabytes.

**Recommended: metadata in Postgres, content in a blob store behind a trait**,
with two implementations: a directory on a volume (default, in the compose file)
and S3-compatible storage (MinIO, Hetzner, AWS). Content-addressed by SHA-256,
so a version that changes only metadata costs no storage. A `bytea` column
would work for songs and makes backups one `pg_dump`, but a church that uploads
its videos would have a multi-gigabyte database within a year. Is there a size
limit per file and per tenant? Recommended: configurable, default 2 GiB per
file, no quota.

**Answer:** Follow recommondation. However, it should be possible to just define a volume via docker-compose where the files are saved. Take into consideration that if a text based file (e.g. a song) is updated, its metadata in the postgres might be updated as well.

### Q6. Is the assigned UUID written into the song?

A song without a `uuid` tag gets a `lineage_uuid` on the server. Is it also
written into the file as a tag?

**Recommended: yes, for songs, as part of the first stored version.** Then the
identity travels with the file — into an export, into a `.cantara.zip`, back
into a local folder — and `element_id` on every client already uses it.
Cost: the stored bytes differ from the uploaded ones, so the `md5`
`selection_io` uses to recognise "the same file" no longer matches the
original. That is acceptable because the UUID is the better identity, and
`resolve_selection` should be taught to prefer it (part of [S5](#s5-server-repositories-in-the-client)).
For PDFs, pictures and videos there is no tag to write; their lineage lives
only on the server.

**Answer:** Yes. And that is already implemented.

### Q7. What identifies an item that has no UUID tag?

Uploading `Lieder/Amazing Grace.song` a second time, or a folder sync that
sees a changed PDF: is that a new version of an existing item or a new item?

**Recommended: lineage tag first; otherwise the same `relative_path` within the
same library is the same item.** Equal content hash with a different path is
reported as a likely duplicate, not merged silently. An explicit "this is a new
version of …" in the UI covers the rest.

**Answer:** Follow recommondation.

### Q8. What the permissions are about

**Recommended: grants on three kinds of resource** — libraries, selections, and
designs/slide-setting collections — with roles `read`, `write`, `manage`
(manage = may grant). Grants go to users, groups, or (for selections and
sessions only) links. Tenant admins may do everything in their tenant; a group
manager may invite to and remove from their group but not change its grants.
Item-level permissions are **not** proposed: a song that only some may see is
a song in another library.

**Answer:** Follow recommondation.

### Q9. Designs and slide settings: shared, personal, or both?

Locally there is one list of each, and elements refer to designs by value or
by index. A server brings tenant-wide designs ("our church's design").

**Recommended: both, kept apart.** The server holds tenant-wide collections
(governed by grants) and nothing personal at first. On the client, a server's
designs appear as a read-only section of the design list, marked with the
instance's name; "copy to my designs" makes a local one. Editing a server design
in the client needs `write` and saves a new version. Syncing a user's *own*
local settings to the server (so that a second laptop has them) is a separate
feature and **not** in this spec. Is that the intended reading of "use the
server's design settings, slide settings"?

**Answer:** Follow recommondation.

### Q10. Can clients write, or only read?

**Recommended: both, gated by grants, but in this order:** reading (the sync
and the cache) in [S5](#s5-server-repositories-in-the-client), writing (upload,
edit a song, delete) in [S6](#s6-writing-from-the-client). The existing editing
paths write to `SourceFile::path`; for a server item, saving means `PUT` of a new
version and, offline, a queued change. **Conflicts:** a save names the version
it was based on; if the item has moved on, the server answers `409` and the
client offers "keep mine as a new version / take theirs". No merging of song
text.

**Answer:** Follow recommondation.

### Q11. What "sharing a running presentation" carries

Two things are being asked: watching (the slides, on a phone or a second
projector somewhere else) and controlling (the console).

* **The presenting device stays the authority.** A desktop or a browser tab
  presenting a service opens a session on the server and publishes its state;
  viewers and controllers connect to the session, never to the presenting
  device. That works across the internet and behind every church's NAT.
* **What is sent:** the slides themselves once (the
  `Vec<SlideChapter>`, which is what `RunningPresentation` carries and what the
  localStorage sync already sends between tabs), then only position, black
  screen, video commands and timer — revisioned, as
  [`StreamState`](../../src/logic/stream/protocol.rs) already is. A controller
  sends the same intents back; the presenting device applies them and publishes
  the result. Recommended over sending whole presentations both ways, as the
  LAN console does, because a relay should not have to carry a service's
  pictures every time someone presses "next".
* **Pictures, PDFs and videos** a slide refers to are fetched by the viewer
  from `/versions/{uuid}/content` where the element is a server item, and are
  uploaded to the session as temporary blobs where it is a local file. Is
  video in a shared session in scope? **Recommended: not in the first version**
  — the slide shows the video's poster and says that it plays in the room.
* **Links:** two per session, `…/s/{token}` (watch) and `…/s/{token}/control`,
  each revocable, each with an expiry (default: end of session + 1 h). A
  control link opens the console after login unless the session owner allows
  anonymous control. Sharing with users and groups puts the session on their
  start page while it runs.
* The LAN stream and remote console stay as they are. A session is additive.

**Answer:** The existing presentation sharing feature (without central server) should remain, running a presentation on a server ans sharing it from there should be added additionally.

### Q12. Saved selections and the existing file format

**Recommended: a saved selection is the `selection.json` manifest** of
[`selection_io`](../../src/logic/selection_io.rs), stored as JSONB, with one
extension: a `ManifestItem` may name `item_version` (and `lineage`) instead of
an archive path. Opening a saved selection pins nothing — the recommendation is
to resolve to the item's *current* version, with the version it was saved with
as the fallback if the item has been deleted. Exporting builds an ordinary
`.cantara.zip`, so nothing about the file format changes for people without a
server. Should a selection carry a date and a service type ("Gottesdienst
3. Advent") for the list? Recommended: a date and free text, nothing more.

**Answer:** Follow recommendation.

### Q13. Offline cache on the desktop

* **Where:** `cache_dir()/cantara/servers/{instance-uuid}/` with
  `index-{library}.json` and `blobs/{version-uuid}`. Version UUIDs are
  immutable, so a cached blob is never invalidated, only collected.
* **What is cached:** by default every library the user has subscribed to in
  the settings, completely. Recommended: a per-library switch "offline
  verfügbar" (default on for songs-only libraries, off for libraries above a
  size the UI shows), because a video library may not fit on a laptop.
* **When:** on start, on a manual refresh, and every 15 minutes while online.
  Offline, the library is the cache, marked as such in the source list.
* **Collection:** versions no current index and no saved selection refers to
  are deleted after 30 days.

**Answer:** Follow recommendation.

### Q14. Mobile and the browser

Android uses the desktop code paths and gets the same features. The browser
build on GitHub Pages stays a server-less demo; the `server-client` build is a
second web build with `base_path = "/"`, produced only for the Docker image.
Should a GitHub-Pages browser be able to log into a server (CORS)?
**Recommended: no**, at least at first — the server's own web UI is the browser
client, and the server can keep a strict same-origin policy.

**Answer:** Follow recommendation.

### Q15. Licence, data protection, operation

* The server is GPL-3.0 like the rest. Is AGPL wanted for the server, so that
  hosted modifications must be published? That is the owner's decision; it
  changes nothing technical.
* Personal data is names, e-mail addresses and login records. A tenant admin
  can export and delete a user's data (needed under the GDPR for any church in
  the EU); deletion keeps their item versions with `created_by` cleared.
* Logs go to stdout (structured, `tracing`), the container has `/healthz` and
  `/readyz`, migrations run on start.

**Answer:** The server should be in AGPL. Everything else follow recommendation.

## Work plan

Every stage ends with `cargo test`, Clippy and the browser suite green, and
with nothing half-visible to users: a feature appears in the UI in the stage
that finishes it. S0–S4 touch no client code beyond the workspace move and can
be reviewed as a server on their own.

### S0. Workspace

* Root `Cargo.toml` becomes a workspace; the app moves to `crates/cantara`
  (or stays at the root as a member — whichever keeps `dx` and the release
  workflow simplest; to be tried, not argued).
* `crates/cantara-api`: empty but compiling on `wasm32` and native.
* CI builds and tests all members; the release workflow is unchanged in
  output.
* `uuid` gains the `v7` feature.

### S1. Server skeleton and deployment

* `crates/cantara-server`: axum, tokio, `tracing`, config from environment
  (`envy` or `figment`), `/healthz`, `/readyz`, `/api/v1/openapi.json` via
  `utoipa` + `utoipa-axum`, a rendered reference at `/api/docs`.
* sqlx with Postgres, migrations in `crates/cantara-server/migrations`, run on
  start. Tests against a real Postgres (`sqlx::test`), started in CI as a
  service container.
* `Dockerfile` (multi-stage: `rust` builder for the server, `dx build --web
  --features server-client` for the web UI, `debian:bookworm-slim` runtime,
  non-root user, `/data` volume for blobs), `deploy/.env.example`,
  `deploy/docker-compose.yml` (server, `postgres:17`, a volume each, optionally
  Keycloak behind a profile for trying OIDC locally).
* README section "Running a Cantara server".

### S2. Tenants, users, groups, login

* Tables `tenants`, `users`, `memberships`, `identities`, `groups`,
  `group_members`, `invitations`, `api_tokens`.
* Local login (argon2id), sessions as HTTP-only cookies for the browser, API
  tokens for clients, bootstrap admin from `.env`.
* OIDC login (`openidconnect` crate), accounts linked by verified e-mail on
  first login only when an invitation for that address exists.
* The OAuth authorisation-code + PKCE endpoint for desktop clients, device-code
  flow.
* Admin endpoints: users, groups, members, invitations (invitation e-mail via
  SMTP from `.env`; without SMTP the invitation link is shown to copy).
* Tenant isolation is enforced in one place: every query goes through a
  repository layer that takes a `TenantScope` obtained from the authenticated
  request. Tests that try to read across tenants for every endpoint.

### S3. Libraries, items, versions

* Tables `libraries`, `items`, `item_versions`, `blobs`, `grants`; blob store
  trait with filesystem and S3 implementations.
* Upload of single files and of a ZIP (a whole existing repository, the same
  layout `RepositoryType::RemoteZip` reads); lineage per Q6/Q7; song metadata
  (title, tags) extracted with `cantara-songlib` on upload.
* The library index with `?since=` revisions, content download with
  `ETag`/`immutable`, version history, soft delete.
* Grants and their checks on every endpoint.

### S4. Designs, slide settings, selections on the server

* `designs` and `slide_settings` stored as the JSON `settings_io` already
  writes, fonts as blobs; versioned like items.
* `selections`: CRUD, sharing via grants, export as `.cantara.zip` through
  `selection_io` (now reachable from the server through the API crate).

### S5. Server repositories in the client

* Settings: a list of **instances** (`url`, display name, chosen tenant,
  token reference) — "Online-Instanzen" in the settings, with login (Q4),
  logout and the libraries offered there, each with "verwenden" and "offline
  verfügbar".
* `RepositoryType::Server { instance, library }`, implemented in
  `get_files_async` on every target: native through the cache (Q13), `wasm32`
  through a `web-server://` VFS prefix beside `web-zip://` and `web-github://`.
  `SourceFile` gains an optional `version_uuid`, so the detail view, export and
  selections can refer to a server item precisely.
* Server designs and slide settings in the client's lists (Q9).
* Saved selections: open from server, save to server, beside the existing file
  import and export.
* `resolve_selection` prefers the lineage UUID over `md5` (Q6).
* Offline: everything above works from the cache, and the UI says when it does.

### S6. Writing from the client

* Upload, edit and delete server items from the client where grants allow it;
  saving a song writes a new version; `409` handling (Q10).
* Offline edits queued and sent on reconnect, with the same conflict handling.

### S7. The web UI in server mode

* `server-client` feature on the app: login page, tenant switcher, the
  instance is the origin the page came from, no "Online-Instanzen" list.
* Admin pages (users, groups, invitations, grants) — in the same app, behind
  the admin role, rather than a second front end.
* Library management (upload, history, restore a version).
* Browser tests (Playwright) against a server started with a fixture database,
  following [0004](0004-testing-playwright.md).

### S8. Shared running presentations

* `sessions` table, the WebSocket relay, the session protocol in
  `cantara-api` (slides once, then revisioned state and intents — Q11).
* Presenting device: "Präsentation teilen" in the presentation options on
  desktop and web, publishing to a session on a chosen instance; links copied
  or shown as a QR code; sharing with users and groups.
* Viewer and controller pages in the web UI, built from the existing
  presentation and console components, so that a shared slide looks like the
  wall.
* Temporary upload of local pictures and PDFs a session needs; video out of
  scope (Q11).

### S9. Hardening

* Rate limits on login, size limits on uploads, CSRF protection for the cookie
  session, a security review of the token flows.
* Data export and deletion for a user (Q15).
* Backup and restore documented (`pg_dump` + the blob volume).

## What was built

All stages were started together. They are listed here by what exists now.

**S0 — Workspace.** The program stays at the root, so `dx` and the release
workflow are unchanged. `crates/cantara-api` holds the request, answer and
socket types, with OpenAPI derives behind the `openapi` feature. It builds for
`wasm32`. A plain `cargo build`/`cargo test` at the root still means the
program alone.

**S1 — Server and deployment.** `crates/cantara-server` (AGPL-3.0-or-later):
axum, sqlx/PostgreSQL with migrations run on start, configuration from the
environment, `/healthz` and `/readyz`. The OpenAPI document is generated with
utoipa (`/api/v1/openapi.json`, rendered at `/api/v1/docs`) and committed as
`crates/cantara-server/openapi.json`. A test fails when the document no longer
matches the code. Also built: a `Dockerfile` that builds the server and the
web build in its server flavour, plus `deploy/docker-compose.yml` (PostgreSQL,
a volume for the files that can be any host directory through `CANTARA_DATA`,
and Keycloak behind the profile `oidc`) and `deploy/.env.example`. CI has a
job of its own with PostgreSQL as a service.

**S2 — Accounts.** Every tenant has an `admins` group, and tenants are kept
apart in one place (`auth::member`). Logging in works with a password
(argon2id) and through OpenID Connect providers configured in the environment.
Self-registration can be switched on, and an invitation allows registration
even where it is off. Browser sessions are HTTP-only cookies; a request that
carries the cookie, changes something and comes from another origin is
refused. A desktop program gets an API token through the authorisation-code
flow with PKCE to a loopback port. Tokens can be listed and revoked, and a
user can export their data and delete their account. Repeated failed logins
are slowed down.

**S3 — Libraries.** Items have immutable versions. Content lives in a
directory addressed by SHA-256, which is the Docker volume. Metadata (title,
tags) is read again from every version, so a song whose title changed in its
text is listed under the new title at once (the answer to Q5). Lineage follows
Q6/Q7: a song without a `uuid` tag is given one, written into a `.song` or
`.song.yml`. Also built: the index with `?since=`, content with range requests
and `immutable` caching, version history, restore, soft delete, ZIP import of
a whole repository, and grants.

**S4 — Shared settings and running orders.** Designs and slide divisions are
stored as the program's own JSON and versioned, with `version_conflict`.
Running orders are the `selection.json` manifest, extended by `lineage` and
`item_version`.

**S5 — The program as a client.** *Settings → Online instances*: logging in
through the browser (the token is kept in the keyring, or in the settings file
where there is none), choosing the organisation, and choosing which libraries
to use. A used library is a repository of the new kind
`RepositoryType::Server`. It is cached by version in the user's cache
directory, checked against the SHA-256, and read from the cache when the
server cannot be reached. Unreferenced versions are collected after 30 days.
The server's designs and slide divisions can be taken over, and the user's own
can be offered to the organisation. Running orders can be saved to and opened
from a server.

**S7 — Web UI.** The `server-client` feature makes the web build the server's
client. It finds out who is logged in (or sends them to `/login`) and turns
the libraries they may use into repositories. An administration page
(`/admin`) is shared by the web build and the desktop: libraries with upload,
ZIP import and sharing; members and the admin role; invitations; groups and
their members.

**S8 — Presentations through the server.** Sessions with links to watch and to
control, and the anonymous control switch. The socket sends the whole
presentation once and then only the top-level fields that changed. The state is
written to the database, so a restart does not lose it. The desktop shares
through *Presentation options → Share through a server*, beside the LAN stream,
which is unchanged. The web build shows a link's presentation, or its console
for a control link (`/s/{token}`).

## What is left

- **S6, writing from the program.** The server's API for it is there (upload,
  new version with `base_version`, `version_conflict`). What is missing is in
  the program: saving an edited song back to a server library, and offline
  edits queued until the server can be reached.
- **Media in shared presentations.** A slide that shows a picture or a PDF
  page names a path on the presenting computer, and a viewer elsewhere cannot
  load it. Temporary upload of such files to the session (Q11) is not built
  yet. Song slides are unaffected.
- **Sharing from the web build.** It can watch and control a session, but it
  cannot start one yet: its presentation runs in a tab of its own, which the
  polling loop on the desktop does not reach.
- **Offline switch per library (Q13).** Every used library is cached
  completely. The switch for large video libraries is not built.
- **Invitations by e-mail.** No SMTP: the link is shown to the person inviting,
  to pass on.
- **Device-code flow** for devices without a browser.
- **S3-compatible storage.** Only the directory exists, which is what Q5's
  answer asked for first.
- **Fonts of a shared design** are not uploaded with it. A design that uses an
  installed font needs that font on every computer.
- **Per-tenant OIDC providers.** Providers are configured per deployment.
- **Not verified in a real browser yet:** the web build in server mode, the
  admin page, and `/s/{token}`. Their Rust code compiles and the server side is
  tested, but no Playwright test drives them yet.

## Testing

* **Server:** unit tests for lineage, versioning and grant resolution in pure
  Rust; integration tests per endpoint against Postgres via `sqlx::test`,
  including one cross-tenant test per resource; the OpenAPI document is
  generated in a test and diffed against a committed copy, so an API change is
  visible in review.
* **Client:** the cache and the index diff are pure and tested without a
  network; `RepositoryType::Server` against an in-process server in tests.
* **Browser:** the server-mode web UI and a shared session (one page presents,
  another watches, a third controls) in Playwright, with the server and a
  database started by the test setup.

## What this is not

* Not a replacement for local repositories or for the LAN stream and remote
  console. A Cantara without a server works exactly as before.
* Not a sync of a user's personal settings between devices (Q9).
* Not collaborative editing of a song. Two people saving the same song get a
  conflict, not a merge.
* Not server-side rendering of slides. The server stores, authorises and
  relays; slides are made where they are shown, by the code that makes them
  today.
* Not CCLI reporting or licence management, however close it sits to
  "repository items shared across a church".
