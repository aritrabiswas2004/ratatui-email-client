use std::{
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use color_eyre::eyre::{Context, Result, eyre};
use rand::{Rng, distributions::Alphanumeric, thread_rng};
use reqwest::{Url, blocking::Client};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const APP_DIR: &str = "term-gui";
const TOKEN_FILE: &str = "google-token.json";
const DEFAULT_REDIRECT_PORT: u16 = 8765;

const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/gmail.send",
];

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_port: u16,
}

impl OAuthConfig {
    pub fn from_env() -> Result<Self> {
        let client_id = env::var("GOOGLE_CLIENT_ID")
            .or_else(|_| env::var("GMAIL_CLIENT_ID"))
            .context("GOOGLE_CLIENT_ID (or GMAIL_CLIENT_ID) must be set")?;
        let client_secret = env::var("GOOGLE_CLIENT_SECRET")
            .ok()
            .or_else(|| env::var("GMAIL_CLIENT_SECRET").ok());
        let redirect_port = env::var("GOOGLE_REDIRECT_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_REDIRECT_PORT);

        Ok(Self {
            client_id,
            client_secret,
            redirect_port,
        })
    }

    fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/", self.redirect_port)
    }

    fn scope_string(&self) -> String {
        SCOPES.join(" ")
    }
}

#[derive(Debug, Clone)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
}

