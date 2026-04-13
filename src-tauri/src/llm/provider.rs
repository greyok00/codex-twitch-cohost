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

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagModel {
    name: String,
}

fn model_size_score(name: &str) -> u32 {
    let lower = name.to_lowercase();
    if let Some(idx) = lower.rfind(':') {
        let tail = &lower[idx + 1..];
        if let Some(num) = tail.strip_suffix('b') {
            let clean = num.split('-').next().unwrap_or(num);
            if let Ok(parsed) = clean.parse::<f32>() {
                return (parsed * 10.0) as u32;
            }
        }
    }
    9_999
}

fn rank_retry_models(models: Vec<String>) -> Vec<String> {
    let mut unique = models
        .into_iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect::<Vec<_>>();
    unique.sort_by(|a, b| {
        model_size_score(a)
            .cmp(&model_size_score(b))
            .then_with(|| a.cmp(b))
    });
    unique.dedup();
    unique
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
        async fn send_chat(
            client: &reqwest::Client,
            provider: &ProviderConfig,
            model: &str,
            system_prompt: &str,
            user_prompt: &str,
        ) -> AppResult<reqwest::Response> {
            let req = OllamaChatRequest {
                model,
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
                    temperature: 0.22,
                    top_p: 0.85,
                    repeat_penalty: 1.28,
                },
            };
            let mut request = client
                .post(format!("{}/api/chat", provider.base_url.trim_end_matches('/')))
                .json(&req);
            if let Some(key) = &provider.api_key {
                request = request.bearer_auth(key);
            }
            timeout(Duration::from_millis(provider.timeout_ms), request.send())
                .await
                .map_err(|_| AppError::Provider("provider request timed out".to_string()))?
                .map_err(AppError::from)
        }

        if !provider.enabled {
            return Err(AppError::Provider(format!(
                "provider {} is disabled",
                provider.name
            )));
        }

        let selected_model = normalize_model_name(&provider.model, &provider.name);
        let response = send_chat(&self.client, provider, &selected_model, system_prompt, user_prompt).await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|_| "<empty>".to_string());
            if status.as_u16() == 404 {
                let tags_url = format!("{}/api/tags", provider.base_url.trim_end_matches('/'));
                let mut tags_request = self.client.get(tags_url);
                if let Some(key) = &provider.api_key {
                    tags_request = tags_request.bearer_auth(key);
                }
                if let Ok(tags_response) = timeout(Duration::from_millis(provider.timeout_ms), tags_request.send())
                    .await
                    .map_err(|_| AppError::Provider("provider request timed out".to_string()))?
                {
                    if tags_response.status().is_success() {
                        if let Ok(tags_payload) = tags_response.json::<OllamaTagsResponse>().await {
                            let models = rank_retry_models(tags_payload
                                .models
                                .into_iter()
                                .map(|m| m.name.trim().to_string())
                                .collect::<Vec<_>>());
                            for model in models.iter().take(5) {
                                let retry_response = send_chat(&self.client, provider, model, system_prompt, user_prompt).await?;
                                if retry_response.status().is_success() {
                                    let payload: OllamaChatResponse = retry_response.json().await?;
                                    let content = payload
                                        .message
                                        .map(|m| m.content)
                                        .unwrap_or_else(|| "I lost the thread for a second, run that again?".to_string());
                                    return Ok(content);
                                }
                            }
                        }
                    }
                }
            }
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
        let mut req = self.client.get(url);
        if let Some(key) = &provider.api_key {
            req = req.bearer_auth(key);
        }
        req.send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }
}

pub(crate) fn normalize_model_name(raw: &str, provider_name: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_lowercase();
    let cloud = provider_name.eq_ignore_ascii_case("ollama-cloud");
    if trimmed.is_empty()
        || lower.contains("qwen2.5vl")
        || lower.contains("mistral-small:24b-instruct")
        || lower.contains("qwen2.5:14b-instruct")
        || (cloud && (lower.contains("llama3.1:8b-instruct") || lower.contains("llama3.3:70b-instruct") || lower.contains("phi4:14b")))
    {
        if cloud {
            "qwen3:8b".to_string()
        } else {
            "llama3.2:3b".to_string()
        }
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{model_size_score, normalize_model_name, rank_retry_models};

    #[test]
    fn normalizes_bad_cloud_models_to_supported_default() {
        assert_eq!(
            normalize_model_name("llama3.3:70b-instruct", "ollama-cloud"),
            "qwen3:8b"
        );
        assert_eq!(
            normalize_model_name("phi4:14b", "ollama-cloud"),
            "qwen3:8b"
        );
        assert_eq!(
            normalize_model_name("qwen2.5vl:latest", "ollama-cloud"),
            "qwen3:8b"
        );
    }

    #[test]
    fn normalizes_bad_local_models_to_local_default() {
        assert_eq!(
            normalize_model_name("qwen2.5vl:latest", "local-ollama"),
            "llama3.2:3b"
        );
    }

    #[test]
    fn preserves_supported_model_names() {
        assert_eq!(
            normalize_model_name("gemma3:12b", "ollama-cloud"),
            "gemma3:12b"
        );
    }

    #[test]
    fn ranks_smaller_retry_models_first() {
        let models = rank_retry_models(vec![
            "gpt-oss:20b".to_string(),
            "llama3.2:3b".to_string(),
            "qwen3:8b".to_string(),
        ]);
        assert_eq!(models[0], "llama3.2:3b");
        assert_eq!(model_size_score("qwen3:8b"), 80);
    }
}
