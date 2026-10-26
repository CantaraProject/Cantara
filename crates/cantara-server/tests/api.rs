//! The server, driven through its HTTP API against a real PostgreSQL.
//!
//! Each test gets a database of its own, created on the server that
//! `CANTARA_TEST_DATABASE_URL` points at (a URL whose user may create
//! databases). Without that variable the tests say so and pass — a plain
//! `cargo test` on a machine without PostgreSQL stays green, and CI sets it.
//!
//! ```sh
//! CANTARA_TEST_DATABASE_URL=postgres://cantara@127.0.0.1:5432/postgres cargo test -p cantara-server
//! ```

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use cantara_api::*;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use cantara_server::config::Config;

struct TestServer {
    app: Router,
    state: cantara_server::state::AppState,
    _blobs: tempfile::TempDir,
    mail: tempfile::TempDir,
    database_url: String,
}

async fn server() -> Option<TestServer> {
    let Ok(base) = std::env::var("CANTARA_TEST_DATABASE_URL") else {
        eprintln!("CANTARA_TEST_DATABASE_URL is not set; skipping");
        return None;
    };
    let name = format!("cantara_test_{}", Uuid::now_v7().simple());
    let admin = sqlx::PgPool::connect(&base).await.expect("the test database server answers");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await
        .expect("a test database can be created");
    let mut url = url::Url::parse(&base).expect("a URL");
    url.set_path(&name);

    let blobs = tempfile::tempdir().expect("a temporary directory");
    let mail = tempfile::tempdir().expect("a temporary directory");
    let mut config = Config::for_tests(url.as_str(), blobs.path().to_path_buf());
    config.storage = storage_under_test(&name);
    config.mail = cantara_server::config::MailConfig::Directory {
        path: mail.path().to_path_buf(),
        from: "Cantara <cantara@example.org>".to_string(),
    };
    let state = cantara_server::prepare(config).await.expect("the server starts");
    Some(TestServer {
        app: cantara_server::app(state.clone()),
        state,
        _blobs: blobs,
        mail,
        database_url: url.to_string(),
    })
}

/// Where these tests keep file content: a directory unless
/// `CANTARA_TEST_STORAGE` says `memory` (an object store in memory) or `s3`
/// (a bucket, named by `CANTARA_TEST_S3_BUCKET` and reached at
/// `CANTARA_TEST_S3_ENDPOINT` with `CANTARA_TEST_S3_ACCESS_KEY_ID` and
/// `CANTARA_TEST_S3_SECRET_ACCESS_KEY` — a Versity S3 Gateway, in CI). Each test keeps its
/// content under a prefix of its own.
fn storage_under_test(prefix: &str) -> cantara_server::config::StorageConfig {
    use cantara_server::config::StorageConfig;
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    match var("CANTARA_TEST_STORAGE").as_deref() {
        Some("memory") => StorageConfig::Memory,
        Some("s3") => StorageConfig::S3 {
            bucket: var("CANTARA_TEST_S3_BUCKET").expect("CANTARA_TEST_S3_BUCKET"),
            region: Some(var("CANTARA_TEST_S3_REGION").unwrap_or_else(|| "us-east-1".to_string())),
            endpoint: var("CANTARA_TEST_S3_ENDPOINT"),
            access_key_id: var("CANTARA_TEST_S3_ACCESS_KEY_ID"),
            secret_access_key: var("CANTARA_TEST_S3_SECRET_ACCESS_KEY"),
            prefix: prefix.to_string(),
            path_style: true,
        },
        _ => StorageConfig::Directory,
    }
}

/// Who a request is sent as.
#[derive(Clone)]
enum As {
    Nobody,
    Bearer(String),
    Cookie(String),
}

impl TestServer {
    async fn send(&self, request: Request<Body>) -> (StatusCode, Vec<u8>, axum::http::HeaderMap) {
        let response = self.app.clone().oneshot(request).await.expect("an answer");
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.into_body().collect().await.expect("a body").to_bytes().to_vec();
        (status, body, headers)
    }

    fn request(&self, method: &str, path: &str, who: &As) -> axum::http::request::Builder {
        let builder = Request::builder().method(method).uri(path);
        match who {
            As::Nobody => builder,
            As::Bearer(token) => builder.header(header::AUTHORIZATION, format!("Bearer {token}")),
            As::Cookie(token) => builder.header(header::COOKIE, format!("{SESSION_COOKIE}={token}")),
        }
    }

    async fn json<T: DeserializeOwned>(&self, method: &str, path: &str, who: &As, body: Option<Value>) -> (StatusCode, Option<T>) {
        let path = format!("{API_PREFIX}{path}");
        let request = match body {
            Some(body) => self
                .request(method, &path, who)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => self.request(method, &path, who).body(Body::empty()),
        }
        .expect("a request");
        let (status, bytes, _) = self.send(request).await;
        (status, serde_json::from_slice(&bytes).ok())
    }

    async fn ok<T: DeserializeOwned>(&self, method: &str, path: &str, who: &As, body: Option<Value>) -> T {
        let (status, value) = self.json::<Value>(method, path, who, body).await;
        assert!(status.is_success(), "{method} {path} answered {status}: {value:?}");
        serde_json::from_value(value.unwrap_or(Value::Null)).expect("the expected answer")
    }

    async fn status(&self, method: &str, path: &str, who: &As, body: Option<Value>) -> StatusCode {
        self.json::<Value>(method, path, who, body).await.0
    }

