use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub twitch: TwitchConfig,
    pub providers: ProviderSection,
    pub personality_path: String,
    pub voice: VoiceConfig,
    pub memory: MemoryConfig,
    pub moderation: ModerationConfig,
    pub search: SearchConfig,
    pub browser: BrowserConfig,
    pub behavior: BehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitchConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub bot_username: String,
    pub channel: String,
    pub bot_token: Option<String>,
    pub redirect_url: String,
    pub scopes: Vec<String>,
    pub broadcaster_login: Option<String>,
    pub use_eventsub: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSection {
    pub primary: ProviderConfig,
    pub fallbacks: Vec<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout_ms: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub voice_name: Option<String>,
    #[serde(default)]
    pub volume_percent: Option<u8>,
    pub piper_binary_path: Option<String>,
    pub piper_model_path: Option<String>,
    pub piper_config_path: Option<String>,
    pub speech_rate: Option<i32>,
    pub allow_mic_commands: bool,
    pub stt_enabled: bool,
    pub stt_binary_path: Option<String>,
    pub stt_model_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub persist: bool,
    pub max_recent_messages: usize,
    pub store_viewer_facts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationConfig {
    pub blocked_phrases: Vec<String>,
    pub minimum_reply_interval_ms: u64,
    pub max_reply_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub provider: String,
    pub api_key: Option<String>,
    pub max_results: usize,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub allow_open_url: bool,
    pub require_explicit_open_command: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub proactive_event_replies: bool,
    pub cohost_mode: bool,
    pub auto_greet: bool,
    pub scheduled_messages_minutes: Option<u64>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            twitch: TwitchConfig {
                client_id: String::new(),
                client_secret: None,
                bot_username: String::new(),
                channel: String::new(),
                bot_token: None,
                redirect_url: "http://127.0.0.1:37219/callback".to_string(),
                scopes: vec![
                    "chat:read".to_string(),
                    "chat:edit".to_string(),
                    "moderator:read:followers".to_string(),
                    "channel:read:subscriptions".to_string(),
                    "channel:manage:redemptions".to_string(),
                ],
                broadcaster_login: None,
                use_eventsub: true,
            },
            providers: ProviderSection {
                primary: ProviderConfig {
                    name: "local-ollama".to_string(),
                    base_url: "http://127.0.0.1:11434".to_string(),
                    model: "llama3.1:8b-instruct".to_string(),
                    api_key: None,
                    timeout_ms: 6000,
                    enabled: true,
                },
                fallbacks: vec![ProviderConfig {
                    name: "ollama-cloud".to_string(),
                    base_url: "https://ollama.com".to_string(),
                    model: "llama3.1:8b-instruct".to_string(),
                    api_key: None,
                    timeout_ms: 10000,
                    enabled: false,
                }],
            },
            personality_path: "./personality.json".to_string(),
            voice: VoiceConfig {
                enabled: false,
                voice_name: Some("en_US-lessac-medium".to_string()),
                volume_percent: Some(100),
                piper_binary_path: None,
                piper_model_path: None,
                piper_config_path: None,
                speech_rate: Some(175),
                allow_mic_commands: false,
                stt_enabled: false,
                stt_binary_path: Some("whisper-cli".to_string()),
                stt_model_path: None,
            },
            memory: MemoryConfig {
                persist: true,
                max_recent_messages: 40,
                store_viewer_facts: true,
            },
            moderation: ModerationConfig {
                blocked_phrases: vec![],
                minimum_reply_interval_ms: 3500,
                max_reply_chars: 400,
            },
            search: SearchConfig {
                provider: "duckduckgo".to_string(),
                api_key: None,
                max_results: 3,
                enabled: false,
            },
            browser: BrowserConfig {
                allow_open_url: true,
                require_explicit_open_command: true,
            },
            behavior: BehaviorConfig {
                proactive_event_replies: true,
                cohost_mode: true,
                auto_greet: true,
                scheduled_messages_minutes: Some(15),
            },
        }
    }
}

impl AppConfig {
    fn preferred_config_path() -> PathBuf {
        let root = PathBuf::from("../config.json");
        if root.exists() {
            return root;
        }
        PathBuf::from("./config.json")
    }

    pub fn load() -> AppResult<Self> {
        let config_path = Self::preferred_config_path();
        if !config_path.exists() {
            return Err(AppError::Config(
                "config.json not found. Copy config.example.json to config.json and set Twitch client_id/channel/bot_username.".to_string(),
            ));
        }

        let raw = fs::read_to_string(&config_path)
            .map_err(|e| AppError::Config(format!("failed reading {}: {e}", config_path.display())))?;
        let cfg: Self = serde_json::from_str(&raw).map_err(|e| {
            AppError::Config(format!("invalid JSON in {}: {e}", config_path.display()))
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save_to_disk(&self) -> AppResult<()> {
        let rendered = serde_json::to_string_pretty(self)?;
        let target = Self::preferred_config_path();
        fs::write(&target, rendered)
            .map_err(|e| AppError::Config(format!("failed writing {}: {e}", target.display())))
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.twitch.client_id.trim().is_empty()
            || self.twitch.client_id == "your_twitch_client_id"
            || self.twitch.client_id == "replace_client_id"
        {
            return Err(AppError::Config(
                "twitch.client_id must be set in config.json".to_string(),
            ));
        }
        if self.providers.primary.base_url.trim().is_empty() {
            return Err(AppError::Config(
                "providers.primary.base_url is required".to_string(),
            ));
        }
        if self.providers.primary.model.trim().is_empty() {
            return Err(AppError::Config(
                "providers.primary.model is required".to_string(),
            ));
        }
        if self.moderation.max_reply_chars < 50 {
            return Err(AppError::Config(
                "moderation.max_reply_chars must be at least 50".to_string(),
            ));
        }
        Ok(())
    }
}
