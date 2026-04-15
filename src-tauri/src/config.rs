use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, AppResult},
    personality::engine::PersonalityProfile,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub twitch: TwitchConfig,
    pub providers: ProviderSection,
    #[serde(default)]
    pub personality: PersonalityProfile,
    pub voice: VoiceConfig,
    pub memory: MemoryConfig,
    pub moderation: ModerationConfig,
    pub search: SearchConfig,
    pub browser: BrowserConfig,
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub scene: SceneConfig,
    #[serde(default)]
    pub character_studio: CharacterStudioConfig,
    #[serde(default)]
    pub avatar_rig: AvatarRigConfig,
    #[serde(default)]
    pub public_call: PublicCallConfig,
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
    #[serde(default = "default_post_bot_messages_to_twitch")]
    pub post_bot_messages_to_twitch: bool,
    #[serde(default)]
    pub topic_continuation_mode: bool,
    #[serde(default = "default_reply_length_mode")]
    pub reply_length_mode: String,
    #[serde(default = "default_allow_brief_reactions")]
    pub allow_brief_reactions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneConfig {
    pub mode: String,
    pub max_turns_before_pause: u8,
    pub allow_external_topic_changes: bool,
    pub secondary_character_slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStudioConfig {
    pub selected_preset: String,
    pub warmth: u8,
    pub humor: u8,
    pub flirt: u8,
    pub edge: u8,
    pub energy: u8,
    pub story: u8,
    #[serde(default)]
    pub profanity_allowed: bool,
    pub extra_direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarRigConfig {
    pub mouth_x: i16,
    pub mouth_y: i16,
    pub mouth_width: u16,
    pub mouth_open: u16,
    pub mouth_softness: u16,
    pub mouth_smile: i16,
    pub mouth_tilt: i16,
    pub mouth_color: String,
    #[serde(default)]
    pub brow_x: i16,
    pub brow_y: i16,
    pub brow_spacing: u16,
    pub brow_arch: i16,
    pub brow_tilt: i16,
    pub brow_thickness: u16,
    pub brow_color: String,
    pub eye_open: u16,
    pub eye_squint: u16,
    pub head_tilt: i16,
    pub head_scale: u16,
    pub glow: u16,
    pub popup_width: u16,
    pub popup_height: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PublicCallConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_public_call_token")]
    pub token: String,
    #[serde(default)]
    pub default_character_slug: String,
}

fn default_post_bot_messages_to_twitch() -> bool {
    false
}

fn default_reply_length_mode() -> String {
    "natural".to_string()
}

fn default_allow_brief_reactions() -> bool {
    true
}

fn default_public_call_token() -> String {
    uuid::Uuid::new_v4().to_string()
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
                    model: "llama3.2:3b".to_string(),
                    api_key: None,
                    timeout_ms: 8000,
                    enabled: true,
                },
                fallbacks: vec![ProviderConfig {
                    name: "ollama-cloud".to_string(),
                    base_url: "https://ollama.com".to_string(),
                    model: "qwen3:8b".to_string(),
                    api_key: None,
                    timeout_ms: 18000,
                    enabled: false,
                }],
            },
            personality: PersonalityProfile::default(),
            voice: VoiceConfig {
                enabled: true,
                voice_name: Some("en-US-EmmaNeural".to_string()),
                volume_percent: Some(100),
                piper_binary_path: None,
                piper_model_path: None,
                piper_config_path: None,
                speech_rate: Some(175),
                allow_mic_commands: false,
                stt_enabled: false,
                stt_binary_path: Some("vosk".to_string()),
                stt_model_path: None,
            },
            memory: MemoryConfig {
                persist: true,
                max_recent_messages: 120,
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
                cohost_mode: false,
                auto_greet: true,
                scheduled_messages_minutes: None,
                post_bot_messages_to_twitch: false,
                topic_continuation_mode: false,
                reply_length_mode: default_reply_length_mode(),
                allow_brief_reactions: default_allow_brief_reactions(),
            },
            scene: SceneConfig::default(),
            character_studio: CharacterStudioConfig::default(),
            avatar_rig: AvatarRigConfig::default(),
            public_call: PublicCallConfig {
                enabled: false,
                token: default_public_call_token(),
                default_character_slug: "default".to_string(),
            },
        }
    }
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self {
            mode: "solo".to_string(),
            max_turns_before_pause: 2,
            allow_external_topic_changes: true,
            secondary_character_slug: String::new(),
        }
    }
}

impl Default for CharacterStudioConfig {
    fn default() -> Self {
        Self {
            selected_preset: "guy".to_string(),
            warmth: 55,
            humor: 35,
            flirt: 10,
            edge: 15,
            energy: 60,
            story: 40,
            profanity_allowed: false,
            extra_direction: String::new(),
        }
    }
}

