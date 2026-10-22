//! "Sign in with Microsoft" for Outlook.com / Hotmail, which no longer take
//! passwords from mail apps. OAuth 2.0 authorization code + PKCE, public
//! client (no secret), redirect to a one-shot listener on 127.0.0.1. The
//! keychain holds the refresh token (as `oauth:ms:<token>`); access tokens
//! live in memory and are renewed before they expire.

use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One provider's OAuth 2.0 setup: where to sign in, what to ask for, and
/// Hysj's registration there (a public client: the id is not a secret).
#[derive(Debug)]
pub struct OAuthConfig {
    /// In the keychain entry: `oauth:<key>:<refresh token>`.
    pub key: &'static str,
    /// "Sign in with Microsoft".
    pub name: &'static str,
    pub authorize: &'static str,
    pub token: &'static str,
    pub scopes: &'static str,
    pub client_id: &'static str,
}

/// Every OAuth provider Hysj knows (see `provider.rs` for who uses which).
pub const CONFIGS: &[&OAuthConfig] = &[&crate::provider::MICROSOFT_OAUTH];

/// The keychain entry for a refresh token.
pub fn secret(cfg: &OAuthConfig, refresh: &str) -> String {
    format!("oauth:{}:{refresh}", cfg.key)
}

/// A keychain entry that holds an OAuth refresh token: its provider and the
/// token. `None` for a password.
pub fn parse_secret(secret: &str) -> Option<(&'static OAuthConfig, &str)> {
    let rest = secret.strip_prefix("oauth:")?;
    let (key, token) = rest.split_once(':')?;
    CONFIGS.iter().find(|c| c.key == key).map(|c| (*c, token))
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("Hysj isn't registered with this provider yet (no client id in this build)")]
    NotRegistered,
    #[error("sign-in wasn't finished in the browser")]
    Abandoned,
    #[error("Microsoft said no: {0}")]
    Denied(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Http(#[from] reqwest::Error),
}

/// A sign-in in progress: the page to open, and the listener it returns to.
pub struct Pending {
    pub cfg: &'static OAuthConfig,
    pub url: String,
    listener: TcpListener,
    redirect: String,
    verifier: String,
    state: String,
}

fn random(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    getrandom::getrandom(&mut b).expect("the OS random source");
    URL_SAFE_NO_PAD.encode(b)
}

/// Starts a sign-in for `address`: open `url` in the browser, then [`finish`].
pub async fn start(cfg: &'static OAuthConfig, address: &str) -> Result<Pending, OAuthError> {
    if cfg.client_id.is_empty() {
        return Err(OAuthError::NotRegistered);
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let redirect = format!("http://localhost:{}", listener.local_addr()?.port());
    let verifier = random(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random(16);
    let url = reqwest::Url::parse_with_params(
        cfg.authorize,
        &[
            ("client_id", cfg.client_id),
            ("response_type", "code"),
            ("redirect_uri", &redirect),
            ("scope", cfg.scopes),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("login_hint", address),
            ("prompt", "select_account"),
        ],
    )
    .expect("a valid URL")
    .to_string();
    Ok(Pending {
        cfg,
        url,
        listener,
        redirect,
        verifier,
        state,
    })
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Debug, Clone)]
pub struct Tokens {
    pub access: String,
    /// Microsoft sends a new one with each renewal; keep the newest.
    pub refresh: Option<String>,
    pub expires: Instant,
}

impl Tokens {
    /// Still good for a while (renew five minutes early).
    pub fn fresh(&self) -> bool {
        self.expires > Instant::now() + Duration::from_secs(300)
    }
}

async fn token(cfg: &OAuthConfig, form: &[(&str, &str)]) -> Result<Tokens, OAuthError> {
    let resp = reqwest::Client::new()
        .post(cfg.token)
        .form(form)
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        #[derive(Deserialize)]
        struct Failed {
            error_description: Option<String>,
            error: Option<String>,
        }
        let why = serde_json::from_str::<Failed>(&body)
            .ok()
            .and_then(|f| f.error_description.or(f.error))
            .unwrap_or(body);
        // First line only: the rest is trace ids.
        return Err(OAuthError::Denied(
            why.lines().next().unwrap_or_default().to_string(),
        ));
    }
    let t: TokenResponse = resp.json().await?;
    Ok(Tokens {
        access: t.access_token,
        refresh: t.refresh_token,
        expires: Instant::now() + Duration::from_secs(t.expires_in),
    })
}

/// Waits for the browser to come back to the listener with the code (and
/// the `state` this sign-in sent); answers the browser with a short page.
async fn wait_for_code(
    listener: &TcpListener,
    expected: &str,
    within: Duration,
) -> Result<String, OAuthError> {
    tokio::time::timeout(within, async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await?;
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req.split_whitespace().nth(1).unwrap_or("/");
            let url = reqwest::Url::parse(&format!("http://localhost{path}")).expect("a path");
            let get = |k: &str| {
                url.query_pairs()
                    .find(|(name, _)| name == k)
                    .map(|(_, v)| v.into_owned())
            };
            // Browsers also ask for /favicon.ico; only the redirect counts.
            let (Some(state), code, error) = (get("state"), get("code"), get("error_description"))
            else {
                let _ = sock
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .await;
                continue;
            };
            let ok = state == expected && code.is_some();
            let page = if ok {
                "Signed in. You can close this tab and go back to Hysj."
            } else {
                "Sign-in didn't finish. Go back to Hysj and try again."
            };
            let html = format!(
                "<!doctype html><meta charset=utf-8><title>Hysj</title><body style=\"font:17px system-ui;margin:20vh auto;max-width:28em;text-align:center\"><h1>Hysj!</h1><p>{page}</p>"
            );
            let _ = sock
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                        html.len()
                    )
                    .as_bytes(),
                )
                .await;
            return match (ok, code, error) {
                (true, Some(code), _) => Ok(code),
                (_, _, Some(e)) => Err(OAuthError::Denied(e)),
                _ => Err(OAuthError::Abandoned),
            };
        }
    })
    .await
    .map_err(|_| OAuthError::Abandoned)?
}

