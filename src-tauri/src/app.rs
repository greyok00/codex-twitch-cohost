use std::{
    collections::{HashSet, VecDeque},
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::RwLock;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::{
    config::AppConfig,
    llm::provider::LlmService,
    memory::store::MemoryStore,
    personality::engine::{PersonalityEngine, PersonalityProfile},
    security::secret_store::SecretStore,
    search::service::SearchService,
    state::{AppState, ChatMessage, ConnectionState, DiagnosticsState, PipelineInput, SharedState},
    twitch::eventsub::{smoke_test_streamer_api, EventSubService},
    twitch::irc::TwitchIrcService,
};

pub fn bootstrap(app: AppHandle) -> Result<AppState, String> {
    let (config, startup_error) = match AppConfig::load() {
        Ok(cfg) => (cfg, None),
        Err(e) => (AppConfig::default(), Some(format!("Config load failed: {e}"))),
    };

    let secrets = SecretStore::new();
    let mut config = config;
    if let Some(secret) = config.twitch.client_secret.take() {
        let _ = secrets.set_twitch_client_secret(&config.twitch.client_id, &secret);
        let _ = config.save_to_disk();
    }

    if let Some(token) = config.twitch.bot_token.take() {
        let _ = secrets.set_twitch_token(&config.twitch.channel, &token);
    }
    if let Some(key) = config.providers.primary.api_key.take() {
        let _ = secrets.set_provider_key(&config.providers.primary.name, &key);
    }
    for provider in &mut config.providers.fallbacks {
        if let Some(key) = provider.api_key.take() {
            let _ = secrets.set_provider_key(&provider.name, &key);
        }
    }

    let profile = PersonalityEngine::load(&config.personality_path)
        .unwrap_or_else(|_| PersonalityProfile::default());
    let memory_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("./data"))
        .join("memory_db");
    let _ = fs::create_dir_all(&memory_dir);

    let memory = match MemoryStore::new(memory_dir.clone()) {
        Ok(store) => store,
        Err(primary_err) => {
            let err_text = primary_err.to_string();
            if err_text.contains("could not acquire lock") || err_text.contains("WouldBlock") {
                let fallback_dir = memory_dir.with_file_name(format!(
                    "memory_db_session_{}",
                    std::process::id()
                ));
                let _ = fs::create_dir_all(&fallback_dir);
                MemoryStore::new(fallback_dir).map_err(|fallback_err| {
                    format!(
                        "Failed initializing memory store (primary lock conflict: {}; fallback failed: {})",
                        primary_err, fallback_err
                    )
                })?
            } else {
                return Err(format!("Failed initializing memory store: {primary_err}"));
            }
        }
    };

    let (queue_tx, queue_rx) = mpsc::channel::<PipelineInput>(512);

    let state = Arc::new(SharedState {
        voice_enabled: RwLock::new(config.voice.enabled),
        lurk_mode: RwLock::new(false),
        config: RwLock::new(config),
        personality: RwLock::new(profile),
        memory,
        llm: LlmService::new(),
        search: SearchService::new(),
        twitch: TwitchIrcService::new(),
        eventsub: EventSubService::new(),
        secrets,
        diagnostics: RwLock::new(DiagnosticsState {
            last_error: startup_error,
            twitch_state: ConnectionState::Disconnected,
            provider_state: ConnectionState::Disconnected,
            uptime_seconds: 0,
        }),
        cooldown_until: RwLock::new(None),
        seen_message_ids: RwLock::new(HashSet::new()),
        response_queue_tx: queue_tx,
        recent_chat: RwLock::new(VecDeque::new()),
        stt_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        search_gate: Arc::new(tokio::sync::Semaphore::new(2)),
        summarize_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        browser_gate: Arc::new(tokio::sync::Semaphore::new(2)),
    });

    let app_state = AppState(state.clone());

    spawn_pipeline_worker(app.clone(), state, queue_rx);
    spawn_diagnostics_publisher(app.clone(), app_state.clone());
    spawn_scheduled_messages(app_state.clone());
    spawn_startup_streamer_api_probe(app, app_state.clone());

    Ok(app_state)
}

