use std::collections::HashMap;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};
use tracing::{error, info};
use url::Url;

use crate::{config::AppConfig, error::{AppError, AppResult}};

#[derive(Debug, Clone)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeviceCodeStart {
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
    pub user_code: String,
    pub verification_uri: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct HelixUsersResponse {
    pub data: Vec<TwitchUser>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TwitchUser {
    pub id: String,
    pub login: String,
    pub display_name: String,
}

pub fn generate_pkce_pair() -> AppResult<PkcePair> {
    let mut raw = [0_u8; 32];
    OsRng.fill_bytes(&mut raw);
    let verifier = URL_SAFE_NO_PAD.encode(raw);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Ok(PkcePair { verifier, challenge })
}

pub fn build_authorize_url(config: &AppConfig, csrf_state: &str, pkce: &PkcePair) -> AppResult<String> {
    let mut url = Url::parse("https://id.twitch.tv/oauth2/authorize")
        .map_err(|e| AppError::Auth(format!("failed to build auth URL: {e}")))?;

    let scopes = config.twitch.scopes.join(" ");
    url.query_pairs_mut()
        .append_pair("client_id", &config.twitch.client_id)
        .append_pair("redirect_uri", &config.twitch.redirect_url)
        .append_pair("response_type", "code")
        .append_pair("scope", &scopes)
        .append_pair("state", csrf_state)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("force_verify", "true");

    Ok(url.to_string())
}

pub async fn wait_for_oauth_code(redirect_url: &str, expected_state: &str) -> AppResult<String> {
    let parsed = Url::parse(redirect_url)
        .map_err(|e| AppError::Auth(format!("invalid redirect URL in config: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::Auth("redirect URL host missing".to_string()))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| AppError::Auth("redirect URL port missing".to_string()))?;
    let path = parsed.path().to_string();

    let listener = TcpListener::bind(format!("{}:{}", host, port))
        .await
        .map_err(|e| AppError::Auth(format!("failed binding local OAuth listener: {e}")))?;
    info!("oauth listener ready on {}:{}", host, port);

    let (mut socket, _) = listener
        .accept()
        .await
        .map_err(|e| AppError::Auth(format!("OAuth listener accept failed: {e}")))?;

    let mut buf = vec![0_u8; 4096];
    let n = socket
        .read(&mut buf)
        .await
        .map_err(|e| AppError::Auth(format!("failed reading OAuth callback request: {e}")))?;

    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| AppError::Auth("invalid OAuth callback request".to_string()))?;

    let target = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| AppError::Auth("missing callback path".to_string()))?;

    if !target.starts_with(&path) {
        let _ = socket
            .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nInvalid callback path")
            .await;
        return Err(AppError::Auth("OAuth callback path mismatch".to_string()));
    }

    let full_url = format!("http://{}:{}{}", host, port, target);
    let callback = Url::parse(&full_url)
        .map_err(|e| AppError::Auth(format!("failed parsing callback URL: {e}")))?;

    let query: HashMap<_, _> = callback.query_pairs().into_owned().collect();
    if let Some(err) = query.get("error") {
        let _ = socket
            .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nOAuth denied")
            .await;
        return Err(AppError::Auth(format!("OAuth denied: {err}")));
    }

    match query.get("state") {
        Some(value) if value == expected_state => {}
        _ => {
            let _ = socket
                .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\nOAuth state mismatch")
                .await;
            return Err(AppError::Auth("OAuth state mismatch".to_string()));
        }
    }

    let code = query
        .get("code")
        .ok_or_else(|| AppError::Auth("OAuth code missing".to_string()))?
        .to_string();

    let _ = socket
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<h2>Twitch connection complete</h2><p>You can close this tab and return to the app.</p>",
        )
        .await;
    Ok(code)
}

pub async fn exchange_code_for_token(
    config: &AppConfig,
    code: &str,
    client_secret: Option<&str>,
    code_verifier: &str,
) -> AppResult<String> {
    let client = reqwest::Client::new();
    let mut params = vec![
        ("client_id", config.twitch.client_id.clone()),
        ("code", code.to_string()),
        ("grant_type", "authorization_code".to_string()),
        ("redirect_uri", config.twitch.redirect_url.clone()),
        ("code_verifier", code_verifier.to_string()),
    ];
    if let Some(secret) = client_secret {
        if !secret.trim().is_empty() {
            params.push(("client_secret", secret.to_string()));
        }
    }
    let response = client
        .post("https://id.twitch.tv/oauth2/token")
        .form(&params)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_else(|_| "<empty>".to_string());
        error!("oauth token exchange failed: {}", text);
        return Err(AppError::Auth(format!(
            "token exchange failed with status {}",
            status
        )));
    }

    let payload: serde_json::Value = response.json().await?;
    let token = payload
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Auth("access_token missing from Twitch response".to_string()))?;

    Ok(token.to_string())
}