/// Waits (up to ten minutes) for the browser to come back, then trades the
/// code for tokens.
pub async fn finish(p: Pending) -> Result<Tokens, OAuthError> {
    let code = wait_for_code(&p.listener, &p.state, Duration::from_secs(600)).await?;

    token(
        p.cfg,
        &[
            ("client_id", p.cfg.client_id),
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &p.redirect),
            ("code_verifier", &p.verifier),
            ("scope", p.cfg.scopes),
        ],
    )
    .await
}

/// A new access token from a refresh token.
pub async fn refresh(cfg: &OAuthConfig, refresh_token: &str) -> Result<Tokens, OAuthError> {
    token(
        cfg,
        &[
            ("client_id", cfg.client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", cfg.scopes),
        ],
    )
    .await
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn the_browser_comes_back_with_the_code() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = super::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let browser = |path: &'static str| async move {
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut page = String::new();
            let _ = s.read_to_string(&mut page).await;
            page
        };
        let wait = super::wait_for_code(&listener, "st4te", std::time::Duration::from_secs(5));
        let (code, _, page) = tokio::join!(wait, async {}, async {
            // A favicon request first, then the redirect.
            browser("/favicon.ico").await;
            browser("/?code=C0DE&state=st4te").await
        });
        assert_eq!(code.unwrap(), "C0DE");
        assert!(page.contains("Signed in"));

        let wait = super::wait_for_code(&listener, "st4te", std::time::Duration::from_secs(5));
        let (code, _) = tokio::join!(wait, browser("/?code=C0DE&state=forged"));
        assert!(code.is_err(), "a different state is not this sign-in");
    }

    #[test]
    fn keychain_entries() {
        let cfg = &crate::provider::MICROSOFT_OAUTH;
        let s = super::secret(cfg, "R:T");
        assert_eq!(
            s, "oauth:ms:R:T",
            "entries from before the registry still read"
        );
        let (c, t) = super::parse_secret(&s).unwrap();
        assert_eq!((c.key, t), ("ms", "R:T"));
        assert!(super::parse_secret("hunter2").is_none(), "a password");
        assert!(
            super::parse_secret("oauth:nope:x").is_none(),
            "an unknown provider"
        );
    }
}