impl AuthSession {
    fn into_record(self) -> TokenRecord {
        TokenRecord {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: self.expires_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenRecord {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
}

impl TokenRecord {
    fn into_session(self) -> AuthSession {
        AuthSession {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: self.expires_at,
        }
    }

    fn expires_soon(&self) -> bool {
        match self.expires_at {
            Some(expires_at) => current_epoch_seconds().saturating_add(60) >= expires_at,
            None => false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

pub fn authenticate() -> Result<AuthSession> {
    let config = OAuthConfig::from_env()?;
    let store = TokenStore::new()?;

    if let Some(record) = store.load()? {
        if !record.expires_soon() {
            return Ok(record.into_session());
        }

        if let Some(refresh_token) = record.refresh_token.clone() {
            if let Ok(session) = refresh_access_token(&config, &refresh_token) {
                store.save(&session.clone().into_record())?;
                return Ok(session);
            }
        }
    }

    let session = interactive_login(&config)?;
    store.save(&session.clone().into_record())?;
    Ok(session)
}

fn interactive_login(config: &OAuthConfig) -> Result<AuthSession> {
    let (verifier, challenge) = pkce_pair();
    let state = random_token(32);
    let auth_url = build_authorization_url(config, &state, &challenge)?;
    let listener = TcpListener::bind(("127.0.0.1", config.redirect_port))
        .context("failed to bind the local OAuth callback listener")?;

    launch_browser(&auth_url);

    let code = wait_for_authorization_code(listener, &state)?;
    let token_response = exchange_code(config, &code, &verifier)?;

    Ok(AuthSession {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        expires_at: token_response
            .expires_in
            .map(|seconds| current_epoch_seconds() + seconds),
    })
}

fn build_authorization_url(config: &OAuthConfig, state: &str, challenge: &str) -> Result<String> {
    let redirect_uri = config.redirect_uri();
    let scope = config.scope_string();
    let url = Url::parse_with_params(
        AUTH_ENDPOINT,
        [
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", scope.as_str()),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )?;

    Ok(url.into())
}

fn launch_browser(url: &str) {
    match Command::new("xdg-open").arg(url).spawn() {
        Ok(_) => {}
        Err(err) => {
            eprintln!("Open this URL in your browser to continue sign-in:\n{url}\n({err})");
        }
    }
}

fn wait_for_authorization_code(listener: TcpListener, expected_state: &str) -> Result<String> {
    let (mut stream, _) = listener
        .accept()
        .context("failed waiting for the OAuth redirect")?;
    let mut buffer = [0u8; 8192];
    let bytes_read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| eyre!("OAuth callback request was empty"))?;
    let request_target = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| eyre!("OAuth callback path missing"))?;
    let parsed_url = Url::parse(&format!("http://localhost{request_target}"))?;

    let mut code = None;
    let mut state = None;
    for (key, value) in parsed_url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            _ => {}
        }
    }

    let body = if state.as_deref() == Some(expected_state) {
        "<html><body><h2>Signed in</h2><p>You can close this tab and return to the terminal.</p></body></html>"
    } else {
        "<html><body><h2>Sign-in failed</h2><p>The OAuth state token did not match.</p></body></html>"
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;

    if state.as_deref() != Some(expected_state) {
        return Err(eyre!("OAuth state token mismatch"));
    }

    code.ok_or_else(|| eyre!("authorization code missing from OAuth callback"))
}

fn exchange_code(config: &OAuthConfig, code: &str, verifier: &str) -> Result<TokenResponse> {
    let client = Client::new();
    let redirect_uri = config.redirect_uri();
    let mut params = vec![
        ("code", code.to_string()),
        ("client_id", config.client_id.clone()),
        ("code_verifier", verifier.to_string()),
        ("redirect_uri", redirect_uri.clone()),
        ("grant_type", "authorization_code".to_string()),
    ];
    if let Some(secret) = &config.client_secret {
        params.push(("client_secret", secret.clone()));
    }
    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .context("failed to exchange the authorization code")?;

    let status = response.status();
    let body = response
        .text()
        .context("failed to read Google token exchange response body")?;

    if !status.is_success() {
        let secret_hint = if body.contains("client_secret is missing") {
            " For this client, set GOOGLE_CLIENT_SECRET (or create a Desktop OAuth client in Google Cloud)."
        } else {
            ""
        };
        return Err(eyre!(
            "Google rejected the authorization code exchange (status: {}). \
possible causes: wrong OAuth client type (use Desktop app), wrong client_id env var, \
redirect URI mismatch, reused/expired code.{} redirect_uri={}, response_body={}",
            status,
            secret_hint,
            redirect_uri,
            body
        ));
    }

    let parsed = serde_json::from_str::<TokenResponse>(&body)
        .context("failed to parse Google token response JSON body")?;

    Ok(parsed)
}

fn refresh_access_token(config: &OAuthConfig, refresh_token: &str) -> Result<AuthSession> {
    let client = Client::new();
    let mut params = vec![
        ("refresh_token", refresh_token.to_string()),
        ("client_id", config.client_id.clone()),
        ("grant_type", "refresh_token".to_string()),
    ];
    if let Some(secret) = &config.client_secret {
        params.push(("client_secret", secret.clone()));
    }
    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .context("failed to refresh the OAuth access token")?;

    let status = response.status();
    let body = response
        .text()
        .context("failed to read Google refresh response body")?;

    if !status.is_success() {
        return Err(eyre!(
            "Google rejected the refresh token request (status: {}). \
possible causes: revoked/invalid refresh token, wrong client_id env var. response_body={}",
            status,
            body
        ));
    }

    let response = serde_json::from_str::<TokenResponse>(&body)
        .context("failed to parse Google refresh response JSON body")?;

    Ok(AuthSession {
        access_token: response.access_token,
        refresh_token: Some(refresh_token.to_string()),
        expires_at: response
            .expires_in
            .map(|seconds| current_epoch_seconds() + seconds),
    })
}

fn pkce_pair() -> (String, String) {
    let verifier = random_token(96);
    let challenge = {
        let digest = Sha256::digest(verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    };

    (verifier, challenge)
}

fn random_token(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

struct TokenStore {
    path: PathBuf,
}

impl TokenStore {
    fn new() -> Result<Self> {
        Ok(Self {
            path: token_path()?,
        })
    }

    fn load(&self) -> Result<Option<TokenRecord>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let record = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse {}", self.path.display()))?;
        Ok(Some(record))
    }

    fn save(&self, record: &TokenRecord) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(record)?;
        fs::write(&self.path, json)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }
}

fn token_path() -> Result<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| eyre!("unable to determine a config directory"))?;

    Ok(config_home.join(APP_DIR).join(TOKEN_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_pair_generates_expected_lengths() {
        let (verifier, challenge) = pkce_pair();
        assert!(!verifier.is_empty());
        assert!(!challenge.is_empty());
    }

    #[test]
    fn token_store_round_trip() {
        let temp_dir = std::env::temp_dir().join(format!("term-gui-auth-{}", random_token(8)));
        let path = temp_dir.join(TOKEN_FILE);
        let store = TokenStore { path };
        let record = TokenRecord {
            access_token: "access".into(),
            refresh_token: Some("refresh".into()),
            expires_at: Some(1_000_000),
        };

        store.save(&record).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.access_token, "access");
        assert_eq!(loaded.refresh_token.as_deref(), Some("refresh"));
    }
}
