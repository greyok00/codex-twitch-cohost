use serde::{Deserialize, Serialize};
use tokio::time::{timeout, Duration};

use crate::{config::ProviderConfig, error::{AppError, AppResult}};

#[derive(Debug, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatReqMessage<'a>>,
    stream: bool,
    options: OllamaChatOptions,
}

#[derive(Debug, Serialize)]
struct ChatReqMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct OllamaChatOptions {
    temperature: f32,
    top_p: f32,
    repeat_penalty: f32,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: Option<ChatRespMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatRespMessage {
    content: String,
}

#[derive(Clone)]
pub struct LlmService {
    client: reqwest::Client,
}

impl LlmService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn generate(
        &self,
        primary: &ProviderConfig,
        fallbacks: &[ProviderConfig],
        system_prompt: &str,
        user_prompt: &str,
    ) -> AppResult<String> {
        match self.call_provider(primary, system_prompt, user_prompt).await {
            Ok(v) => Ok(v),
            Err(primary_error) => {
                for provider in fallbacks.iter().filter(|p| p.enabled) {
                    if let Ok(v) = self.call_provider(provider, system_prompt, user_prompt).await {
                        return Ok(v);
                    }
                }
                Err(AppError::Provider(format!(
                    "primary provider failed and no fallback succeeded: {primary_error}"
                )))
            }
        }
    }

    async fn call_provider(
        &self,
        provider: &ProviderConfig,
        system_prompt: &str,
        user_prompt: &str,
    ) -> AppResult<String> {
        if !provider.enabled {
            return Err(AppError::Provider(format!(
                "provider {} is disabled",
                provider.name
            )));
        }

        let url = format!("{}/api/chat", provider.base_url.trim_end_matches('/'));
        let req = OllamaChatRequest {
            model: &provider.model,
            messages: vec![
                ChatReqMessage {
                    role: "system",
                    content: system_prompt,
                },
                ChatReqMessage {
                    role: "user",
                    content: user_prompt,
                },
            ],
            stream: false,
            options: OllamaChatOptions {
                temperature: 0.35,
                top_p: 0.9,
                repeat_penalty: 1.22,
            },
        };

        let mut request = self.client.post(url).json(&req);
        if let Some(key) = &provider.api_key {
            request = request.bearer_auth(key);
        }

        let send_future = request.send();
        let response = timeout(Duration::from_millis(provider.timeout_ms), send_future)
            .await
            .map_err(|_| AppError::Provider("provider request timed out".to_string()))??;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|_| "<empty>".to_string());
            return Err(AppError::Provider(format!(
                "provider {} returned {}: {}",
                provider.name,
                status,
                body
            )));
        }

        let payload: OllamaChatResponse = response.json().await?;
        let content = payload
            .message
            .map(|m| m.content)
            .unwrap_or_else(|| "I lost the thread for a second, run that again?".to_string());
        Ok(content)
    }

    pub async fn healthcheck(&self, provider: &ProviderConfig) -> bool {
        let url = format!("{}/api/tags", provider.base_url.trim_end_matches('/'));
        self.client.get(url).send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }
}