    async fn upload(&self, path: &str, who: &As, file_name: &str, content: &[u8], fields: &[(&str, &str)]) -> (StatusCode, Value) {
        let boundary = "cantara-test-boundary";
        let mut body = Vec::new();
        for (name, value) in fields {
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
        }
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let request = self
            .request(if path.contains("/items/") { "PUT" } else { "POST" }, &format!("{API_PREFIX}{path}"), who)
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(body))
            .expect("a request");
        let (status, bytes, _) = self.send(request).await;
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// Logs in with a password and returns the session cookie's token.
    async fn login(&self, email: &str, password: &str) -> As {
        let request = Request::post(format!("{API_PREFIX}/auth/login"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "email": email, "password": password }).to_string()))
            .expect("a request");
        let (status, _, headers) = self.send(request).await;
        assert_eq!(status, StatusCode::OK, "login of {email}");
        let cookie = headers
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .expect("a cookie");
        let token = cookie
            .split(';')
            .next()
            .and_then(|pair| pair.split_once('='))
            .map(|(_, token)| token.to_string())
            .expect("a token in the cookie");
        As::Cookie(token)
    }

    async fn register(&self, email: &str, name: &str) -> As {
        let _: Me = self
            .ok("POST", "/auth/register", &As::Nobody, Some(json!({ "email": email, "password": "a long password", "display_name": name })))
            .await;
        self.login(email, "a long password").await
    }
}

async fn admin_and_tenant(server: &TestServer) -> (As, Uuid) {
    let admin = server.login("admin@example.org", "admin-password").await;
    let me: Me = server.ok("GET", "/me", &admin, None).await;
    assert!(me.user.is_server_admin);
    assert_eq!(me.tenants.len(), 1, "the single tenant was created on first start");
    assert!(me.tenants[0].is_admin);
    (admin, me.tenants[0].tenant.uuid)
}

const SONG: &str = "#title: Amazing Grace\n#author: John Newton\n\nAmazing grace, how sweet the sound\nthat saved a wretch like me\n";

