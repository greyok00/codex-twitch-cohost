use tracing::error;

use crate::{config::AppConfig, error::{AppError, AppResult}};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeviceCodeStart {
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
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
    pub login: String,
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
