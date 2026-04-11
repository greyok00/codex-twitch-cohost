use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

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
    fn local_config_candidates() -> Vec<PathBuf> {
        vec![PathBuf::from("../config.json"), PathBuf::from("./config.json")]
    }

    fn user_config_path() -> PathBuf {
        if let Some(explicit) = env::var_os("TWITCH_COHOST_CONFIG_DIR") {
            return PathBuf::from(explicit).join("config.json");
        }
        if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("twitch-cohost-bot").join("config.json");
        }
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("twitch-cohost-bot")
                .join("config.json");
        }
        PathBuf::from("./config.json")
    }

    fn ensure_parent_dir(path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::Config(format!("failed creating config dir {}: {e}", parent.display()))
            })?;
        }
        Ok(())
    }

    fn is_writable_target(path: &Path) -> bool {
        if path.exists() {
            return OpenOptions::new().write(true).append(true).open(path).is_ok();
        }
        let Some(parent) = path.parent() else {
            return false;
        };
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
        let probe = parent.join(format!(".write_probe_{}", std::process::id()));
        match OpenOptions::new().create_new(true).write(true).open(&probe) {
            Ok(mut f) => {
                let _ = f.write_all(b"ok");
                let _ = fs::remove_file(probe);
                true
            }
            Err(_) => false,
        }
    }

    fn read_candidates() -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        let user = Self::user_config_path();
        if user.exists() {
            candidates.push(user);
        }
        for local in Self::local_config_candidates() {
            if local.exists() {
                candidates.push(local);
            }
        }
        if candidates.is_empty() {
            candidates.push(Self::user_config_path());
        }
        candidates
    }

    fn write_target_path() -> PathBuf {
        for local in Self::local_config_candidates() {
            if Self::is_writable_target(&local) {
                return local;
            }
        }
        Self::user_config_path()
    }

    pub fn load() -> AppResult<Self> {
        for config_path in Self::read_candidates() {
            if !config_path.exists() {
                continue;
            }
            let raw = fs::read_to_string(&config_path).map_err(|e| {
                AppError::Config(format!("failed reading {}: {e}", config_path.display()))
            })?;
            let cfg: Self = serde_json::from_str(&raw).map_err(|e| {
                AppError::Config(format!("invalid JSON in {}: {e}", config_path.display()))
            })?;
            cfg.validate()?;
            return Ok(cfg);
        }

        Ok(Self::default())
    }

    pub fn save_to_disk(&self) -> AppResult<()> {
        // Never persist secrets to config.json; keep them in keychain only.
        let mut safe = self.clone();
        safe.twitch.client_secret = None;
        safe.twitch.bot_token = None;
        safe.providers.primary.api_key = None;
        for fallback in &mut safe.providers.fallbacks {
            fallback.api_key = None;
        }
        safe.search.api_key = None;
        let rendered = serde_json::to_string_pretty(&safe)?;
        let target = Self::write_target_path();
        Self::ensure_parent_dir(&target)?;
        fs::write(&target, rendered)
            .map_err(|e| AppError::Config(format!("failed writing {}: {e}", target.display())))
    }

    pub fn validate(&self) -> AppResult<()> {
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
