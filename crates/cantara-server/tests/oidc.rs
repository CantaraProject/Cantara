//! Logging in through OpenID Connect, against an identity provider that is
//! started for the purpose.
//!
//! The provider below is small but real: it answers discovery, publishes its
//! signing key, checks the PKCE verifier and the client's credentials, and
//! signs its ID tokens with RS256. The server talks to it over HTTP exactly
//! as it would to Keycloak or Entra ID — discovery, code exchange, token
//! verification are the server's own code, unchanged. Only the person typing
//! a password at the provider is skipped: the test hands the provider the
//! person who "logged in" and follows the redirect itself.
//!
//! Like `tests/api.rs`, this needs `CANTARA_TEST_DATABASE_URL`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Form, State};
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use cantara_api::*;
use http_body_util::BodyExt;
use rsa::pkcs1v15::SigningKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use uuid::Uuid;

use cantara_server::config::{Config, OidcProvider};

// ── The identity provider ────────────────────────────────────────────────────

const CLIENT_ID: &str = "cantara-server";
const CLIENT_SECRET: &str = "very-secret";

/// Somebody the provider vouches for.
#[derive(Clone)]
struct Person {
    subject: String,
    email: String,
    verified: bool,
    name: String,
}

/// What a code stands for, as the provider remembers it between sending the
/// browser back and being asked for tokens.
struct Grant {
    person: Person,
    nonce: String,
    challenge: String,
    redirect_uri: String,
}

struct Idp {
    base: String,
    key: RsaPrivateKey,
    grants: Mutex<HashMap<String, Grant>>,
}

/// One RSA key for the whole run: making one is the slow part.
fn key() -> RsaPrivateKey {
    static KEY: std::sync::OnceLock<RsaPrivateKey> = std::sync::OnceLock::new();
    KEY.get_or_init(|| RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).expect("an RSA key"))
        .clone()
}

async fn discovery(State(idp): State<Arc<Idp>>) -> Json<Value> {
    Json(json!({
        "issuer": idp.base,
        "authorization_endpoint": format!("{}/authorize", idp.base),
        "token_endpoint": format!("{}/token", idp.base),
        "jwks_uri": format!("{}/jwks", idp.base),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "scopes_supported": ["openid", "email", "profile"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic"],
    }))
}

async fn jwks(State(idp): State<Arc<Idp>>) -> Json<Value> {
    let public = idp.key.to_public_key();
    Json(json!({
        "keys": [{
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": "test",
            "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
            "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        }]
    }))
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: String,
    redirect_uri: String,
    code_verifier: String,
}

async fn token(State(idp): State<Arc<Idp>>, headers: HeaderMap, Form(form): Form<TokenForm>) -> (StatusCode, Json<Value>) {
    let refuse = |error: &str| (StatusCode::BAD_REQUEST, Json(json!({ "error": error })));

    // The client has to say who it is.
    let expected = format!("Basic {}", STANDARD.encode(format!("{CLIENT_ID}:{CLIENT_SECRET}")));
    if headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) != Some(expected.as_str()) {
        return refuse("invalid_client");
    }
    if form.grant_type != "authorization_code" {
        return refuse("unsupported_grant_type");
    }
    let Some(grant) = idp.grants.lock().unwrap().remove(&form.code) else {
        return refuse("invalid_grant");
    };
    // The code is good only together with the verifier the challenge was
    // made from, and only for where it was sent.
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(form.code_verifier.as_bytes()));
    if challenge != grant.challenge || form.redirect_uri != grant.redirect_uri {
        return refuse("invalid_grant");
    }

    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "iss": idp.base,
        "sub": grant.person.subject,
        "aud": CLIENT_ID,
        "iat": now,
        "exp": now + 300,
        "nonce": grant.nonce,
        "email": grant.person.email,
        "email_verified": grant.person.verified,
        "name": grant.person.name,
    });
    let header = json!({ "alg": "RS256", "typ": "JWT", "kid": "test" });
    let signed = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let signature = SigningKey::<Sha256>::new(idp.key.clone()).sign(signed.as_bytes()).to_bytes();
    let id_token = format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature));

    (
        StatusCode::OK,
        Json(json!({
            "access_token": "opaque",
            "token_type": "Bearer",
            "expires_in": 300,
            "id_token": id_token,
        })),
    )
}

async fn start_idp() -> Arc<Idp> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let idp = Arc::new(Idp {
        base: format!("http://{}", listener.local_addr().unwrap()),
        key: key(),
        grants: Mutex::new(HashMap::new()),
    });
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(jwks))
        .route("/token", post(token))
        .with_state(idp.clone());
    tokio::spawn(async move { axum::serve(listener, app).await });
    idp
}

// ── The Cantara server ───────────────────────────────────────────────────────

struct TestServer {
    app: Router,
    _blobs: tempfile::TempDir,
}