fn spawn_diagnostics_publisher(app: AppHandle, state: AppState) {
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        loop {
            {
                let mut d = state.0.diagnostics.write();
                d.uptime_seconds = started.elapsed().as_secs();
                let _ = app.emit("diagnostics_state", d.clone());
            }
            let _ = app.emit("status_updated", state.0.get_status());
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

fn spawn_pipeline_worker(app: AppHandle, state: Arc<SharedState>, mut rx: mpsc::Receiver<PipelineInput>) {
    tauri::async_runtime::spawn(async move {
        while let Some(item) = rx.recv().await {
            match item {
                PipelineInput::Chat(chat) => {
                    process_chat_input(&app, &state, chat, true).await;
                }
                PipelineInput::LocalChat(chat) => {
                    process_chat_input(&app, &state, chat, false).await;
                }
                PipelineInput::Event(event) => {
                    let _ = state.memory.append("event", None, &event.content);
                    if state.config.read().behavior.proactive_event_replies {
                        let msg = format!("Chat, {} just happened: {}", event.kind, event.content);
                        if let Err(err) = state.twitch.send_message(msg.clone()).await {
                            warn!("event response send failed: {}", err);
                        } else {
                            let _ = app.emit(
                                "bot_response",
                                ChatMessage {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    user: state.config.read().twitch.bot_username.clone(),
                                    content: msg,
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                    is_bot: true,
                                },
                            );
                        }
                    }
                }
                PipelineInput::Manual(text) => {
                    if let Err(err) = state.twitch.send_message(text.clone()).await {
                        set_error(&app, &state, format!("Manual send failed: {err}"));
                    }
                }
            }
        }
    });
}

async fn process_chat_input(
    app: &AppHandle,
    state: &SharedState,
    chat: ChatMessage,
    send_to_twitch: bool,
) {
    if should_ignore_message(state, &chat) {
        return;
    }
    state.recent_chat.write().push_front(chat.clone());
    while state.recent_chat.read().len() > 80 {
        state.recent_chat.write().pop_back();
    }
    let _ = state.memory.append("chat", Some(&chat.user), &chat.content);

    if chat.content.starts_with('!') {
        if let Err(err) = handle_bot_command(app, state, &chat.content).await {
            set_error(app, state, format!("Command failed: {err}"));
        }
        return;
    }

    if let Some(query) = extract_search_query(&chat.content) {
        let search_cfg = state.config.read().search.clone();
        let direct = if !search_cfg.enabled {
            "Web search is disabled in Settings. Enable Search to use this.".to_string()
        } else {
            match state.search.search(&search_cfg, &query).await {
                Ok(result) => result,
                Err(err) => format!("Search failed: {err}"),
            }
        };
        if state.twitch.is_connected() {
            if let Err(err) = state.twitch.send_message(direct.clone()).await {
                set_error(app, state, format!("Direct search send failed: {err}"));
            }
        }
        let _ = app.emit(
            "bot_response",
            ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                user: state.config.read().twitch.bot_username.clone(),
                content: direct,
                timestamp: chrono::Utc::now().to_rfc3339(),
                is_bot: true,
            },
        );
        return;
    }

    if send_to_twitch && *state.lurk_mode.read() {
        return;
    }

    // In Twitch channel mode, only respond when directly addressed to avoid spam loops.
    if send_to_twitch && !is_directly_addressed(state, &chat) {
        return;
    }

    if send_to_twitch {
        if let Some(until) = *state.cooldown_until.read() {
            if Instant::now() < until {
                return;
            }
        }
    }

    let config = state.config.read().clone();
    let profile = state.personality.read().clone();
    let mut primary_provider = config.providers.primary.clone();
    if primary_provider.api_key.is_none() {
        primary_provider.api_key = state
            .secrets
            .get_provider_key(&primary_provider.name)
            .ok()
            .flatten();
    }
    let mut fallback_providers = config.providers.fallbacks.clone();
    for provider in &mut fallback_providers {
        if provider.api_key.is_none() {
            provider.api_key = state
                .secrets
                .get_provider_key(&provider.name)
                .ok()
                .flatten();
        }
    }

    let memory = state
        .memory
        .recent(config.memory.max_recent_messages)
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.content)
        .collect::<Vec<_>>();

    let recent_chat = state.recent_chat.read().iter().cloned().collect::<Vec<_>>();
    let system_prompt = PersonalityEngine::build_prompt(
        &profile,
        &recent_chat,
        &[],
        &memory,
        *state.lurk_mode.read(),
        *state.voice_enabled.read(),
    );

    let user_prompt = format!("Viewer {} said: {}", chat.user, chat.content);
    let response = state
        .llm
        .generate(
            &primary_provider,
            &fallback_providers,
            &system_prompt,
            &user_prompt,
        )
        .await;

    match response {
        Ok(mut text) => {
            text = text.chars().take(config.moderation.max_reply_chars).collect();
            if state.twitch.is_connected() {
                if let Err(err) = state.twitch.send_message(text.clone()).await {
                    set_error(app, state, format!("Failed to send Twitch reply: {err}"));
                }
            }

            let bot_event = ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                user: state.config.read().twitch.bot_username.clone(),
                content: text.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                is_bot: true,
            };
            let _ = app.emit("bot_response", &bot_event);
            let _ = state.memory.append("bot_reply", Some("bot"), &text);

            if send_to_twitch {
                let wait_ms = config.moderation.minimum_reply_interval_ms;
                *state.cooldown_until.write() = Some(Instant::now() + Duration::from_millis(wait_ms));
            }
        }
        Err(err) => {
            set_error(app, state, format!("LLM generation failed: {err}"));
        }
    }
}

