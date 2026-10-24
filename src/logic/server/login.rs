//! Logging in to a server from the desktop.
//!
//! The password — or the organisation's single sign-on — is typed into the
//! server's own page in the system browser, never into this program: OAuth
//! 2.0 for native apps (RFC 8252), with PKCE. This program opens a port on
//! the loopback interface for a moment, sends the browser to the server's
//! `/authorize` page with that port as the way back, and waits for the code.
//! The code, together with the verifier only this program knows, buys a token
//! (`POST /auth/token`), which is kept in the keyring.

use base64::Engine;
use cantara_api::{DeviceAuthorization, TokenRequest, DESKTOP_CLIENT_ID};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use super::{normalise_url, secrets, Client, ClientError, ServerInstance};

/// How long the browser has to come back.
const WAIT: std::time::Duration = std::time::Duration::from_secs(300);

/// What a login brings: the instance, acting in the first organisation the
/// account belongs to. Where there are several, the settings offer the
/// others.
pub struct LoggedIn {
    pub instance: ServerInstance,
}

/// A verifier (RFC 7636: 43 to 128 unreserved characters) and its challenge.
fn pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn device_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|name| !name.trim().is_empty())
        .map(|name| format!("Cantara ({name})"))
        .unwrap_or_else(|| "Cantara".to_string())
}

/// The query of the request line `GET /callback?code=…&state=… HTTP/1.1`.
fn query_of(request: &str) -> Vec<(String, String)> {
    let target = request.split_whitespace().nth(1).unwrap_or_default();
    let query = target.split_once('?').map(|(_, query)| query).unwrap_or_default();
    url_pairs(query)
}

fn url_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (decode(key), decode(value)))
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => out.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                match u8::from_str_radix(&text[index + 1..index + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Cantara</title>\
<body style=\"font-family:system-ui;display:grid;place-items:center;min-height:90vh\">\
<p>Cantara ist verbunden. Dieses Fenster kann geschlossen werden.<br>\
Cantara is connected. You can close this window.</p>";

/// Logs in to the server at `address`, through the system browser.
pub async fn login(address: &str) -> Result<LoggedIn, String> {
    login_with(address, |url| {
        webbrowser::open(url).map_err(|error| format!("the browser cannot be opened: {error}"))
    })
    .await
}

/// Logs in to the server at `address`, with `open` sending somebody to the
/// server's page. The system browser in [`login`]; the browser a test drives
/// in the harness.
pub async fn login_with(address: &str, open: impl FnOnce(&str) -> Result<(), String>) -> Result<LoggedIn, String> {
    let url = normalise_url(address).ok_or("that is not the address of a server")?;
    let anonymous = Client::new(&url, None);
    let info = anonymous
        .info()
        .await
        .map_err(|error| format!("{url} does not answer as a Cantara server: {error}"))?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let (verifier, challenge) = pkce_pair();
    let state = Uuid::new_v4().simple().to_string();
    let device = device_name();

    let authorize = format!(
        "{url}/authorize?client_id={}&redirect_uri={}&code_challenge={challenge}&code_challenge_method=S256&state={state}&device_name={}",
        encode(DESKTOP_CLIENT_ID),
        encode(&redirect),
        encode(&device),
    );
    open(&authorize)?;

    // The first request that arrives with our state is the answer. Anything
    // else knocking on the port (a browser asking for a favicon) is told no
    // and the wait goes on.
    let code = tokio::time::timeout(WAIT, async {
        loop {
            let (mut stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
            let mut buffer = vec![0u8; 8192];
            let read = stream.read(&mut buffer).await.map_err(|error| error.to_string())?;
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let pairs = query_of(&request);
            let get = |key: &str| pairs.iter().find(|(name, _)| name == key).map(|(_, value)| value.clone());
            if get("state").as_deref() != Some(state.as_str()) {
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
                continue;
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{DONE_PAGE}",
                DONE_PAGE.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            return match (get("code"), get("error")) {
                (Some(code), _) => Ok(code),
                (None, Some(error)) if error == "access_denied" => Err("the login was declined".to_string()),
                (None, error) => Err(format!("the server sent no code ({})", error.unwrap_or_default())),
            };
        }
    })
    .await
    .map_err(|_| "the login was not finished in time".to_string())??;

    let token = anonymous
        .exchange_code(&TokenRequest {
            code,
            code_verifier: verifier,
            redirect_uri: redirect,
            device_name: device,
        })
        .await
        .map_err(|error| error.to_string())?;
    logged_in(&url, info, token.access_token).await
}

/// Logs in to the server at `address` with a code typed in elsewhere — for a
/// device whose browser cannot come back to it (RFC 8628). `show` is told the
/// code and where to type it in, and is shown it until the user has done so.
pub async fn login_with_code(address: &str, show: impl FnOnce(&DeviceAuthorization)) -> Result<LoggedIn, String> {
    let url = normalise_url(address).ok_or("that is not the address of a server")?;
    let anonymous = Client::new(&url, None);
    let info = anonymous
        .info()
        .await
        .map_err(|error| format!("{url} does not answer as a Cantara server: {error}"))?;
    let authorization = anonymous
        .device_authorize(&device_name())
        .await
        .map_err(|error| error.to_string())?;
    show(&authorization);

    let mut interval = authorization.interval.max(1) as u64;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(authorization.expires_in.max(1) as u64);
    loop {
        crate::logic::timer::sleep(std::time::Duration::from_secs(interval)).await;
        if std::time::Instant::now() > deadline {
            return Err("the code has expired".to_string());
        }
        match anonymous.device_token(&authorization.device_code).await {
            Ok(token) => return logged_in(&url, info, token.access_token).await,
            Err(ClientError::Refused { error: Some(error), .. }) => match error.code.as_str() {
                "authorization_pending" => {}
                "slow_down" => interval += 5,
                "access_denied" => return Err("the login was declined".to_string()),
                _ => return Err(error.message),
            },
            // Offline for a moment: keep asking until the code expires.
            Err(ClientError::Unreachable(_)) => {}
            Err(other) => return Err(other.to_string()),
        }
    }
}

/// What a token buys: the instance, with the first organisation chosen.
async fn logged_in(url: &str, info: cantara_api::ServerInfo, token: String) -> Result<LoggedIn, String> {
    let client = Client::new(url, Some(token.clone()));
    let me = client.me().await.map_err(|error| error.to_string())?;
    let uuid = Uuid::new_v4();
    let kept = secrets::store(uuid, &token);
    let first = me.tenants.first();
    Ok(LoggedIn {
        instance: ServerInstance {
            uuid,
            url: url.to_string(),
            name: info.name,
            tenant: first.map(|membership| membership.tenant.uuid),
            tenant_name: first.map(|membership| membership.tenant.name.clone()).unwrap_or_default(),
            user_email: me.user.email,
            user_name: me.user.display_name,
            sharing: info.sharing_enabled,
            token_fallback: (!kept).then_some(token),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_callback_is_read_from_the_request_line() {
        let pairs = query_of("GET /callback?code=abc%2Bdef&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
        assert_eq!(pairs, vec![("code".into(), "abc+def".into()), ("state".into(), "xyz".into())]);
    }

    #[test]
    fn a_pkce_pair_fits_the_rules() {
        let (verifier, challenge) = pkce_pair();
        assert!((43..=128).contains(&verifier.len()));
        assert_eq!(challenge.len(), 43);
        assert_eq!(encode("http://127.0.0.1:5/callback"), "http%3A%2F%2F127.0.0.1%3A5%2Fcallback");
    }
}
