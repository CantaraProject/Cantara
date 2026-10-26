//! What the server is told by its environment.
//!
//! Everything comes from environment variables, because that is what a
//! container is configured with; `deploy/.env.example` lists every one of them
//! with what it does. Nothing here is read again after start.

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

/// One OpenID Connect provider people may log in with.
#[derive(Clone, Debug)]
pub struct OidcProvider {
    /// The short name used in URLs — `keycloak`.
    pub id: String,
    /// What the login page calls it — `Bistum-Login`.
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

/// Where file content is kept.
#[derive(Clone, Debug, Default)]
pub enum StorageConfig {
    /// In `blob_dir`.
    #[default]
    Directory,
    /// In an S3-compatible bucket. `blob_dir` is where uploads are received
    /// before they go there.
    S3 {
        bucket: String,
        region: Option<String>,
        /// For anything that is not AWS itself — MinIO, Hetzner, Wasabi.
        endpoint: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        /// Prepended to every key, so that a bucket can be shared.
        prefix: String,
        /// `bucket.host/key` (virtual hosted, AWS's way) or `host/bucket/key`
        /// (what MinIO usually wants).
        path_style: bool,
    },
    /// In memory, for the tests: gone when the server is.
    #[doc(hidden)]
    Memory,
}

/// How invitations are sent.
#[derive(Clone, Debug, Default)]
pub enum MailConfig {
    /// Not at all: the link is shown to whoever invites, to pass on.
    #[default]
    Off,
    Smtp {
        host: String,
        port: u16,
        /// `starttls` (the default), `tls`, or `none` — the last only for a
        /// relay on the same machine.
        security: String,
        username: Option<String>,
        password: Option<String>,
        from: String,
    },
    /// Into a directory, one `.eml` file per message — for development and
    /// for the tests.
    Directory { path: PathBuf, from: String },
}

#[derive(Clone, Debug)]
pub struct Config {
    /// What this server is called on its pages and in clients.
    pub name: String,
    pub listen: SocketAddr,
    /// The address people reach the server at, without a trailing slash —
    /// `https://lieder.example.org`. Links handed out (invitations, shared
    /// presentations) start with it, and OpenID Connect redirects come back
    /// to it.
    pub public_url: String,
    pub database_url: String,
    /// Where file content is kept. In the container, a volume.
    pub blob_dir: PathBuf,
    /// Where the web build is, if this server serves one.
    pub web_dir: Option<PathBuf>,
    pub single_tenant: bool,
    /// The tenant created on first start in single-tenant mode.
    pub default_tenant_name: String,
    pub registration_open: bool,
    pub password_login: bool,
    pub bootstrap_admin_email: Option<String>,
    pub bootstrap_admin_password: Option<String>,
    /// The largest file accepted, in bytes.
    pub max_upload_bytes: u64,
    /// How long a browser session lasts without being used, in days.
    pub session_days: i64,
    /// Whether presentations may be shared through this server.
    pub sharing_enabled: bool,
    pub oidc_providers: Vec<OidcProvider>,
    pub mail: MailConfig,
    pub storage: StorageConfig,
    /// How long an ended presentation is kept, in days.
    pub keep_ended_sessions_days: i64,
    /// After how many days without a change a presentation nobody ended is
    /// ended and removed.
    pub idle_session_days: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} must be set")]
    Missing(&'static str),
    #[error("{name} is not valid: {reason}")]
    Invalid { name: String, reason: String },
}

fn var(name: &str) -> Option<String> {
    env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

fn flag(name: &str, default: bool) -> Result<bool, ConfigError> {
    match var(name).as_deref() {
        None => Ok(default),
        Some("1" | "true" | "yes" | "on") => Ok(true),
        Some("0" | "false" | "no" | "off") => Ok(false),
        Some(other) => Err(ConfigError::Invalid {
            name: name.to_string(),
            reason: format!("expected true or false, got `{other}`"),
        }),
    }
}

fn number<T: std::str::FromStr>(name: &str, default: T) -> Result<T, ConfigError> {
    match var(name) {
        None => Ok(default),
        Some(value) => value.parse().map_err(|_| ConfigError::Invalid {
            name: name.to_string(),
            reason: format!("`{value}` is not a number"),
        }),
    }
}

impl Config {
    pub fn from_env() -> Result<Config, ConfigError> {
        let listen = var("CANTARA_LISTEN").unwrap_or_else(|| "0.0.0.0:8080".to_string());
        let listen = listen.parse().map_err(|_| ConfigError::Invalid {
            name: "CANTARA_LISTEN".to_string(),
            reason: format!("`{listen}` is not an address and port"),
        })?;

        let public_url = var("CANTARA_PUBLIC_URL")
            .unwrap_or_else(|| "http://localhost:8080".to_string())
            .trim_end_matches('/')
            .to_string();
        if url::Url::parse(&public_url).is_err() {
            return Err(ConfigError::Invalid {
                name: "CANTARA_PUBLIC_URL".to_string(),
                reason: format!("`{public_url}` is not a URL"),
            });
        }

        let mut oidc_providers = Vec::new();
        for id in var("CANTARA_OIDC_PROVIDERS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            let prefix = format!("CANTARA_OIDC_{}_", id.to_uppercase().replace('-', "_"));
            let get = |suffix: &str| var(&format!("{prefix}{suffix}"));
            let missing = |suffix: &str| ConfigError::Invalid {
                name: format!("{prefix}{suffix}"),
                reason: format!("must be set for the provider `{id}`"),
            };
            oidc_providers.push(OidcProvider {
                id: id.to_string(),
                name: get("NAME").unwrap_or_else(|| id.to_string()),
                issuer: get("ISSUER").ok_or_else(|| missing("ISSUER"))?,
                client_id: get("CLIENT_ID").ok_or_else(|| missing("CLIENT_ID"))?,
                client_secret: get("CLIENT_SECRET"),
            });
        }

        let from = var("CANTARA_MAIL_FROM").unwrap_or_else(|| "Cantara <cantara@localhost>".to_string());
        let mail = match (var("CANTARA_SMTP_HOST"), var("CANTARA_MAIL_DIR")) {
            (Some(host), _) => {
                let security = var("CANTARA_SMTP_SECURITY").unwrap_or_else(|| "starttls".to_string());
                if !matches!(security.as_str(), "starttls" | "tls" | "none") {
                    return Err(ConfigError::Invalid {
                        name: "CANTARA_SMTP_SECURITY".to_string(),
                        reason: format!("expected starttls, tls or none, got `{security}`"),
                    });
                }
                let default_port = match security.as_str() {
                    "tls" => 465,
                    "none" => 25,
                    _ => 587,
                };
                MailConfig::Smtp {
                    host,
                    port: number("CANTARA_SMTP_PORT", default_port)?,
                    security,
                    username: var("CANTARA_SMTP_USERNAME"),
                    password: var("CANTARA_SMTP_PASSWORD"),
                    from,
                }
            }
            (None, Some(path)) => MailConfig::Directory { path: PathBuf::from(path), from },
            (None, None) => MailConfig::Off,
        };

        let storage = match var("CANTARA_S3_BUCKET") {
            Some(bucket) => StorageConfig::S3 {
                bucket,
                region: var("CANTARA_S3_REGION"),
                endpoint: var("CANTARA_S3_ENDPOINT"),
                access_key_id: var("CANTARA_S3_ACCESS_KEY_ID"),
                secret_access_key: var("CANTARA_S3_SECRET_ACCESS_KEY"),
                prefix: var("CANTARA_S3_PREFIX").unwrap_or_default(),
                path_style: flag("CANTARA_S3_PATH_STYLE", false)?,
            },
            None => StorageConfig::Directory,
        };

        Ok(Config {
            mail,
            storage,
            keep_ended_sessions_days: number("CANTARA_KEEP_ENDED_SESSIONS_DAYS", 7)?,
            idle_session_days: number("CANTARA_IDLE_SESSION_DAYS", 30)?,
            name: var("CANTARA_SERVER_NAME").unwrap_or_else(|| "Cantara".to_string()),
            listen,
            public_url,
            database_url: var("DATABASE_URL").ok_or(ConfigError::Missing("DATABASE_URL"))?,
            blob_dir: var("CANTARA_BLOB_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./data/blobs")),
            web_dir: var("CANTARA_WEB_DIR").map(PathBuf::from),
            single_tenant: flag("CANTARA_SINGLE_TENANT", true)?,
            default_tenant_name: var("CANTARA_TENANT_NAME").unwrap_or_else(|| "Gemeinde".to_string()),
            registration_open: flag("CANTARA_ALLOW_REGISTRATION", false)?,
            password_login: flag("CANTARA_PASSWORD_LOGIN", true)?,
            bootstrap_admin_email: var("CANTARA_BOOTSTRAP_ADMIN_EMAIL"),
            bootstrap_admin_password: var("CANTARA_BOOTSTRAP_ADMIN_PASSWORD"),
            max_upload_bytes: number("CANTARA_MAX_UPLOAD_BYTES", 2 * 1024 * 1024 * 1024)?,
            session_days: number("CANTARA_SESSION_DAYS", 30)?,
            sharing_enabled: flag("CANTARA_ALLOW_SHARING", true)?,
            oidc_providers,
        })
    }

    /// Whether the cookie may be sent over plain HTTP. Only where the public
    /// address is plain HTTP — a development server, or one behind nothing.
    pub fn secure_cookies(&self) -> bool {
        self.public_url.starts_with("https://")
    }

    /// A configuration for tests: everything at its default, against the
    /// given database.
    #[doc(hidden)]
    pub fn for_tests(database_url: &str, blob_dir: PathBuf) -> Config {
        Config {
            name: "Test".to_string(),
            listen: ([127, 0, 0, 1], 0).into(),
            public_url: "http://localhost:8080".to_string(),
            database_url: database_url.to_string(),
            blob_dir,
            web_dir: None,
            single_tenant: true,
            default_tenant_name: "Gemeinde".to_string(),
            registration_open: true,
            password_login: true,
            bootstrap_admin_email: Some("admin@example.org".to_string()),
            bootstrap_admin_password: Some("admin-password".to_string()),
            max_upload_bytes: 50 * 1024 * 1024,
            session_days: 30,
            sharing_enabled: true,
            oidc_providers: Vec::new(),
            mail: MailConfig::Off,
            storage: StorageConfig::Directory,
            keep_ended_sessions_days: 7,
            idle_session_days: 30,
        }
    }
}