fn is_directly_addressed(state: &SharedState, chat: &ChatMessage) -> bool {
    let content = chat.content.trim().to_lowercase();
    if content.starts_with("!ai ") || content.starts_with("@ai ") {
        return true;
    }
    let cfg = state.config.read().twitch.clone();
    let bot = cfg.bot_username.trim().trim_start_matches('#').to_lowercase();
    let channel = cfg.channel.trim().trim_start_matches('#').to_lowercase();
    let broadcaster = cfg
        .broadcaster_login
        .unwrap_or_default()
        .trim()
        .trim_start_matches('#')
        .to_lowercase();

    let mentions = [
        format!("@{bot}"),
        format!("{bot},"),
        format!("{bot}:"),
        format!("@{channel}"),
        format!("@{broadcaster}"),
    ];

    mentions
        .iter()
        .filter(|m| !m.trim_matches('@').trim().is_empty())
        .any(|m| content.contains(m))
}

fn spawn_scheduled_messages(state: AppState) {
    tauri::async_runtime::spawn(async move {
        loop {
            let minutes = state
                .0
                .config
                .read()
                .behavior
                .scheduled_messages_minutes
                .unwrap_or(0);

            if minutes == 0 {
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }

            tokio::time::sleep(Duration::from_secs(minutes * 60)).await;

            if !state.0.config.read().behavior.cohost_mode {
                continue;
            }
            if *state.0.lurk_mode.read() {
                continue;
            }

            let _ = state
                .0
                .response_queue_tx
                .send(PipelineInput::Manual(
                    "Quick check-in: hydrate, stretch, and tell chat what build we ship next.".to_string(),
                ))
                .await;
        }
    });
}

fn spawn_startup_streamer_api_probe(app: AppHandle, state: AppState) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let cfg = state.0.config.read().clone();
        let broadcaster_login = cfg
            .twitch
            .broadcaster_login
            .as_deref()
            .map(|v| v.trim().trim_start_matches('#').to_lowercase())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| cfg.twitch.channel.trim().trim_start_matches('#').to_lowercase());

        if broadcaster_login.is_empty() {
            let _ = app.emit(
                "timeline_event",
                serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "kind": "eventsub_check",
                    "content": "Streamer API check skipped (startup): broadcaster login is not configured",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            );
            return;
        }

        let key = format!("broadcaster:{broadcaster_login}");
        let Some(token) = state.0.secrets.get_twitch_token(&key).ok().flatten() else {
            let _ = app.emit(
                "timeline_event",
                serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "kind": "eventsub_check",
                    "content": format!("Streamer API check skipped (startup): no streamer session for {broadcaster_login}"),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            );
            return;
        };

        match smoke_test_streamer_api(&cfg.twitch.client_id, &token, &broadcaster_login).await {
            Ok(summary) => {
                let _ = app.emit(
                    "timeline_event",
                    serde_json::json!({
                        "id": uuid::Uuid::new_v4().to_string(),
                        "kind": "eventsub_check",
                        "content": format!("{summary} (startup)"),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }),
                );
            }
            Err(err) => {
                let msg = format!("Streamer API check failed (startup): {err}");
                state.0.diagnostics.write().last_error =
                    Some(msg.clone());
                let _ = app.emit("error_banner", msg.clone());
                let _ = app.emit(
                    "timeline_event",
                    serde_json::json!({
                        "id": uuid::Uuid::new_v4().to_string(),
                        "kind": "eventsub_check",
                        "content": msg,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }),
                );
            }
        }
    });
}