/// A server whose registration is closed — so that only an invitation or an
/// organisation's own provider lets anybody new in — knowing `idp` as a
/// provider of the whole deployment.
async fn server(idp: &Idp) -> Option<TestServer> {
    let Ok(base) = std::env::var("CANTARA_TEST_DATABASE_URL") else {
        eprintln!("CANTARA_TEST_DATABASE_URL is not set; skipping");
        return None;
    };
    let name = format!("cantara_test_{}", Uuid::now_v7().simple());
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = url::Url::parse(&base).unwrap();
    url.set_path(&name);

    let blobs = tempfile::tempdir().unwrap();
    let mut config = Config::for_tests(url.as_str(), blobs.path().to_path_buf());
    config.registration_open = false;
    config.oidc_providers = vec![OidcProvider {
        id: "zentral".to_string(),
        name: "Zentrales Login".to_string(),
        issuer: idp.base.clone(),
        client_id: CLIENT_ID.to_string(),
        client_secret: Some(CLIENT_SECRET.to_string()),
    }];
    let state = cantara_server::prepare(config).await.unwrap();
    Some(TestServer {
        app: cantara_server::app(state),
        _blobs: blobs,
    })
}

struct Answer {
    status: StatusCode,
    location: Option<String>,
    cookie: Option<String>,
    body: String,
}

impl TestServer {
    async fn get(&self, path: &str, cookie: Option<&str>) -> Answer {
        let mut request = Request::get(path).header(header::ACCEPT_LANGUAGE, "en");
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, format!("{SESSION_COOKIE}={cookie}"));
        }
        self.send(request.body(Body::empty()).unwrap()).await
    }

    async fn send(&self, request: Request<Body>) -> Answer {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let header_text = |name| {
            response
                .headers()
                .get(name)
                .and_then(|value: &header::HeaderValue| value.to_str().ok())
                .map(str::to_string)
        };
        let location = header_text(header::LOCATION);
        let cookie = header_text(header::SET_COOKIE).and_then(|cookie| {
            cookie
                .split(';')
                .next()
                .and_then(|pair| pair.split_once('='))
                .map(|(_, token)| token.to_string())
                .filter(|token| !token.is_empty())
        });
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        Answer {
            status,
            location,
            cookie,
            body: String::from_utf8_lossy(&body).into_owned(),
        }
    }

    async fn json(&self, method: &str, path: &str, cookie: &str, body: Option<Value>) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(format!("{API_PREFIX}{path}"))
            .header(header::COOKIE, format!("{SESSION_COOKIE}={cookie}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.map(|body| Body::from(body.to_string())).unwrap_or_else(Body::empty))
            .unwrap();
        let answer = self.send(request).await;
        (answer.status, serde_json::from_str(&answer.body).unwrap_or(Value::Null))
    }

    async fn admin(&self) -> String {
        let request = Request::post(format!("{API_PREFIX}/auth/login"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "email": "admin@example.org", "password": "admin-password" }).to_string()))
            .unwrap();
        self.send(request).await.cookie.expect("the administrator logs in")
    }

    /// Starts a login at `start`, lets `person` log in at the provider, and
    /// follows the provider back to the server. `nonce` replaces the nonce
    /// the provider is meant to repeat — for the test that it is checked.
    async fn log_in(&self, idp: &Idp, start: &str, person: &Person, nonce: Option<&str>) -> Answer {
        let sent = self.get(start, None).await;
        assert_eq!(sent.status, StatusCode::SEE_OTHER, "{}", sent.body);
        let location = url::Url::parse(&sent.location.unwrap()).unwrap();
        assert_eq!(format!("{}{}", location.origin().ascii_serialization(), location.path()), format!("{}/authorize", idp.base));
        let query: HashMap<String, String> = location.query_pairs().into_owned().collect();
        assert_eq!(query["client_id"], CLIENT_ID);
        assert_eq!(query["code_challenge_method"], "S256");
        assert!(query["scope"].contains("openid"));

        let code = Uuid::now_v7().simple().to_string();
        idp.grants.lock().unwrap().insert(
            code.clone(),
            Grant {
                person: person.clone(),
                nonce: nonce.map(str::to_string).unwrap_or_else(|| query["nonce"].clone()),
                challenge: query["code_challenge"].clone(),
                redirect_uri: query["redirect_uri"].clone(),
            },
        );
        let back = url::Url::parse(&query["redirect_uri"]).unwrap();
        self.get(&format!("{}?code={code}&state={}", back.path(), query["state"]), None).await
    }

    async fn me(&self, cookie: &str) -> Me {
        let (status, me) = self.json("GET", "/me", cookie, None).await;
        assert_eq!(status, StatusCode::OK);
        serde_json::from_value(me).unwrap()
    }
}

fn person(email: &str) -> Person {
    Person {
        subject: Uuid::now_v7().to_string(),
        email: email.to_string(),
        verified: true,
        name: "Hildegard Beispiel".to_string(),
    }
}