pub async fn start_device_code_flow(config: &AppConfig) -> AppResult<DeviceCodeStart> {
    let client = reqwest::Client::new();
    let scope_str = config.twitch.scopes.join(" ");
    let response = client
        .post("https://id.twitch.tv/oauth2/device")
        .form(&[
            ("client_id", config.twitch.client_id.as_str()),
            ("scopes", scope_str.as_str()),
        ])
        .send()
        .await?;

    if response.status().is_success() {
        return response
            .json::<DeviceCodeStart>()
            .await
            .map_err(AppError::from);
    }

    let first_status = response.status();
    let first_body = response.text().await.unwrap_or_else(|_| "<empty>".to_string());
    error!(
        "device code start failed (scopes param): status={}, body={}",
        first_status, first_body
    );

    let retry = client
        .post("https://id.twitch.tv/oauth2/device")
        .form(&[
            ("client_id", config.twitch.client_id.as_str()),
            ("scope", scope_str.as_str()),
        ])
        .send()
        .await?;

    if retry.status().is_success() {
        return retry
            .json::<DeviceCodeStart>()
            .await
            .map_err(AppError::from);
    }

    let retry_status = retry.status();
    let retry_body = retry.text().await.unwrap_or_else(|_| "<empty>".to_string());
    error!(
        "device code start failed (scope param): status={}, body={}",
        retry_status, retry_body
    );
    Err(AppError::Auth(format!(
        "device code start failed. scopes-status={} body={}; scope-status={} body={}",
        first_status, first_body, retry_status, retry_body
    )))
}

pub async fn poll_device_code_for_token(
    config: &AppConfig,
    device_code: &str,
    interval_secs: u64,
    expires_in_secs: u64,
) -> AppResult<OAuthTokenResponse> {
    let client = reqwest::Client::new();
    let scope_str = config.twitch.scopes.join(" ");
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(expires_in_secs);
    let wait = interval_secs.max(2);

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(AppError::Auth("device code expired before authorization completed".to_string()));
        }

        let response = client
            .post("https://id.twitch.tv/oauth2/token")
            .form(&[
                ("client_id", config.twitch.client_id.as_str()),
                ("scopes", scope_str.as_str()),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?;

        if response.status().is_success() {
            return response
                .json::<OAuthTokenResponse>()
                .await
                .map_err(AppError::from);
        }

        let status = response.status();
        let text = response.text().await.unwrap_or_else(|_| "<empty>".to_string());
        let body: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let err_code = body.get("message").and_then(|v| v.as_str()).unwrap_or("");

        if status.as_u16() == 400 && (err_code == "authorization_pending" || err_code == "slow_down") {
            tokio::time::sleep(tokio::time::Duration::from_secs(wait)).await;
            continue;
        }

        return Err(AppError::Auth(format!(
            "device token exchange failed with status {}: {}",
            status, text
        )));
    }
}

pub async fn refresh_access_token(
    client_id: &str,
    refresh_token: &str,
    client_secret: Option<&str>,
) -> AppResult<OAuthTokenResponse> {
    let client = reqwest::Client::new();
    let mut params = vec![
        ("client_id", client_id.to_string()),
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
    ];
    if let Some(secret) = client_secret {
        if !secret.trim().is_empty() {
            params.push(("client_secret", secret.to_string()));
        }
    }

    let response = client
        .post("https://id.twitch.tv/oauth2/token")
        .form(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_else(|_| "<empty>".to_string());
        return Err(AppError::Auth(format!(
            "refresh token exchange failed with status {}: {}",
            status, text
        )));
    }

    response
        .json::<OAuthTokenResponse>()
        .await
        .map_err(AppError::from)
}

pub async fn validate_access_token(access_token: &str) -> AppResult<bool> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://id.twitch.tv/oauth2/validate")
        .bearer_auth(access_token)
        .send()
        .await?;

    if response.status().as_u16() == 200 {
        return Ok(true);
    }
    if response.status().as_u16() == 401 {
        return Ok(false);
    }

    Err(AppError::Auth(format!(
        "token validation failed with status {}",
        response.status()
    )))
}

pub async fn fetch_current_user(client_id: &str, access_token: &str) -> AppResult<TwitchUser> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.twitch.tv/helix/users")
        .header("Client-Id", client_id)
        .bearer_auth(access_token)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "<empty>".to_string());
        return Err(AppError::Auth(format!(
            "failed to fetch authenticated Twitch user (status {}): {}",
            status, body
        )));
    }

    let payload = response.json::<HelixUsersResponse>().await?;
    payload
        .data
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Auth("authenticated user not found in Helix response".to_string()))
}