fn should_ignore_message(state: &SharedState, chat: &ChatMessage) -> bool {
    let cfg = state.config.read().twitch.clone();
    let chat_user = chat.user.trim().trim_start_matches('#').to_lowercase();
    let bot_user = cfg.bot_username.trim().trim_start_matches('#').to_lowercase();
    if !bot_user.is_empty() && chat_user == bot_user {
        return true;
    }

    let blocked = state.config.read().moderation.blocked_phrases.clone();
    let lower = chat.content.to_lowercase();
    if blocked.iter().any(|p| lower.contains(&p.to_lowercase())) {
        return true;
    }

    let mut seen = state.seen_message_ids.write();
    if seen.contains(&chat.id) {
        return true;
    }
    seen.insert(chat.id.clone());
    if seen.len() > 2000 {
        let keep = seen.iter().take(1000).cloned().collect::<HashSet<_>>();
        *seen = keep;
    }
    false
}

async fn handle_bot_command(app: &AppHandle, state: &SharedState, command: &str) -> Result<(), String> {
    let trimmed = command.trim();

    if let Some(rest) = trimmed.strip_prefix("!search ") {
        let search_cfg = state.config.read().search.clone();
        let result = state
            .search
            .search(&search_cfg, rest)
            .await
            .map_err(|e| e.to_string())?;
        state
            .twitch
            .send_message(result)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    if let Some(rest) = trimmed.strip_prefix("!say ") {
        state
            .response_queue_tx
            .send(PipelineInput::Manual(rest.to_string()))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    if trimmed == "!lurk on" {
        *state.lurk_mode.write() = true;
        let _ = app.emit("status_updated", state.get_status());
        return Ok(());
    }
    if trimmed == "!lurk off" {
        *state.lurk_mode.write() = false;
        let _ = app.emit("status_updated", state.get_status());
        return Ok(());
    }

    if let Some(rest) = trimmed.strip_prefix("!model ") {
        state.config.write().providers.primary.model = rest.to_string();
        let _ = app.emit("status_updated", state.get_status());
        return Ok(());
    }

    Ok(())
}

fn extract_search_query(input: &str) -> Option<String> {
    let lowered = input.trim().to_lowercase();
    let prefixes = ["!search ", "search for ", "search ", "look up ", "google "];
    for prefix in prefixes {
        if let Some(rest) = lowered.strip_prefix(prefix) {
            let q = rest.trim();
            if !q.is_empty() {
                return Some(q.to_string());
            }
        }
    }
    None
}

fn set_error(app: &AppHandle, state: &SharedState, err: String) {
    error!("{}", err);
    state.diagnostics.write().last_error = Some(err.clone());
    let _ = app.emit("error_banner", err);
}

pub fn update_twitch_state(state: &SharedState, conn: ConnectionState) {
    state.diagnostics.write().twitch_state = conn;
}

pub fn try_provider_health_probe(state: Arc<SharedState>) {
    tauri::async_runtime::spawn(async move {
        let provider = state.config.read().providers.primary.clone();
        let ok = state.llm.healthcheck(&provider).await;
        if ok {
            state.diagnostics.write().provider_state = ConnectionState::Connected;
            info!("primary provider is healthy");
        } else {
            state.diagnostics.write().provider_state = ConnectionState::Error;
            warn!("primary provider health check failed");
        }
    });
}
