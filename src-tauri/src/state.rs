use std::{collections::{HashSet, VecDeque}, sync::Arc, time::Instant};

use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::{mpsc, Semaphore};

use crate::{
    config::AppConfig,
    llm::provider::LlmService,
    memory::store::MemoryStore,
    personality::engine::PersonalityProfile,
    security::secret_store::SecretStore,
    search::service::SearchService,
    twitch::eventsub::EventSubService,
    twitch::irc::TwitchIrcService,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub user: String,
    pub content: String,
    pub timestamp: String,
    pub is_bot: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventMessage {
    pub id: String,
    pub kind: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsState {
    pub last_error: Option<String>,
    pub twitch_state: ConnectionState,
    pub provider_state: ConnectionState,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub channel: Option<String>,
    pub model: String,
    pub voice_enabled: bool,
    pub lurk_mode: bool,
    pub twitch_state: ConnectionState,
}

#[derive(Debug, Clone)]
pub enum PipelineInput {
    Chat(ChatMessage),
    LocalChat(ChatMessage),
    Event(EventMessage),
    Manual(String),
}

pub struct SharedState {
    pub config: RwLock<AppConfig>,
    pub personality: RwLock<PersonalityProfile>,
    pub memory: MemoryStore,
    pub llm: LlmService,
    pub search: SearchService,
    pub twitch: TwitchIrcService,
    pub eventsub: EventSubService,
    pub secrets: SecretStore,
    pub diagnostics: RwLock<DiagnosticsState>,
    pub cooldown_until: RwLock<Option<Instant>>,
    pub seen_message_ids: RwLock<HashSet<String>>,
    pub response_queue_tx: mpsc::Sender<PipelineInput>,
    pub recent_chat: RwLock<VecDeque<ChatMessage>>,
    pub recent_event_replies: RwLock<VecDeque<String>>,
    pub recent_bot_replies: RwLock<VecDeque<String>>,
    pub local_prompt_counter: RwLock<u64>,
    pub llm_hiccup_notice_sent: RwLock<bool>,
    pub voice_enabled: RwLock<bool>,
    pub lurk_mode: RwLock<bool>,
    pub local_chat_gate: Arc<Semaphore>,
    pub chat_gate: Arc<Semaphore>,
    pub event_gate: Arc<Semaphore>,
    pub stt_gate: Arc<Semaphore>,
    pub tts_gate: Arc<Semaphore>,
    pub search_gate: Arc<Semaphore>,
    pub summarize_gate: Arc<Semaphore>,
    pub browser_gate: Arc<Semaphore>,
}

impl SharedState {
    pub fn get_status(&self) -> AppStatus {
        let config = self.config.read();
        let diagnostics = self.diagnostics.read();
        AppStatus {
            channel: Some(config.twitch.channel.clone()),
            model: config.providers.primary.model.clone(),
            voice_enabled: *self.voice_enabled.read(),
            lurk_mode: *self.lurk_mode.read(),
            twitch_state: diagnostics.twitch_state.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AppState(pub Arc<SharedState>);