async fn add_provider(server: &TestServer, admin: &str, tenant: Uuid, issuer: &str, auto_join: bool) {
    let (status, created) = server
        .json(
            "POST",
            &format!("/tenants/{tenant}/oidc-providers"),
            admin,
            Some(json!({
                "slug": "bistum",
                "name": "Bistums-Login",
                "issuer": issuer,
                "client_id": CLIENT_ID,
                "client_secret": CLIENT_SECRET,
                "auto_join": auto_join,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
}

// ── The tests ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_organisations_own_provider_lets_its_people_join() {
    let idp = start_idp().await;
    let Some(server) = server(&idp).await else { return };
    let admin = server.admin().await;
    let tenant = server.me(&admin).await.tenants[0].tenant.uuid;
    add_provider(&server, &admin, tenant, &idp.base, true).await;

    // Somebody nobody invited, on a server whose registration is closed.
    let hildegard = person("hildegard@bistum.example");
    let answer = server.log_in(&idp, "/auth/oidc/t/default/bistum?next=/admin", &hildegard, None).await;
    assert_eq!(answer.status, StatusCode::SEE_OTHER, "{}", answer.body);
    assert_eq!(answer.location.as_deref(), Some("/admin"), "back where the login started");
    let cookie = answer.cookie.expect("a session");

    let me = server.me(&cookie).await;
    assert_eq!(me.user.email, "hildegard@bistum.example");
    assert_eq!(me.user.display_name, "Hildegard Beispiel");
    assert_eq!(me.tenants.len(), 1, "a member of the organisation whose provider vouched for her");
    assert_eq!(me.tenants[0].tenant.uuid, tenant);
    assert!(!me.tenants[0].is_admin);

    // The same person again is the same account.
    let again = server.log_in(&idp, "/auth/oidc/t/default/bistum", &hildegard, None).await;
    let me_again = server.me(&again.cookie.unwrap()).await;
    assert_eq!(me_again.user.uuid, me.user.uuid);
}

#[tokio::test]
async fn without_joining_only_the_invited_get_in() {
    let idp = start_idp().await;
    let Some(server) = server(&idp).await else { return };
    let admin = server.admin().await;
    let tenant = server.me(&admin).await.tenants[0].tenant.uuid;
    add_provider(&server, &admin, tenant, &idp.base, false).await;

    let otto = person("otto@bistum.example");
    let refused = server.log_in(&idp, "/auth/oidc/t/default/bistum", &otto, None).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.body.contains("no access to this server"), "{}", refused.body);
    assert!(refused.cookie.is_none());

    // Invited, the same login lets him in — into the organisation that
    // invited him.
    let (status, _) = server
        .json("POST", &format!("/tenants/{tenant}/invitations"), &admin, Some(json!({ "email": "otto@bistum.example" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let answer = server.log_in(&idp, "/auth/oidc/t/default/bistum", &otto, None).await;
    assert_eq!(answer.status, StatusCode::SEE_OTHER, "{}", answer.body);
    let me = server.me(&answer.cookie.unwrap()).await;
    assert_eq!(me.tenants.len(), 1);

    // The invitation is used up.
    let (_, invitations) = server.json("GET", &format!("/tenants/{tenant}/invitations"), &admin, None).await;
    assert_eq!(invitations.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn an_existing_account_is_joined_by_a_verified_address_only() {
    let idp = start_idp().await;
    let Some(server) = server(&idp).await else { return };

    // The administrator's address, but the provider does not vouch for it.
    let mut unverified = person("admin@example.org");
    unverified.verified = false;
    let refused = server.log_in(&idp, "/auth/oidc/zentral", &unverified, None).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);

    // Vouched for: the provider's account and the existing one are one from
    // now on.
    let verified = person("admin@example.org");
    let answer = server.log_in(&idp, "/auth/oidc/zentral", &verified, None).await;
    assert_eq!(answer.status, StatusCode::SEE_OTHER, "{}", answer.body);
    let me = server.me(&answer.cookie.unwrap()).await;
    assert!(me.user.is_server_admin);
    assert_eq!(me.user.display_name, "Administrator", "the account kept its own name");
}

#[tokio::test]
async fn a_token_that_does_not_answer_this_login_is_refused() {
    let idp = start_idp().await;
    let Some(server) = server(&idp).await else { return };

    // An ID token made for some other login — its nonce is not this one's.
    let answer = server
        .log_in(&idp, "/auth/oidc/zentral", &person("admin@example.org"), Some("a-different-nonce"))
        .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert!(answer.cookie.is_none());

    // A state the server never handed out leads nowhere either.
    let forged = server.get("/auth/oidc/callback?code=anything&state=made-up", None).await;
    assert_eq!(forged.status, StatusCode::BAD_REQUEST);
    assert!(forged.cookie.is_none());
}