#[tokio::test]
async fn logging_in_and_out() {
    let Some(server) = server().await else { return };
    let info: ServerInfo = server.ok("GET", "/info", &As::Nobody, None).await;
    assert!(info.single_tenant);

    assert_eq!(server.status("GET", "/me", &As::Nobody, None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        server
            .status("POST", "/auth/login", &As::Nobody, Some(json!({ "email": "admin@example.org", "password": "wrong" })))
            .await,
        StatusCode::UNAUTHORIZED
    );
    let (admin, _) = admin_and_tenant(&server).await;
    assert_eq!(server.status("POST", "/auth/logout", &admin, None).await, StatusCode::NO_CONTENT);
    assert_eq!(server.status("GET", "/me", &admin, None).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_song_is_stored_versioned_and_synced() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let library: LibraryInfo = server
        .ok("POST", &format!("/tenants/{tenant}/libraries"), &admin, Some(json!({ "name": "Lieder" })))
        .await;
    let base = format!("/tenants/{tenant}/libraries/{}", library.uuid);

    // A song without a uuid is given one, written into the file.
    let (status, stored) = server
        .upload(&format!("{base}/items"), &admin, "Amazing Grace.song", SONG.as_bytes(), &[("relative_path", "Lieder/Amazing Grace.song")])
        .await;
    assert_eq!(status, StatusCode::OK, "{stored}");
    let stored: StoredItem = serde_json::from_value(stored).unwrap();
    assert_eq!(stored.outcome, StoreOutcome::Created);
    assert_eq!(stored.item.current.title, "Amazing Grace");
    assert_eq!(stored.item.current.tags.get("author").map(String::as_str), Some("John Newton"));
    let lineage = stored.item.lineage;
    let first_version = stored.item.current.uuid;

    let request = server
        .request("GET", &format!("{API_PREFIX}/tenants/{tenant}/versions/{first_version}/content"), &admin)
        .body(Body::empty())
        .unwrap();
    let (status, content, headers) = server.send(request).await;
    assert_eq!(status, StatusCode::OK);
    let content = String::from_utf8(content).unwrap();
    assert!(content.starts_with(&format!("#uuid: {lineage}\n")), "{content}");
    assert!(headers.get(header::CACHE_CONTROL).unwrap().to_str().unwrap().contains("immutable"));

    // The same file again is nothing new.
    let (_, again) = server
        .upload(&format!("{base}/items"), &admin, "Amazing Grace.song", SONG.as_bytes(), &[("relative_path", "Lieder/Amazing Grace.song")])
        .await;
    assert_eq!(again["outcome"], "unchanged");

    let index: LibraryIndex = server.ok("GET", &format!("{base}/index"), &admin, None).await;
    assert!(index.complete);
    assert_eq!(index.entries.len(), 1);
    let seen = index.revision;

    // Edited: a new version under the same lineage, and the sync sees it.
    let edited = content.replace("Amazing grace", "Amazing Grace");
    let (_, changed) = server
        .upload(&format!("{base}/items/{lineage}"), &admin, "Amazing Grace.song", edited.as_bytes(), &[("base_version", &first_version.to_string())])
        .await;
    assert_eq!(changed["outcome"], "new_version", "{changed}");
    let index: LibraryIndex = server.ok("GET", &format!("{base}/index?since={seen}"), &admin, None).await;
    assert!(!index.complete);
    assert_eq!(index.entries.len(), 1);
    assert_ne!(index.entries[0].version, first_version);

    // An edit that started from the old version is refused.
    let (status, conflict) = server
        .upload(&format!("{base}/items/{lineage}"), &admin, "Amazing Grace.song", b"#title: Other\n\nText\n", &[("base_version", &first_version.to_string())])
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["code"], "version_conflict");

    let versions: Vec<VersionInfo> = server.ok("GET", &format!("{base}/items/{lineage}/versions"), &admin, None).await;
    assert_eq!(versions.len(), 2);

    // Deleted, the sync is told; restored, it is back.
    let seen = index.revision;
    assert_eq!(server.status("DELETE", &format!("{base}/items/{lineage}"), &admin, None).await, StatusCode::NO_CONTENT);
    let index: LibraryIndex = server.ok("GET", &format!("{base}/index?since={seen}"), &admin, None).await;
    assert_eq!(index.removed, vec![lineage]);
    assert!(index.entries.is_empty());
    let restored: StoredItem = server
        .ok("POST", &format!("{base}/items/{lineage}/restore"), &admin, Some(json!({ "version": first_version })))
        .await;
    assert!(!restored.item.deleted);
    let index: LibraryIndex = server.ok("GET", &format!("{base}/index"), &admin, None).await;
    assert_eq!(index.entries.len(), 1);

    // Formats Cantara does not read are refused.
    let (status, _) = server.upload(&format!("{base}/items"), &admin, "notes.txt", b"text", &[]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn members_see_only_what_is_shared_with_them() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let singer = server.register("singer@example.org", "Sänger").await;
    let me: Me = server.ok("GET", "/me", &singer, None).await;
    let singer_uuid = me.user.uuid;
    assert_eq!(me.tenants.len(), 1, "registration joins the single tenant");
    assert!(!me.tenants[0].is_admin);

    let library: LibraryInfo = server
        .ok("POST", &format!("/tenants/{tenant}/libraries"), &admin, Some(json!({ "name": "Intern" })))
        .await;
    let base = format!("/tenants/{tenant}/libraries/{}", library.uuid);

    let visible: Vec<LibraryInfo> = server.ok("GET", &format!("/tenants/{tenant}/libraries"), &singer, None).await;
    assert!(visible.is_empty());
    assert_eq!(server.status("GET", &format!("{base}/index"), &singer, None).await, StatusCode::NOT_FOUND);
    assert_eq!(
        server.status("POST", &format!("/tenants/{tenant}/libraries"), &singer, Some(json!({ "name": "Meins" }))).await,
        StatusCode::FORBIDDEN
    );

    // Shared with a group the singer is in: readable, not writable.
    let group: GroupInfo = server
        .ok("POST", &format!("/tenants/{tenant}/groups"), &admin, Some(json!({ "name": "Chor" })))
        .await;
    assert_eq!(
        server
            .status("PUT", &format!("/tenants/{tenant}/groups/{}/members", group.uuid), &admin, Some(json!({ "user": singer_uuid })))
            .await,
        StatusCode::NO_CONTENT
    );
    let _: GrantInfo = server
        .ok(
            "POST",
            &format!("/tenants/{tenant}/grants/library/{}", library.uuid),
            &admin,
            Some(json!({ "grantee": { "type": "group", "uuid": group.uuid }, "role": "read" })),
        )
        .await;
    let visible: Vec<LibraryInfo> = server.ok("GET", &format!("/tenants/{tenant}/libraries"), &singer, None).await;
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].role, Role::Read);
    let (status, _) = server.upload(&format!("{base}/items"), &singer, "Lied.song", SONG.as_bytes(), &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Raised to write for the singer personally.
    let _: GrantInfo = server
        .ok(
            "POST",
            &format!("/tenants/{tenant}/grants/library/{}", library.uuid),
            &admin,
            Some(json!({ "grantee": { "type": "user", "uuid": singer_uuid }, "role": "write" })),
        )
        .await;
    let (status, _) = server.upload(&format!("{base}/items"), &singer, "Lied.song", SONG.as_bytes(), &[]).await;
    assert_eq!(status, StatusCode::OK);

    // A group member who is not a manager cannot add people.
    assert_eq!(
        server
            .status("PUT", &format!("/tenants/{tenant}/groups/{}/members", group.uuid), &singer, Some(json!({ "user": singer_uuid, "role": "manager" })))
            .await,
        StatusCode::FORBIDDEN
    );

    // Every member can use the tenant's designs; only granted ones change them.
    let entries: Vec<SettingsEntry> = server.ok("GET", &format!("/tenants/{tenant}/settings"), &singer, None).await;
    assert!(entries.is_empty());
    let design = json!({ "name": "Gottesdienst", "body": { "name": "Gottesdienst", "presentation_design_settings": {} } });
    assert_eq!(
        server.status("POST", &format!("/tenants/{tenant}/settings/design"), &singer, Some(design.clone())).await,
        StatusCode::FORBIDDEN
    );
    let created: SettingsEntry = server.ok("POST", &format!("/tenants/{tenant}/settings/design"), &admin, Some(design.clone())).await;
    let entries: Vec<SettingsEntry> = server.ok("GET", &format!("/tenants/{tenant}/settings?kind=design"), &singer, None).await;
    assert_eq!(entries.len(), 1);
    let saved: SettingsEntry = server
        .ok(
            "PUT",
            &format!("/tenants/{tenant}/settings/design/{}", created.lineage),
            &admin,
            Some(json!({ "name": "Neu", "body": {}, "base_version": created.version })),
        )
        .await;
    assert_ne!(saved.version, created.version);
    assert_eq!(
        server
            .status(
                "PUT",
                &format!("/tenants/{tenant}/settings/design/{}", created.lineage),
                &admin,
                Some(json!({ "name": "Alt", "body": {}, "base_version": created.version })),
            )
            .await,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn tenants_do_not_see_one_another() {
    let Some(server) = server().await else { return };
    let (admin, first) = admin_and_tenant(&server).await;
    let library: LibraryInfo = server
        .ok("POST", &format!("/tenants/{first}/libraries"), &admin, Some(json!({ "name": "Lieder" })))
        .await;
    let (_, stored) = server
        .upload(&format!("/tenants/{first}/libraries/{}/items", library.uuid), &admin, "Lied.song", SONG.as_bytes(), &[])
        .await;
    let version = stored["item"]["current"]["uuid"].as_str().unwrap().to_string();

    let second: TenantInfo = server
        .ok("POST", "/tenants", &admin, Some(json!({ "name": "Nachbargemeinde", "slug": "nachbar" })))
        .await;
    let invitation: InvitationInfo = server
        .ok("POST", &format!("/tenants/{}/invitations", second.uuid), &admin, Some(json!({ "email": "other@example.org" })))
        .await;
    let token = invitation.link.unwrap().rsplit('/').next().unwrap().to_string();
    let preview: InvitationPreview = server.ok("GET", &format!("/invitations/{token}"), &As::Nobody, None).await;
    assert_eq!(preview.tenant.uuid, second.uuid);
    let _: Me = server
        .ok(
            "POST",
            "/auth/register",
            &As::Nobody,
            Some(json!({ "email": "other@example.org", "password": "a long password", "display_name": "Other", "invitation": token })),
        )
        .await;
    let other = server.login("other@example.org", "a long password").await;
    let me: Me = server.ok("GET", "/me", &other, None).await;
    // Registration on a single-tenant server joins the default tenant too, so
    // this account is in both — leave the first, then look.
    if me.tenants.iter().any(|membership| membership.tenant.uuid == first) {
        assert_eq!(
            server.status("DELETE", &format!("/tenants/{first}/members/{}", me.user.uuid), &other, None).await,
            StatusCode::NO_CONTENT
        );
    }

    for path in [
        format!("/tenants/{first}/libraries"),
        format!("/tenants/{first}/libraries/{}/index", library.uuid),
        format!("/tenants/{first}/versions/{version}/content"),
        format!("/tenants/{first}/members"),
        format!("/tenants/{first}/selections"),
    ] {
        assert_eq!(server.status("GET", &path, &other, None).await, StatusCode::NOT_FOUND, "{path}");
    }
    // Asked through the tenant the account does belong to, the version of the
    // other tenant is not there either.
    assert_eq!(
        server.status("GET", &format!("/tenants/{}/versions/{version}/content", second.uuid), &other, None).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_desktop_program_gets_a_token_through_the_browser() {
    let Some(server) = server().await else { return };
    let (admin, _) = admin_and_tenant(&server).await;
    let As::Cookie(cookie) = admin.clone() else { unreachable!() };

    let verifier = "a-verifier-that-is-long-enough-to-be-a-real-pkce-verifier-1234";
    let challenge = {
        use base64::Engine;
        use sha2::Digest;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()))
    };
    let redirect = "http://127.0.0.1:53999/callback";
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", DESKTOP_CLIENT_ID)
        .append_pair("redirect_uri", redirect)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", "xyz")
        .append_pair("device_name", "Technik-Laptop")
        .finish();
    let request = Request::post("/authorize")
        .header(header::COOKIE, format!("{SESSION_COOKIE}={cookie}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
        .unwrap();
    let (status, _, headers) = server.send(request).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = url::Url::parse(headers.get(header::LOCATION).unwrap().to_str().unwrap()).unwrap();
    assert!(location.as_str().starts_with(redirect));
    let pairs: std::collections::HashMap<_, _> = location.query_pairs().into_owned().collect();
    assert_eq!(pairs["state"], "xyz");

    // A wrong verifier gets nothing, and burns the code.
    let token: TokenResponse = server
        .ok(
            "POST",
            "/auth/token",
            &As::Nobody,
            Some(json!({ "code": pairs["code"], "code_verifier": verifier, "redirect_uri": redirect, "device_name": "Technik-Laptop" })),
        )
        .await;
    let program = As::Bearer(token.access_token);
    let me: Me = server.ok("GET", "/me", &program, None).await;
    assert_eq!(me.user.email, "admin@example.org");
    assert_eq!(
        server
            .status("POST", "/auth/token", &As::Nobody, Some(json!({ "code": pairs["code"], "code_verifier": verifier, "redirect_uri": redirect })))
            .await,
        StatusCode::BAD_REQUEST,
        "a code is used once"
    );

    let tokens: Vec<ApiTokenInfo> = server.ok("GET", "/me/tokens", &admin, None).await;
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, "Technik-Laptop");
    assert_eq!(server.status("DELETE", &format!("/me/tokens/{}", tokens[0].uuid), &admin, None).await, StatusCode::NO_CONTENT);
    assert_eq!(server.status("GET", "/me", &program, None).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_cookie_request_from_another_site_is_refused() {
    let Some(server) = server().await else { return };
    let (As::Cookie(cookie), tenant) = admin_and_tenant(&server).await else { unreachable!() };
    let request = Request::post(format!("{API_PREFIX}/tenants/{tenant}/libraries"))
        .header(header::COOKIE, format!("{SESSION_COOKIE}={cookie}"))
        .header(header::ORIGIN, "https://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "name": "x" }).to_string()))
        .unwrap();
    assert_eq!(server.send(request).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn running_orders_are_saved_and_shared() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let singer = server.register("singer@example.org", "Sänger").await;
    let manifest = json!({ "format": "cantara.selection", "version": 1, "items": [{ "name": "Amazing Grace", "lineage": Uuid::now_v7() }] });
    let saved: SelectionDetail = server
        .ok("POST", &format!("/tenants/{tenant}/selections"), &admin, Some(json!({ "name": "3. Advent", "date": "2026-12-13", "manifest": manifest })))
        .await;
    assert_eq!(saved.summary.item_count, 1);
    assert_eq!(
        server
            .status("POST", &format!("/tenants/{tenant}/selections"), &admin, Some(json!({ "name": "x", "manifest": { "items": [] } })))
            .await,
        StatusCode::BAD_REQUEST
    );

    let visible: Vec<SelectionSummary> = server.ok("GET", &format!("/tenants/{tenant}/selections"), &singer, None).await;
    assert!(visible.is_empty());
    let me: Me = server.ok("GET", "/me", &singer, None).await;
    let _: GrantInfo = server
        .ok(
            "POST",
            &format!("/tenants/{tenant}/grants/selection/{}", saved.summary.uuid),
            &admin,
            Some(json!({ "grantee": { "type": "user", "uuid": me.user.uuid }, "role": "read" })),
        )
        .await;
    let visible: Vec<SelectionSummary> = server.ok("GET", &format!("/tenants/{tenant}/selections"), &singer, None).await;
    assert_eq!(visible.len(), 1);
    assert_eq!(
        server
            .status("PUT", &format!("/tenants/{tenant}/selections/{}", saved.summary.uuid), &singer, Some(json!({ "name": "x", "manifest": manifest })))
            .await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn a_presentation_is_shared_through_the_server() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let session: SessionInfo = server
        .ok("POST", &format!("/tenants/{tenant}/sessions"), &admin, Some(json!({ "name": "Gottesdienst" })))
        .await;
    let watch = session.watch_link.unwrap().rsplit('/').next().unwrap().to_string();
    let control = session.control_link.unwrap().rsplit('/').next().unwrap().to_string();

    let link: SessionLink = server.ok("GET", &format!("/links/{watch}"), &As::Nobody, None).await;
    assert_eq!(link.access, SessionAccess::Watch);
    assert!(!link.login_required);
    let link: SessionLink = server.ok("GET", &format!("/links/{control}"), &As::Nobody, None).await;
    assert!(link.login_required);

    // A real socket needs a real listener.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = server.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await });
    let socket_url = |link: &str| format!("ws://{address}{API_PREFIX}/sessions/{}/socket?link={link}", session.uuid);

    let As::Cookie(cookie) = admin.clone() else { unreachable!() };
    let mut request = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(socket_url(&control)).unwrap();
    request.headers_mut().insert(header::COOKIE, format!("{SESSION_COOKIE}={cookie}").parse().unwrap());
    let (mut controller, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (mut watcher, _) = tokio_tungstenite::connect_async(socket_url(&watch)).await.unwrap();

    async fn next(socket: &mut (impl StreamExt<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>> + Unpin)) -> SessionServerMessage {
        loop {
            let message = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
                .await
                .expect("a message in time")
                .expect("an open socket")
                .expect("a message");
            if let tokio_tungstenite::tungstenite::Message::Text(text) = message {
                let parsed: SessionServerMessage = serde_json::from_str(text.as_str()).unwrap();
                if !matches!(parsed, SessionServerMessage::Viewers { .. }) {
                    return parsed;
                }
            }
        }
    }
    let send = |message: SessionClientMessage| tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&message).unwrap().into());

    assert!(matches!(next(&mut controller).await, SessionServerMessage::Snapshot { state: None, access: SessionAccess::Control, .. }));
    assert!(matches!(next(&mut watcher).await, SessionServerMessage::Snapshot { state: None, access: SessionAccess::Watch, .. }));

    let presentation = json!({ "presentation": [{ "slides": [1, 2, 3] }], "position": { "chapter": 0, "slide": 0 }, "is_black_screen": false });
    controller.send(send(SessionClientMessage::Replace { state: presentation.clone() })).await.unwrap();
    match next(&mut watcher).await {
        SessionServerMessage::Snapshot { state: Some(state), revision, .. } => {
            assert_eq!(state, presentation);
            assert_eq!(revision, 1);
        }
        other => panic!("expected the presentation, got {other:?}"),
    }

    let after = json!({ "presentation": [{ "slides": [1, 2, 3] }], "position": { "chapter": 0, "slide": 1 }, "is_black_screen": false });
    controller
        .send(send(SessionClientMessage::Patch { changes: diff_top_level(&presentation, &after) }))
        .await
        .unwrap();
    match next(&mut watcher).await {
        SessionServerMessage::Patch { changes, revision } => {
            assert_eq!(revision, 2);
            assert_eq!(changes.len(), 1, "only the position travels");
            let mut state = presentation.clone();
            apply_patch(&mut state, &changes);
            assert_eq!(state, after);
        }
        other => panic!("expected a patch, got {other:?}"),
    }

    // A watcher cannot steer.
    watcher.send(send(SessionClientMessage::Patch { changes: serde_json::Map::new() })).await.unwrap();
    assert!(matches!(next(&mut watcher).await, SessionServerMessage::Error { .. }));

    // The state survives in the database.
    let db = sqlx::PgPool::connect(&server.database_url).await.unwrap();
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM sessions WHERE uuid = $1").bind(session.uuid).fetch_one(&db).await.unwrap();
    assert_eq!(revision, 2);

    // Ended by its owner, everybody is told.
    assert_eq!(server.status("DELETE", &format!("/tenants/{tenant}/sessions/{}", session.uuid), &admin, None).await, StatusCode::NO_CONTENT);
    assert!(matches!(next(&mut watcher).await, SessionServerMessage::Ended));
}

#[tokio::test]
async fn a_repository_is_imported_from_a_zip() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let library: LibraryInfo = server
        .ok("POST", &format!("/tenants/{tenant}/libraries"), &admin, Some(json!({ "name": "Import" })))
        .await;

    let mut archive = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut archive);
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in [
            ("Songs-main/Lieder/Amazing Grace.song", SONG),
            ("Songs-main/Lieder/Kopie.song", SONG),
            ("Songs-main/README.txt", "nothing"),
            ("Songs-main/Info.md", "# Info\ntext"),
        ] {
            zip.start_file(name, options).unwrap();
            std::io::Write::write_all(&mut zip, content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    let (status, report) = server
        .upload(&format!("/tenants/{tenant}/libraries/{}/import", library.uuid), &admin, "songs.zip", archive.get_ref(), &[])
        .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    let report: ImportReport = serde_json::from_value(report).unwrap();
    assert_eq!(report.created, 3);
    assert_eq!(report.skipped, vec!["README.txt".to_string()]);

    let index: LibraryIndex = server.ok("GET", &format!("/tenants/{tenant}/libraries/{}/index", library.uuid), &admin, None).await;
    let paths: Vec<_> = index.entries.iter().map(|entry| entry.relative_path.as_str()).collect();
    assert_eq!(paths, vec!["Info.md", "Lieder/Amazing Grace.song", "Lieder/Kopie.song"]);
    assert_eq!(index.entries[0].title, "Info");
}

#[tokio::test]
async fn a_device_without_a_browser_is_let_in_by_code() {
    let Some(server) = server().await else { return };
    let (admin, _) = admin_and_tenant(&server).await;

    let device: DeviceAuthorization = server
        .ok("POST", "/auth/device", &As::Nobody, Some(json!({ "client_id": DESKTOP_CLIENT_ID, "device_name": "Notenpult" })))
        .await;
    assert!(device.verification_uri.ends_with("/device"));
    assert_eq!(device.user_code.len(), 9);

    let server = &server;
    let poll = |code: String| async move {
        server
            .json::<ApiError>("POST", "/auth/device/token", &As::Nobody, Some(json!({ "device_code": code })))
            .await
    };
    let (status, error) = poll(device.device_code.clone()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error.unwrap().code, "authorization_pending");
    let (_, error) = poll(device.device_code.clone()).await;
    assert_eq!(error.unwrap().code, "slow_down", "asked again at once");

    // Somebody logged in types the code in, the way the page sends it.
    let As::Cookie(cookie) = admin else { unreachable!() };
    let typed = device.user_code.to_lowercase().replace('-', " ");
    let request = Request::post("/device")
        .header(header::COOKIE, format!("{SESSION_COOKIE}={cookie}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("user_code={}&decision=allow", typed.replace(' ', "+"))))
        .unwrap();
    let (status, page, _) = server.send(request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&page).contains("Verbunden"));

    let token: TokenResponse = server
        .ok("POST", "/auth/device/token", &As::Nobody, Some(json!({ "device_code": device.device_code })))
        .await;
    assert_eq!(token.user.email, "admin@example.org");
    let me: Me = server.ok("GET", "/me", &As::Bearer(token.access_token), None).await;
    assert_eq!(me.user.email, "admin@example.org");
    let tokens: Vec<ApiTokenInfo> = server.ok("GET", "/me/tokens", &As::Cookie(cookie), None).await;
    assert_eq!(tokens[0].name, "Notenpult");

    // The code is used up.
    assert_eq!(
        server
            .json::<ApiError>("POST", "/auth/device/token", &As::Nobody, Some(json!({ "device_code": device.device_code })))
            .await
            .1
            .unwrap()
            .code,
        "expired_token"
    );
}

#[tokio::test]
async fn the_pictures_of_a_shared_presentation_travel_with_it() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let session: SessionInfo = server
        .ok("POST", &format!("/tenants/{tenant}/sessions"), &admin, Some(json!({ "name": "Mit Bildern" })))
        .await;
    let watch = session.watch_link.unwrap().rsplit('/').next().unwrap().to_string();
    let path = "/home/technik/Bilder/Banner.png";
    let encoded: String = url::form_urlencoded::byte_serialize(path.as_bytes()).collect();
    let media = format!("{API_PREFIX}/sessions/{}/media?path={encoded}", session.uuid);

    // A watcher cannot put anything there.
    let request = server
        .request("PUT", &format!("{media}&link={watch}"), &As::Nobody)
        .body(Body::from(b"\x89PNG fake".to_vec()))
        .unwrap();
    assert_eq!(server.send(request).await.0, StatusCode::FORBIDDEN);

    // Before it is there, a viewer is told so.
    let request = server.request("GET", &format!("{media}&link={watch}"), &As::Nobody).body(Body::empty()).unwrap();
    assert_eq!(server.send(request).await.0, StatusCode::NOT_FOUND);

    // The presenting program, which owns the session, hands it over.
    let request = server.request("PUT", &media, &admin).body(Body::from(b"\x89PNG fake".to_vec())).unwrap();
    assert_eq!(server.send(request).await.0, StatusCode::NO_CONTENT);

    // And somebody holding the link to watch gets it.
    let request = server.request("GET", &format!("{media}&link={watch}"), &As::Nobody).body(Body::empty()).unwrap();
    let (status, bytes, headers) = server.send(request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"\x89PNG fake");
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/png");
}

#[tokio::test]
async fn an_invitation_is_sent_by_mail() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let invitation: InvitationInfo = server
        .ok("POST", &format!("/tenants/{tenant}/invitations"), &admin, Some(json!({ "email": "chor@example.org" })))
        .await;
    assert!(invitation.mailed);
    let link = invitation.link.unwrap();

    let messages: Vec<_> = std::fs::read_dir(server.mail.path()).unwrap().flatten().collect();
    assert_eq!(messages.len(), 1);
    let message = std::fs::read_to_string(messages[0].path()).unwrap();
    assert!(message.contains("To: chor@example.org"), "{message}");
    assert!(message.contains("Gemeinde"));
    // The link is in it — in the text, wrapped or not.
    let unfolded = message.replace("=\r\n", "").replace("=3D", "=");
    assert!(unfolded.contains(&link), "{message}");
}

#[tokio::test]
async fn a_design_travels_with_its_package() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let design: SettingsEntry = server
        .ok("POST", &format!("/tenants/{tenant}/settings/design"), &admin, Some(json!({ "name": "Fest", "body": { "name": "Fest" } })))
        .await;
    assert!(!design.has_package);
    let package_path = format!("{API_PREFIX}/tenants/{tenant}/settings/design/{}/package", design.lineage);

    let request = server.request("GET", &package_path, &admin).body(Body::empty()).unwrap();
    assert_eq!(server.send(request).await.0, StatusCode::NOT_FOUND);

    let request = server.request("PUT", &package_path, &admin).body(Body::from(b"PK zip".to_vec())).unwrap();
    let (status, body, _) = server.send(request).await;
    assert_eq!(status, StatusCode::OK);
    let entry: SettingsEntry = serde_json::from_slice(&body).unwrap();
    assert!(entry.has_package);

    let singer = server.register("singer@example.org", "Sänger").await;
    let request = server.request("GET", &package_path, &singer).body(Body::empty()).unwrap();
    let (status, bytes, _) = server.send(request).await;
    assert_eq!(status, StatusCode::OK, "every member may take a design over");
    assert_eq!(bytes, b"PK zip");
    let request = server.request("PUT", &package_path, &singer).body(Body::from(b"other".to_vec())).unwrap();
    assert_eq!(server.send(request).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_organisation_brings_its_own_login() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let singer = server.register("singer@example.org", "Sänger").await;
    let provider = json!({
        "slug": "bistum",
        "name": "Bistums-Login",
        "issuer": "https://login.example.org/realms/bistum",
        "client_id": "cantara",
        "client_secret": "geheim",
        "auto_join": true
    });
    assert_eq!(
        server.status("POST", &format!("/tenants/{tenant}/oidc-providers"), &singer, Some(provider.clone())).await,
        StatusCode::FORBIDDEN
    );
    let created: TenantOidcProvider = server
        .ok("POST", &format!("/tenants/{tenant}/oidc-providers"), &admin, Some(provider))
        .await;
    assert!(created.has_secret);
    assert_eq!(created.login_path, "/auth/oidc/t/default/bistum");
    assert!(created.redirect_uri.ends_with("/auth/oidc/callback"));

    // The secret is never handed out again.
    let listed: Vec<serde_json::Value> = server.ok("GET", &format!("/tenants/{tenant}/oidc-providers"), &admin, None).await;
    assert_eq!(listed.len(), 1);
    assert!(listed[0].get("client_secret").is_none());

    // Offered to everybody before logging in.
    let info: ServerInfo = server.ok("GET", "/info", &As::Nobody, None).await;
    assert!(info.oidc_providers.iter().any(|provider| provider.id == "t/default/bistum" && provider.name == "Bistums-Login"));
    let request = Request::get("/login").header(header::ACCEPT_LANGUAGE, "en").body(Body::empty()).unwrap();
    let (_, page, _) = server.send(request).await;
    let page = String::from_utf8_lossy(&page);
    assert!(page.contains("/auth/oidc/t/default/bistum"), "{page}");
    assert!(page.contains("Log in with Bistums-Login"));

    // A provider that does not exist is not started.
    let request = Request::get("/auth/oidc/t/default/nirgends").body(Body::empty()).unwrap();
    assert_eq!(server.send(request).await.0, StatusCode::NOT_FOUND);

    assert_eq!(
        server.status("DELETE", &format!("/tenants/{tenant}/oidc-providers/{}", created.uuid), &admin, None).await,
        StatusCode::NO_CONTENT
    );
    let info: ServerInfo = server.ok("GET", "/info", &As::Nobody, None).await;
    assert!(info.oidc_providers.is_empty());
}

#[tokio::test]
async fn what_a_presentation_showed_goes_when_it_ends() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;

    // A song in a library, whose content must survive any collecting.
    let library: LibraryInfo = server
        .ok("POST", &format!("/tenants/{tenant}/libraries"), &admin, Some(json!({ "name": "Lieder" })))
        .await;
    let (_, stored) = server
        .upload(&format!("/tenants/{tenant}/libraries/{}/items", library.uuid), &admin, "Lied.song", SONG.as_bytes(), &[])
        .await;
    let song_sha = stored["item"]["current"]["sha256"].as_str().unwrap().to_string();

    let session: SessionInfo = server
        .ok("POST", &format!("/tenants/{tenant}/sessions"), &admin, Some(json!({ "name": "Kurz" })))
        .await;
    let media = |path: &str| {
        let encoded: String = url::form_urlencoded::byte_serialize(path.as_bytes()).collect();
        format!("{API_PREFIX}/sessions/{}/media?path={encoded}", session.uuid)
    };
    let video = vec![7u8; 4096];
    let request = server.request("PUT", &media("/clips/Intro.mp4"), &admin).body(Body::from(video.clone())).unwrap();
    assert_eq!(server.send(request).await.0, StatusCode::NO_CONTENT);

    // A video is streamed: a range is answered with that range.
    let request = server
        .request("GET", &media("/clips/Intro.mp4"), &admin)
        .header(header::RANGE, "bytes=100-199")
        .body(Body::empty())
        .unwrap();
    let (status, bytes, headers) = server.send(request).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(bytes.len(), 100);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "video/mp4");

    assert_eq!(server.status("DELETE", &format!("/tenants/{tenant}/sessions/{}", session.uuid), &admin, None).await, StatusCode::NO_CONTENT);
    let db = sqlx::PgPool::connect(&server.database_url).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM session_media").fetch_one(&db).await.unwrap();
    assert_eq!(left, 0);

    let video_sha = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(&video))
    };
    assert!(server.state.blobs.exists(&video_sha).await);
    let removed = cantara_server::gc::collect(&server.state, std::time::Duration::ZERO, std::time::Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(removed, 1);
    assert!(!server.state.blobs.exists(&video_sha).await);
    assert!(server.state.blobs.exists(&song_sha).await, "what a library uses stays");
}

#[tokio::test]
async fn old_presentations_are_removed() {
    let Some(server) = server().await else { return };
    let (admin, tenant) = admin_and_tenant(&server).await;
    let singer = server.register("singer@example.org", "Sänger").await;
    let me: Me = server.ok("GET", "/me", &singer, None).await;
    let open = |name: &'static str| {
        let admin = admin.clone();
        let server = &server;
        async move {
            let session: SessionInfo = server
                .ok("POST", &format!("/tenants/{tenant}/sessions"), &admin, Some(json!({ "name": name })))
                .await;
            session.uuid
        }
    };
    let ended_long_ago = open("Ostern").await;
    let ended_recently = open("Pfingsten").await;
    let forgotten = open("Vergessen").await;
    let running = open("Läuft").await;
    let _: GrantInfo = server
        .ok(
            "POST",
            &format!("/tenants/{tenant}/grants/session/{ended_long_ago}"),
            &admin,
            Some(json!({ "grantee": { "type": "user", "uuid": me.user.uuid }, "role": "read" })),
        )
        .await;
    for session in [ended_long_ago, ended_recently] {
        assert_eq!(server.status("DELETE", &format!("/tenants/{tenant}/sessions/{session}"), &admin, None).await, StatusCode::NO_CONTENT);
    }

    // Time passes: one presentation ended more than a week ago, another was
    // left running when the laptop was closed a month ago.
    let db = sqlx::PgPool::connect(&server.database_url).await.unwrap();
    sqlx::query("UPDATE sessions SET ended_at = now() - interval '8 days' WHERE uuid = $1")
        .bind(ended_long_ago)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET last_active = now() - interval '31 days' WHERE uuid = $1")
        .bind(forgotten)
        .execute(&db)
        .await
        .unwrap();

    let removed = cantara_server::gc::remove_sessions(&server.state).await.unwrap();
    assert_eq!(removed, cantara_server::gc::SessionsRemoved { ended: 1, deleted: 1 });

    let remaining: Vec<(Uuid, bool)> = sqlx::query_as("SELECT uuid, ended_at IS NOT NULL FROM sessions ORDER BY created_at")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(remaining, vec![(ended_recently, true), (forgotten, true), (running, false)]);
    let grants: i64 = sqlx::query_scalar("SELECT count(*) FROM grants WHERE resource_kind = 'session'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(grants, 0, "what was shared of a deleted presentation goes with it");

    // The one ended for being idle is not deleted before its week is up.
    let again = cantara_server::gc::remove_sessions(&server.state).await.unwrap();
    assert_eq!(again, cantara_server::gc::SessionsRemoved::default());
}

#[test]
fn the_published_openapi_document_is_current() {
    let generated = serde_json::to_string_pretty(&cantara_server::api::openapi()).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("openapi.json");
    if std::env::var("UPDATE_OPENAPI").is_ok() || !path.exists() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap();
    assert!(
        committed == generated,
        "the API changed: run `UPDATE_OPENAPI=1 cargo test -p cantara-server` and commit openapi.json"
    );
}