impl Default for AvatarRigConfig {
    fn default() -> Self {
        Self {
            mouth_x: 0,
            mouth_y: 20,
            mouth_width: 32,
            mouth_open: 22,
            mouth_softness: 70,
            mouth_smile: 8,
            mouth_tilt: 0,
            mouth_color: "#7c2d12".to_string(),
            brow_x: 0,
            brow_y: -22,
            brow_spacing: 36,
            brow_arch: 14,
            brow_tilt: 0,
            brow_thickness: 9,
            brow_color: "#2b211f".to_string(),
            eye_open: 62,
            eye_squint: 16,
            head_tilt: 0,
            head_scale: 100,
            glow: 28,
            popup_width: 320,
            popup_height: 420,
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

    fn legacy_personality_path() -> PathBuf {
        Self::user_config_path()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("personality.json")
    }

    fn normalize_runtime_paths(&mut self) {
        self.scene.mode = match self.scene.mode.trim() {
            "dual_debate" => "dual_debate".to_string(),
            "chat_topic" => "chat_topic".to_string(),
            _ => "solo".to_string(),
        };
        self.scene.max_turns_before_pause = self.scene.max_turns_before_pause.clamp(1, 6);
    }

    fn maybe_migrate_legacy_personality(&mut self) {
        let default = PersonalityProfile::default();
        let looks_unset = self.personality.name == default.name
            && self.personality.tone == default.tone
            && self.personality.response_style == default.response_style
            && self.personality.master_prompt_override.is_empty();
        if !looks_unset {
            return;
        }
        let legacy = Self::legacy_personality_path();
        if !legacy.exists() {
            return;
        }
        if let Ok(raw) = fs::read_to_string(&legacy) {
            if let Ok(profile) = serde_json::from_str::<PersonalityProfile>(&raw) {
                self.personality = profile;
            }
        }
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
            let mut cfg: Self = serde_json::from_str(&raw).map_err(|e| {
                AppError::Config(format!("invalid JSON in {}: {e}", config_path.display()))
            })?;
            cfg.normalize_runtime_paths();
            cfg.maybe_migrate_legacy_personality();
            cfg.validate()?;
            return Ok(cfg);
        }

        let mut cfg = Self::default();
        cfg.normalize_runtime_paths();
        cfg.maybe_migrate_legacy_personality();
        Ok(cfg)
    }

    pub fn load_path_for_display() -> String {
        Self::read_candidates()
            .into_iter()
            .find(|p| p.exists())
            .unwrap_or_else(Self::write_target_path)
            .to_string_lossy()
            .to_string()
    }

    pub fn save_to_disk(&self) -> AppResult<()> {
        let target = Self::write_target_path();
        self.save_to_path(&target)
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

    pub fn sanitized_for_disk(&self) -> Self {
        let mut safe = self.clone();
        safe.twitch.client_secret = None;
        safe.twitch.bot_token = None;
        safe.providers.primary.api_key = None;
        for fallback in &mut safe.providers.fallbacks {
            fallback.api_key = None;
        }
        safe.search.api_key = None;
        safe.normalize_runtime_paths();
        safe
    }

    pub fn save_to_path(&self, path: &Path) -> AppResult<()> {
        let rendered = serde_json::to_string_pretty(&self.sanitized_for_disk())?;
        Self::ensure_parent_dir(path)?;
        fs::write(path, rendered)
            .map_err(|e| AppError::Config(format!("failed writing {}: {e}", path.display())))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn load_from_path(path: &Path) -> AppResult<Self> {
        let raw = fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("failed reading {}: {e}", path.display())))?;
        let mut cfg: Self = serde_json::from_str(&raw)
            .map_err(|e| AppError::Config(format!("invalid JSON in {}: {e}", path.display())))?;
        cfg.normalize_runtime_paths();
        cfg.maybe_migrate_legacy_personality();
        cfg.validate()?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn save_to_path_redacts_secrets_and_roundtrips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");

        let mut cfg = AppConfig::default();
        cfg.twitch.client_id = "client-id".to_string();
        cfg.twitch.client_secret = Some("secret".to_string());
        cfg.twitch.bot_token = Some("oauth:token".to_string());
        cfg.providers.primary.api_key = Some("provider-key".to_string());
        cfg.search.api_key = Some("search-key".to_string());

        cfg.save_to_path(&path).expect("save config");
        let raw = std::fs::read_to_string(&path).expect("read config");
        assert!(raw.contains("\"client_id\": \"client-id\""));
        assert!(raw.contains("\"client_secret\": null"));
        assert!(raw.contains("\"bot_token\": null"));
        assert!(raw.contains("\"api_key\": null"));

        let loaded = AppConfig::load_from_path(&path).expect("load config");
        assert_eq!(loaded.twitch.client_id, "client-id");
        assert!(loaded.twitch.client_secret.is_none());
        assert!(loaded.twitch.bot_token.is_none());
        assert!(loaded.providers.primary.api_key.is_none());
        assert!(loaded.search.api_key.is_none());
    }
}
