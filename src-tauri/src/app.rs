use std::{
    collections::{HashSet, VecDeque},
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use rand::seq::SliceRandom;

use crate::{
    config::ProviderConfig,
    config::AppConfig,
    llm::provider::LlmService,
    memory::store::{MemoryRecord, MemoryStore},
    personality::engine::PersonalityEngine,
    security::secret_store::SecretStore,
    search::service::SearchService,
    state::{AppState, ChatMessage, ConnectionState, DiagnosticsState, EventMessage, PipelineInput, SharedState},
    twitch::eventsub::{smoke_test_streamer_api, EventSubService},
    twitch::irc::TwitchIrcService,
};

fn normalize_provider_model(provider: &mut ProviderConfig) {
    let model = provider.model.trim().to_lowercase();
    let cloud = provider.name.eq_ignore_ascii_case("ollama-cloud");
    if model.is_empty() {
        provider.model = if cloud {
            "qwen3:8b".to_string()
        } else {
            "llama3.2:3b".to_string()
        };
        return;
    }
    if cloud {
        if model.contains("qwen2.5vl")
            || model.contains("mistral-small:24b-instruct")
            || model.contains("qwen2.5:14b-instruct")
            || model.contains("llama3.1:8b-instruct")
            || model.contains("llama3.3:70b-instruct")
            || model.contains("phi4:14b")
        {
            provider.model = "qwen3:8b".to_string();
        }
    } else if model.contains("qwen2.5vl") {
        provider.model = "llama3.2:3b".to_string();
    }
}

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

    let mut migrated_sensitive_values = false;
    if let Some(token) = config.twitch.bot_token.take() {
        let bot_key = config
            .twitch
            .bot_username
            .trim()
            .trim_start_matches('#')
            .to_lowercase();
        if !bot_key.is_empty() {
            let _ = secrets.set_twitch_token(&bot_key, &token);
        }
        migrated_sensitive_values = true;
    }
    if let Some(key) = config.providers.primary.api_key.take() {
        let _ = secrets.set_provider_key(&config.providers.primary.name, &key);
        migrated_sensitive_values = true;
    }
    for provider in &mut config.providers.fallbacks {
        if let Some(key) = provider.api_key.take() {
            let _ = secrets.set_provider_key(&provider.name, &key);
            migrated_sensitive_values = true;
        }
    }
    if migrated_sensitive_values {
        let _ = config.save_to_disk();
    }

    let profile = config.personality.clone();
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
        recent_event_replies: RwLock::new(VecDeque::new()),
        recent_bot_replies: RwLock::new(VecDeque::new()),
        local_prompt_counter: RwLock::new(0),
        llm_hiccup_notice_sent: RwLock::new(false),
        local_chat_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        chat_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        event_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        stt_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        tts_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        search_gate: Arc::new(tokio::sync::Semaphore::new(2)),
        summarize_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        browser_gate: Arc::new(tokio::sync::Semaphore::new(2)),
    });

    let app_state = AppState(state.clone());

    spawn_pipeline_worker(app.clone(), state, queue_rx);
    spawn_diagnostics_publisher(app.clone(), app_state.clone());
    spawn_scheduled_messages(app.clone(), app_state.clone());
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
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

fn spawn_pipeline_worker(app: AppHandle, state: Arc<SharedState>, mut rx: mpsc::Receiver<PipelineInput>) {
    tauri::async_runtime::spawn(async move {
        while let Some(item) = rx.recv().await {
            let app = app.clone();
            let state = state.clone();
            match item {
                PipelineInput::Chat(chat) => {
                    tauri::async_runtime::spawn(async move {
                        let _permit = state.chat_gate.clone().acquire_owned().await;
                        process_chat_input(&app, &state, chat, true).await;
                    });
                }
                PipelineInput::LocalChat(chat) => {
                    tauri::async_runtime::spawn(async move {
                        let _permit = state.local_chat_gate.clone().acquire_owned().await;
                        process_chat_input(&app, &state, chat, false).await;
                    });
                }
                PipelineInput::Event(event) => {
                    tauri::async_runtime::spawn(async move {
                        let _permit = state.event_gate.clone().acquire_owned().await;
                        process_event_input(&app, &state, event).await;
                    });
                }
                PipelineInput::Manual(text) => {
                    send_bot_message(&app, &state, text, false);
                }
            }
        }
    });
}

fn normalize_for_dedupe(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c.is_ascii_whitespace() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_recent_event_reply(state: &SharedState, text: &str) -> bool {
    let normalized = normalize_for_dedupe(text);
    if normalized.is_empty() {
        return false;
    }
    state
        .recent_event_replies
        .read()
        .iter()
        .any(|v| v == &normalized)
}

fn remember_event_reply(state: &SharedState, text: &str) {
    let normalized = normalize_for_dedupe(text);
    if normalized.is_empty() {
        return;
    }
    let mut q = state.recent_event_replies.write();
    q.push_front(normalized);
    while q.len() > 80 {
        q.pop_back();
    }
}

fn has_recent_bot_reply(state: &SharedState, text: &str) -> bool {
    let normalized = normalize_for_dedupe(text);
    if normalized.is_empty() {
        return false;
    }
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    state.recent_bot_replies.read().iter().any(|v| {
        if v == &normalized {
            return true;
        }
        if normalized.len() > 24 && (normalized.contains(v) || v.contains(&normalized)) {
            return true;
        }
        let other = v.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < 4 || other.len() < 4 {
            return false;
        }
        let overlap = tokens.iter().filter(|tok| other.contains(tok)).count();
        let base = tokens.len().min(other.len());
        overlap * 100 / base >= 72
    })
}

fn remember_bot_reply(state: &SharedState, text: &str) {
    let normalized = normalize_for_dedupe(text);
    if normalized.is_empty() {
        return;
    }
    let mut q = state.recent_bot_replies.write();
    q.push_front(normalized);
    while q.len() > 60 {
        q.pop_back();
    }
}

fn clean_fact_fragment(input: &str) -> String {
    input.trim()
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '.' | ',' | '!' | '?' | ':' | ';'))
        .split(|c| matches!(c, '\n' | '\r'))
        .next()
        .unwrap_or_default()
        .split(" and ")
        .next()
        .unwrap_or_default()
        .chars()
        .take(140)
        .collect::<String>()
        .trim()
        .to_string()
}

fn clean_identity_label(input: &str) -> String {
    let mut out = clean_fact_fragment(input);
    for suffix in [
        " from now on",
        " when you talk to me",
        " when we talk",
        " in chat",
        " on stream",
        " going forward",
        " please",
    ] {
        loop {
            let lowered = out.to_lowercase();
            if lowered.ends_with(suffix) {
                let new_len = out.len().saturating_sub(suffix.len());
                out.truncate(new_len);
                out = out.trim().trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | '.' | '!' | '?')).to_string();
            } else {
                break;
            }
        }
    }
    out.split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn extract_after_phrase(text: &str, phrases: &[&str]) -> Option<String> {
    let lowered = text.to_lowercase();
    for phrase in phrases {
        if let Some(idx) = lowered.find(phrase) {
            let fragment = &text[idx + phrase.len()..];
            let clean = clean_fact_fragment(fragment);
            if !clean.is_empty() {
                return Some(clean);
            }
        }
    }
    None
}

fn has_recent_memory_fact(state: &SharedState, kind: &str, content: &str) -> bool {
    let target = normalize_for_dedupe(content);
    if target.is_empty() {
        return false;
    }
    state
        .memory
        .recent(80)
        .unwrap_or_default()
        .into_iter()
        .filter(|rec| rec.kind == kind)
        .any(|rec| normalize_for_dedupe(&rec.content) == target)
}

fn append_memory_fact(state: &SharedState, kind: &str, user: &str, content: String) {
    let clean = content.trim();
    if clean.is_empty() || has_recent_memory_fact(state, kind, clean) {
        return;
    }
    let _ = state.memory.append(kind, Some(user), clean);
}

fn same_user(a: &str, b: &str) -> bool {
    normalize_for_dedupe(a) == normalize_for_dedupe(b)
}

fn remember_bot_identity_facts(state: &SharedState, content: &str) {
    let lowered = content.to_lowercase();
    if let Some(pref) = extract_after_phrase(content, &["i like ", "i love ", "i prefer ", "my favorite "]) {
        if lowered.starts_with("i like ")
            || lowered.starts_with("i love ")
            || lowered.starts_with("i prefer ")
            || lowered.starts_with("my favorite ")
        {
            append_memory_fact(state, "bot_preference", "bot", format!("Bot likes {pref}."));
        }
    }
    if let Some(dislike) = extract_after_phrase(content, &["i hate ", "i dislike ", "i do not like "]) {
        if lowered.starts_with("i hate ")
            || lowered.starts_with("i dislike ")
            || lowered.starts_with("i do not like ")
        {
            append_memory_fact(state, "bot_preference", "bot", format!("Bot dislikes {dislike}."));
        }
    }
}

fn remember_salient_chat_facts(state: &SharedState, chat: &ChatMessage) {
    let lowered = chat.content.to_lowercase();
    let user = chat.user.trim();
    if user.is_empty() {
        return;
    }

    if let Some(name) = extract_after_phrase(&chat.content, &["my name is "]) {
        let name = clean_identity_label(&name);
        if !name.is_empty() {
            append_memory_fact(state, "profile_fact", user, format!("{user} says their preferred name is {name}."));
        }
    }
    if let Some(name) = extract_after_phrase(
        &chat.content,
        &[
            "call me ",
            "you can call me ",
            "refer to me as ",
            "when you talk to me call me ",
            "i want you to call me ",
            "please call me ",
            "my nickname is ",
            "my pet name is ",
            "my title is ",
        ],
    ) {
        let name = clean_identity_label(&name);
        if !name.is_empty() {
            append_memory_fact(state, "address_preference", user, format!("Address {user} as {name}."));
        }
    }
    if let Some(pref) = extract_after_phrase(&chat.content, &["i like ", "i love ", "i prefer ", "my favorite "]) {
        append_memory_fact(state, "preference", user, format!("{user} likes {pref}."));
    }
    if let Some(dislike) = extract_after_phrase(&chat.content, &["i hate ", "i dislike ", "i do not like "]) {
        append_memory_fact(state, "preference", user, format!("{user} dislikes {dislike}."));
    }
    if let Some(goal) = extract_after_phrase(&chat.content, &["i want ", "i need ", "i'm trying to ", "i am trying to "]) {
        append_memory_fact(state, "goal", user, format!("{user} wants or needs {goal}."));
    }
    if let Some(memory) = extract_after_phrase(&chat.content, &["remember ", "please remember "]) {
        append_memory_fact(state, "explicit_memory", user, format!("{user} explicitly asked to remember: {memory}."));
    }
    if let Some(memory) = extract_after_phrase(&chat.content, &["don't forget ", "do not forget ", "always remember "]) {
        append_memory_fact(state, "explicit_memory", user, format!("{user} explicitly wants remembered: {memory}."));
    }
    if let Some(dynamic) = extract_after_phrase(
        &chat.content,
        &["you are my ", "you're my ", "we are ", "our dynamic is ", "i'm your ", "i am your "],
    ) {
        let dynamic = clean_identity_label(&dynamic);
        if !dynamic.is_empty() {
            append_memory_fact(state, "relationship_state", user, format!("Relationship framing from {user}: {dynamic}."));
        }
    }
    if let Some(role) = extract_after_phrase(
        &chat.content,
        &["call me your ", "treat me like your ", "i'm your ", "i am your "],
    ) {
        let role = clean_identity_label(&role);
        if !role.is_empty() {
            append_memory_fact(state, "role_label", user, format!("{user} wants role framing around {role}."));
        }
    }
    if let Some(pronouns) = extract_after_phrase(&chat.content, &["my pronouns are "]) {
        append_memory_fact(state, "profile_fact", user, format!("{user}'s pronouns are {pronouns}."));
    }

    if lowered.contains("usb mic")
        || lowered.contains("headset")
        || lowered.contains("microphone")
        || lowered.contains("audio interface")
        || lowered.contains("not streaming")
        || lowered.contains("local chat")
        || lowered.contains("twitch chat")
    {
        append_memory_fact(state, "setup_fact", user, format!("{user} setup note: {}", clean_fact_fragment(&chat.content)));
    }

    if lowered.starts_with("actually ") || lowered.starts_with("no,") || lowered.starts_with("no ") || lowered.contains("i mean ") {
        append_memory_fact(state, "correction", user, format!("{user} correction: {}", clean_fact_fragment(&chat.content)));
    }

    let repeated = state
        .recent_chat
        .read()
        .iter()
        .take(12)
        .filter(|item| item.user.eq_ignore_ascii_case(&chat.user))
        .filter(|item| normalize_for_dedupe(&item.content) == normalize_for_dedupe(&chat.content))
        .count();
    if repeated >= 2 && chat.content.trim().len() >= 20 {
        append_memory_fact(
            state,
            "priority_fact",
            user,
            format!("{user} repeated this and it is likely important: {}", clean_fact_fragment(&chat.content)),
        );
    }
}

fn build_memory_context(state: &SharedState, max_items: usize) -> Vec<String> {
    let mut pinned = state
        .memory
        .list_pinned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| format!("[pinned:{}] {}", item.label, item.content))
        .collect::<Vec<_>>();
    let records = state.memory.recent(64).unwrap_or_default();
    let mut priority = Vec::new();
    let mut recent = Vec::new();
    for m in records {
        let line = render_memory_record(&m);
        match m.kind.as_str() {
            "story_state" | "relationship_state" | "role_label" | "address_preference" | "explicit_memory" | "profile_fact" | "preference" | "goal" | "setup_fact" | "correction" | "priority_fact" | "bot_preference" => priority.push(line),
            _ => recent.push(line),
        }
    }
    pinned.extend(priority);
    pinned.extend(recent);
    pinned.truncate(max_items);
    pinned
}

fn build_user_memory_context(state: &SharedState, user: &str, max_items: usize) -> Vec<String> {
    let mut pinned = state
        .memory
        .list_pinned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| format!("[pinned:{}] {}", item.label, item.content))
        .collect::<Vec<_>>();
    let records = state.memory.recent(96).unwrap_or_default();
    let mut direct = Vec::new();
    let mut other = Vec::new();
    for m in records {
        let Some(owner) = m.user.as_deref() else {
            continue;
        };
        if !same_user(owner, user) {
            continue;
        }
        let line = render_memory_record(&m);
        match m.kind.as_str() {
            "address_preference" | "role_label" | "explicit_memory" | "profile_fact" | "relationship_state" | "correction" | "priority_fact" | "preference" | "goal" => direct.push(line),
            _ => other.push(line),
        }
    }
    pinned.extend(direct);
    pinned.extend(other);
    pinned.truncate(max_items);
    pinned
}

fn compact_memory_value(value: &str, max_chars: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

fn render_memory_record(record: &MemoryRecord) -> String {
    if record.kind == "voice_frame" {
        let metadata = record
            .metadata
            .clone()
            .or_else(|| serde_json::from_str::<serde_json::Value>(&record.content).ok());
        if let Some(meta) = metadata {
            let transcript = meta
                .get("transcript")
                .and_then(|v| v.as_str())
                .map(|v| compact_memory_value(v, 160))
                .unwrap_or_else(|| compact_memory_value(&record.content, 160));
            let normalized = meta
                .get("normalizedTranscript")
                .and_then(|v| v.as_str())
                .map(|v| compact_memory_value(v, 120))
                .filter(|v| !v.is_empty() && v != &transcript);
            let subject = meta
                .get("nameHint")
                .and_then(|v| v.as_str())
                .or(record.subject.as_deref())
                .or(record.user.as_deref())
                .filter(|v| !v.trim().is_empty())
                .unwrap_or("speaker");
            let command_hint = meta
                .get("commandHint")
                .and_then(|v| v.as_str())
                .map(|v| compact_memory_value(v, 48))
                .filter(|v| !v.is_empty());
            let engine = meta
                .get("engine")
                .and_then(|v| v.as_str())
                .map(|v| compact_memory_value(v, 24))
                .filter(|v| !v.is_empty());
            let mode = meta
                .get("mode")
                .and_then(|v| v.as_str())
                .map(|v| compact_memory_value(v, 24))
                .filter(|v| !v.is_empty());
            let time = meta
                .get("timeContextIso")
                .and_then(|v| v.as_str())
                .map(|v| compact_memory_value(v, 32))
                .filter(|v| !v.is_empty());

            let mut details = Vec::new();
            if let Some(value) = normalized {
                details.push(format!("normalized=\"{value}\""));
            }
            if let Some(value) = command_hint {
                details.push(format!("command={value}"));
            }
            if let Some(value) = engine {
                details.push(format!("engine={value}"));
            }
            if let Some(value) = mode {
                details.push(format!("mode={value}"));
            }
            if let Some(value) = time {
                details.push(format!("time={value}"));
            }

            if details.is_empty() {
                return format!("[voice_frame:{subject}] heard=\"{transcript}\"");
            }
            return format!(
                "[voice_frame:{subject}] heard=\"{transcript}\" | {}",
                details.join(" | ")
            );
        }
    }

    let user = record.user.as_deref().unwrap_or_default();
    let content = compact_memory_value(&record.content, 180);
    if user.is_empty() {
        format!("[{}] {}", record.kind, content)
    } else {
        format!("[{}:{}] {}", record.kind, user, content)
    }
}

fn recent_bot_story_context(state: &SharedState, max_items: usize) -> Vec<String> {
    state
        .memory
        .recent(40)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.kind == "bot_reply" || m.kind == "story_state")
        .take(max_items)
        .map(|m| m.content)
        .collect::<Vec<_>>()
}

fn looks_like_story_request(input: &str) -> bool {
    let lowered = input.to_lowercase();
    [
        "tell me a story",
        "continue the story",
        "continue this",
        "write a story",
        "make up a story",
        "romantic conversation",
        "romance scene",
        "romantic scene",
        "love scene",
        "seduce",
        "slow burn",
        "roleplay",
        "story mode",
        "sex story",
        "erotic story",
        "nsfw story",
        "dirty story",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn normalize_repetitive_question_reply(text: &str, story_mode: bool, latest_input: &str) -> String {
    let mut out = text.trim().to_string();
    if out.is_empty() {
        return out;
    }
    let question_count = out.matches('?').count();
    let latest_is_question = latest_input.trim().ends_with('?');

    if story_mode && question_count > 0 {
        out = out.replace('?', ".");
    } else if !latest_is_question && question_count > 0 {
        if let Some(last) = out.rfind('?') {
            out.truncate(last);
            out = out.trim().trim_end_matches(|c: char| c == ',' || c == ';' || c == ':').to_string();
            if !out.ends_with('.') && !out.ends_with('!') {
                out.push('.');
            }
        }
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn stylize_reply_punctuation(text: &str) -> String {
    let mut out = text.trim().to_string();
    if out.is_empty() {
        return out;
    }

    let lowered = out.to_lowercase();
    let soft_hits = [
        "sleepy", "soft", "slow", "easy", "gentle", "quiet", "hush", "whisper",
        "sweet", "tender", "closer", "relax", "breathe", "mm", "mmm"
    ]
    .iter()
    .filter(|needle| lowered.contains(**needle))
    .count();

    let excited_hits = [
        "wow", "yes", "yesss", "holy", "damn", "baby", "daddy", "perfect", "love that",
        "lets go", "let's go", "so good", "right now", "god"
    ]
    .iter()
    .filter(|needle| lowered.contains(**needle))
    .count();

    out = out.trim_end_matches(|c: char| matches!(c, '.' | '?' | '!')).trim().to_string();
    if out.is_empty() {
        return out;
    }

    if soft_hits > 0 {
        out.push_str("...");
    } else if excited_hits > 0 {
        out.push('!');
    } else if out.split_whitespace().count() <= 5 {
        out.push('!');
    } else {
        out.push('.');
    }

    out
}

fn fallback_event_reply(event: &EventMessage) -> String {
    let mut rng = rand::thread_rng();
    let pools = match event.kind.as_str() {
        "follow" | "channel.follow" => vec![
            "New follower just unlocked chaos mode. Chat act civilized for 0.3 seconds.",
            "Fresh follow detected. Welcome to the circus, your seat is on fire.",
            "Follower joined and the vibe meter just pegged red.",
        ],
        "subscribe" | "channel.subscribe" => vec![
            "Sub alert! Somebody upgraded from lurker to legend.",
            "Subscription landed like a meteor. Respectfully, that was elite.",
            "Chat, a new sub just entered with main-character energy.",
        ],
        "channel.subscription.gift" => vec![
            "Gift sub storm incoming, hold onto your wigs.",
            "Gifted subs just dropped like loot from a raid boss.",
            "Somebody just fed chat. Absolute menace behavior.",
        ],
        "channel.raid" => vec![
            "Raid just kicked the door in. Hide your frame rate.",
            "Raiders inbound. Everyone look busy and pretend we planned this.",
            "Raid train arrived and my sanity left the station.",
        ],
        "stream.online" => vec![
            "We are live and immediately making questionable choices.",
            "Stream is online. Safety rails are now decorative.",
            "Live signal confirmed. Maximum nonsense unlocked.",
        ],
        "stream.offline" => vec![
            "Stream offline. Wrap it up, goblins.",
            "We are out. Touch grass and hydrate, degenerates.",
            "Offline screen deployed. Chaos postponed, not canceled.",
        ],
        _ => vec![
            "Event just detonated and chat is legally unwell.",
            "That alert was wild. Somebody clip the emotional damage.",
            "New event dropped. The timeline remains deeply cursed.",
        ],
    };
    let pick = pools
        .choose(&mut rng)
        .copied()
        .unwrap_or("Event hit hard and chat chose violence.");
    let tail = event.content.trim();
    if tail.is_empty() {
        pick.to_string()
    } else {
        format!("{pick} ({})", tail.chars().take(80).collect::<String>())
    }
}

fn uniquify_event_reply(state: &SharedState, event: &EventMessage, mut text: String) -> String {
    text = text.trim().to_string();
    if text.is_empty() {
        return fallback_event_reply(event);
    }
    if !has_recent_event_reply(state, &text) {
        return text;
    }
    let alt = fallback_event_reply(event);
    if !has_recent_event_reply(state, &alt) {
        return alt;
    }
    format!("{alt} #{}", uuid::Uuid::new_v4().to_string().chars().take(6).collect::<String>())
}

async fn process_event_input(app: &AppHandle, state: &SharedState, event: EventMessage) {
    let _ = state.memory.append("event", None, &event.content);

    let config = state.config.read().clone();
    let profile = state.personality.read().clone();
    let mut primary_provider = config.providers.primary.clone();
    normalize_provider_model(&mut primary_provider);
    if primary_provider.model.trim().is_empty() {
        send_bot_message(
            app,
            state,
            "No LLM model selected. Open AI Setup and pick a model preset, then enable cloud mode."
                .to_string(),
            false,
        );
        return;
    }
    if primary_provider.api_key.is_none() {
        primary_provider.api_key = state
            .secrets
            .get_provider_key(&primary_provider.name)
            .ok()
            .flatten();
    }
    let mut fallback_providers = config.providers.fallbacks.clone();
    for provider in &mut fallback_providers {
        normalize_provider_model(provider);
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
        .recent(config.memory.max_recent_messages.min(8))
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.content)
        .collect::<Vec<_>>();
    let recent_chat = state.recent_chat.read().iter().cloned().collect::<Vec<_>>();
    let system_prompt = PersonalityEngine::build_prompt(
        &profile,
        &recent_chat,
        std::slice::from_ref(&event),
        &memory,
        *state.lurk_mode.read(),
        *state.voice_enabled.read(),
    );
    let user_prompt = format!(
        "EventSub alert received.\nKind: {}\nDetails: {}\nWrite one short personality-matching reaction under 18 words. Keep it funny, specific, and different from prior event replies.",
        event.kind, event.content
    );

    let candidate = match state
        .llm
        .generate(
            &primary_provider,
            &fallback_providers,
            &system_prompt,
            &user_prompt,
        )
        .await
    {
        Ok(text) => text,
        Err(err) => {
            warn!("event llm generation failed: {}", err);
            fallback_event_reply(&event)
        }
    };

    let mut msg = uniquify_event_reply(state, &event, candidate);
    msg = sanitize_bot_output(&msg);
    msg = msg.chars().take(config.moderation.max_reply_chars).collect();
    if msg.is_empty() {
        return;
    }
    remember_event_reply(state, &msg);

    send_bot_message(app, state, msg, true);
}

async fn process_chat_input(
    app: &AppHandle,
    state: &SharedState,
    chat: ChatMessage,
    send_to_twitch: bool,
) {
    let outbound_to_twitch = send_to_twitch || state.twitch.is_connected();
    if should_ignore_message(state, &chat) {
        return;
    }
    state.recent_chat.write().push_front(chat.clone());
    while state.recent_chat.read().len() > 120 {
        state.recent_chat.write().pop_back();
    }
    let _ = state.memory.append("chat", Some(&chat.user), &chat.content);
    remember_salient_chat_facts(state, &chat);

    if let Some(command_input) = normalize_control_command_input(&chat.content) {
        let sender = chat.user.clone();
        if let Err(err) = handle_bot_command(app, state, &sender, &command_input, send_to_twitch).await {
            send_bot_message(
                app,
                state,
                format!("Command failed: {}", sanitize_bot_output(&err)),
                false,
            );
        }
        return;
    }

    let force_reply = contains_chatbot_keyword(&chat.content)
        || (send_to_twitch && is_directly_addressed(state, &chat));

    if let Some(query) = extract_search_query(&chat.content) {
        let mut search_cfg = state.config.read().search.clone();
        // Conversation mode: allow direct search prompts without forcing settings toggles.
        search_cfg.enabled = true;
        let direct = match state.search.search(&search_cfg, &query).await {
            Ok(result) => result,
            Err(err) => format!("Search failed: {err}"),
        };
        let clean = sanitize_bot_output(&direct);
        if clean.is_empty() {
            return;
        }
        send_bot_message(app, state, clean, outbound_to_twitch);
        return;
    }

    if send_to_twitch && *state.lurk_mode.read() {
        return;
    }

    let cohost_mode = state.config.read().behavior.cohost_mode;
    if send_to_twitch {
        if !cohost_mode && !force_reply {
            return;
        }
        // Always reply when explicitly invoked with the keyword.
        if !force_reply {
            let n = {
                let mut c = state.local_prompt_counter.write();
                *c = c.saturating_add(1);
                *c
            };
            // Keep ambient Twitch chatter sparse unless directly invoked.
            if n % 6 != 0 || chat.content.trim().len() < 18 {
                return;
            }
        }
        if let Some(until) = *state.cooldown_until.read() {
            if !force_reply && Instant::now() < until {
                return;
            }
        }
    }

    let config = state.config.read().clone();
    let profile = state.personality.read().clone();
    let mut primary_provider = config.providers.primary.clone();
    normalize_provider_model(&mut primary_provider);
    if primary_provider.api_key.is_none() {
        primary_provider.api_key = state
            .secrets
            .get_provider_key(&primary_provider.name)
            .ok()
            .flatten();
    }
    let mut fallback_providers = config.providers.fallbacks.clone();
    for provider in &mut fallback_providers {
        normalize_provider_model(provider);
        if provider.api_key.is_none() {
            provider.api_key = state
                .secrets
                .get_provider_key(&provider.name)
                .ok()
                .flatten();
        }
    }

    let memory = build_memory_context(state, config.memory.max_recent_messages.min(20));
    let speaker_memory = build_user_memory_context(state, &chat.user, 10);
    let recent_story = recent_bot_story_context(state, 6);

    let recent_chat = state.recent_chat.read().iter().cloned().collect::<Vec<_>>();
    let system_prompt = PersonalityEngine::build_prompt(
        &profile,
        &recent_chat,
        &[],
        &memory,
        *state.lurk_mode.read(),
        *state.voice_enabled.read(),
    );

    let story_mode = looks_like_story_request(&chat.content);
    let keep_talking_mode = config.behavior.topic_continuation_mode;
    let user_prompt = if send_to_twitch {
        if story_mode {
            format!(
                "Viewer {} said: {}\nThis is a story or scene request. Continue it with concrete details and a strong voice. Use statements, not a list of questions. Keep it concise enough for chat, around 2 to 4 sentences, but still advance the scene.\nCurrent speaker memory:\n{}\nRecent scene context:\n{}",
                chat.user, chat.content, speaker_memory.join("\n"), recent_story.join("\n")
            )
        } else if keep_talking_mode {
            format!(
                "Viewer {} said: {}\nKeep talking about the current subject. Stay on topic, make statements, develop the idea, and avoid question loops. Use recent memory and scene context if relevant. Reply in 2 or 3 short sentences and do not end with more than one brief question.\nCurrent speaker memory:\n{}\nRecent scene context:\n{}",
                chat.user, chat.content, speaker_memory.join("\n"), recent_story.join("\n")
            )
        } else {
            format!(
                "Viewer {} said: {}\nAnswer the actual point of that line first. Make at least one grounded statement or observation before asking anything. Reference one concrete detail from it. If the line appears garbled, incomplete, or low-confidence, return no reply instead of guessing. Keep it under 28 words and stay on topic.\nCurrent speaker memory:\n{}",
                chat.user, chat.content, speaker_memory.join("\n")
            )
        }
    } else {
        if story_mode {
            format!(
                "Streamer {} said: {}\nThis is a live local cohost exchange and the user wants an ongoing story, romance, or scene. Continue the scene instead of interrogating them. Use concrete sensory details, emotional continuity, and established context from recent chat and memory. Prefer statements over questions. Write 1 short paragraph or 3 to 6 sentences, and only ask a question if the user clearly invited choice or direction.\nCurrent speaker memory:\n{}\nRecent scene context:\n{}",
                chat.user, chat.content, speaker_memory.join("\n"), recent_story.join("\n")
            )
        } else if keep_talking_mode {
            format!(
                "Streamer {} said: {}\nKeep talking about the same subject instead of resetting into another question. Develop the current topic with concrete observations, memory, and continuity. Prefer statements, reactions, and continuation over asking for clarification. Reply in 2 or 3 conversational sentences and only ask a question if absolutely necessary.\nCurrent speaker memory:\n{}\nRecent scene context:\n{}",
                chat.user, chat.content, speaker_memory.join("\n"), recent_story.join("\n")
            )
        } else {
            format!(
                "Streamer {} said: {}\nThis is a live local cohost exchange. Reply in plain everyday language. Answer the literal latest line first. Only use recent chat or memory if it directly helps clarify the latest line. Do not invent scene details, weird metaphors, or theatrical phrasing. Reply in 1 or 2 conversational sentences under 42 words. Make a grounded statement first. Do not ask a follow-up question unless the user clearly asked one or asked for options. If the line appears garbled, incomplete, or low-confidence, return no reply instead of guessing.\nCurrent speaker memory:\n{}",
                chat.user, chat.content, speaker_memory.join("\n")
            )
        }
    };
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
            text = sanitize_bot_output(&text);
            text = normalize_repetitive_question_reply(&text, story_mode || keep_talking_mode, &chat.content);
            text = stylize_reply_punctuation(&text);
            text = text.chars().take(config.moderation.max_reply_chars).collect();
            if has_recent_bot_reply(state, &text) {
                let retry_prompt = format!(
                    "{user_prompt}\nDo not repeat recent bot wording. Give a clearly different reply that still answers directly."
                );
                if let Ok(mut retried) = state
                    .llm
                    .generate(
                        &primary_provider,
                        &fallback_providers,
                        &system_prompt,
                        &retry_prompt,
                    )
                    .await
                {
                    retried = sanitize_bot_output(&retried);
                    retried = normalize_repetitive_question_reply(&retried, story_mode || keep_talking_mode, &chat.content);
                    retried = stylize_reply_punctuation(&retried);
                    text = retried.chars().take(config.moderation.max_reply_chars).collect();
                }
            }
            if send_to_twitch && has_explicit_bot_mention(state, &chat.content.to_lowercase()) {
                let mention = format!("@{}", chat.user.trim().trim_start_matches('@'));
                if !text.to_lowercase().starts_with(&mention.to_lowercase()) {
                    text = format!("{mention} {text}");
                }
            }
            if text.is_empty() || has_recent_bot_reply(state, &text) {
                return;
            }
            *state.llm_hiccup_notice_sent.write() = false;
            remember_bot_identity_facts(state, &text);
            if story_mode || keep_talking_mode {
                append_memory_fact(
                    state,
                    "story_state",
                    "bot",
                    format!("Continuation after {} said '{}': {}", chat.user, clean_fact_fragment(&chat.content), text),
                );
            }
            send_bot_message(app, state, text, outbound_to_twitch);

            if send_to_twitch {
                let wait_ms = config.moderation.minimum_reply_interval_ms;
                *state.cooldown_until.write() = Some(Instant::now() + Duration::from_millis(wait_ms));
            }
        }
        Err(err) => {
            let _ = app.emit(
                "timeline_event",
                EventMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: "llm_error".to_string(),
                    content: format!("LLM generation failed: {err}"),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            );
            set_error(app, state, format!("LLM generation failed: {err}"));
            let should_announce = !*state.llm_hiccup_notice_sent.read();
            if should_announce {
                *state.llm_hiccup_notice_sent.write() = true;
                let lower = err.to_string().to_lowercase();
                let msg = if lower.contains("401")
                    || lower.contains("unauthorized")
                    || lower.contains("api key")
                    || lower.contains("invalid oauth token")
                {
                    "Model auth missing. Save your Ollama API key once in AI Setup and retry."
                } else if lower.contains("model") && lower.contains("not found") {
                    "Selected model was not found. In AI Setup, click Check Cloud Models and pick one from your account."
                } else {
                    "Model hiccup. Ask again in a second."
                };
                send_bot_message(app, state, msg.to_string(), false);
            }
        }
    }
}

fn is_directly_addressed(state: &SharedState, chat: &ChatMessage) -> bool {
    let content = chat.content.trim().to_lowercase();
    if content.starts_with("!ai ") || content.starts_with("@ai ") || has_wake_phrase(&content) {
        return true;
    }
    has_explicit_bot_mention(state, &content)
}

fn has_explicit_bot_mention(state: &SharedState, content: &str) -> bool {
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

fn has_wake_phrase(input: &str) -> bool {
    let lowered = input.to_lowercase();
    let phrases = [
        "hey chatbot",
        "hey chat bot",
        "hey chat-bot",
        "hey robot",
        "yo chatbot",
        "ok chatbot",
        "okay chatbot",
        "chat bot,",
    ];
    phrases.iter().any(|p| lowered.contains(p))
}

fn contains_chatbot_keyword(input: &str) -> bool {
    let lowered = input.to_lowercase();
    if lowered.contains("chatbot")
        || lowered.contains("chat bot")
        || lowered.contains("chat-bot")
        || lowered.contains("chat bout")
        || lowered.contains("chat bought")
    {
        return true;
    }
    let compact = lowered
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c.is_ascii_whitespace() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    compact.contains("chatbot") || compact.contains("chat bot")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScheduledTodo {
    id: String,
    created_at: String,
    due_at: String,
    created_by: String,
    content: String,
    #[serde(default)]
    recurring_every_minutes: Option<i64>,
    #[serde(default)]
    paused: bool,
    #[serde(default)]
    run_count: u64,
    #[serde(default)]
    last_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScheduledTodoMarker {
    id: String,
    at: String,
}

fn is_control_command(input: &str) -> bool {
    let trimmed = input.trim_start();
    matches!(trimmed.chars().next(), Some('_') | Some('!') | Some('.') | Some('/'))
}

fn strip_command_prefix(input: &str) -> &str {
    let trimmed = input.trim_start();
    if matches!(trimmed.chars().next(), Some('_') | Some('!') | Some('.') | Some('/')) {
        &trimmed[1..]
    } else {
        trimmed
    }
}

fn normalize_control_command_input(input: &str) -> Option<String> {
    if is_control_command(input) {
        return Some(input.trim_start().to_string());
    }
    extract_spoken_command_body(input).map(|body| format!("_{body}"))
}

fn extract_spoken_command_body(input: &str) -> Option<String> {
    let mut spoken = input.trim();
    if spoken.is_empty() {
        return None;
    }

    let wake_prefixes = [
        "hey chatbot",
        "hey chat bot",
        "hey chat-bot",
        "hey robot",
        "yo chatbot",
        "ok chatbot",
        "okay chatbot",
    ];
    let lowered = spoken.to_lowercase();
    for wake in wake_prefixes {
        if lowered.starts_with(wake) {
            spoken = spoken[wake.len()..]
                .trim_start_matches(|c: char| c == ',' || c == ':' || c == '-' || c.is_whitespace());
            break;
        }
    }

    let lowered = spoken.to_lowercase();
    let command_keywords = [
        "command ",
        "bot command ",
        "cohost command ",
        "underscore ",
        "under score ",
    ];
    for key in command_keywords {
        if lowered.starts_with(key) {
            let body = spoken[key.len()..]
                .trim_start_matches(|c: char| c == ':' || c == '-' || c.is_whitespace())
                .trim();
            if !body.is_empty() {
                return Some(body.to_string());
            }
        }
    }
    None
}

fn parse_rfc3339_utc(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

fn load_pending_todos(state: &SharedState, max_scan: usize) -> Vec<ScheduledTodo> {
    let records = state.memory.recent(max_scan).unwrap_or_default();
    let mut all: std::collections::HashMap<String, ScheduledTodo> = std::collections::HashMap::new();
    let mut completed: HashSet<String> = HashSet::new();

    for record in records.iter().rev() {
        match record.kind.as_str() {
            "todo" => {
                if let Ok(task) = serde_json::from_str::<ScheduledTodo>(&record.content) {
                    all.insert(task.id.clone(), task);
                }
            }
            "todo_done" => {
                if let Ok(marker) = serde_json::from_str::<ScheduledTodoMarker>(&record.content) {
                    completed.insert(marker.id);
                }
            }
            _ => {}
        }
    }

    let mut out = all
        .into_values()
        .filter(|t| !completed.contains(&t.id) && !t.paused)
        .collect::<Vec<_>>();
    out.sort_by_key(|t| parse_rfc3339_utc(&t.due_at));
    out
}

fn send_bot_message(app: &AppHandle, state: &SharedState, content: String, send_to_twitch: bool) {
    let content = sanitize_bot_output(&content);
    if content.is_empty() {
        return;
    }
    if has_recent_bot_reply(state, &content) {
        return;
    }
    remember_bot_reply(state, &content);
    let allow_twitch_post = send_to_twitch
        && state.twitch.is_connected()
        && state.config.read().behavior.post_bot_messages_to_twitch;
    if allow_twitch_post {
        let twitch = state.twitch.clone();
        let msg = content.clone();
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = twitch.send_message(msg).await {
                let _ = app_clone.emit(
                    "timeline_event",
                    serde_json::json!({
                        "id": uuid::Uuid::new_v4().to_string(),
                        "kind": "irc_error",
                        "content": format!("Bot send failed: {}", err),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }),
                );
            }
        });
    }
    let _ = app.emit(
        "bot_response",
        ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            user: state.config.read().twitch.bot_username.clone(),
            content: content.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            is_bot: true,
        },
    );
    let _ = state.memory.append("bot_reply", Some("bot"), &content);
}

fn sanitize_bot_output(input: &str) -> String {
    let mut text = input.trim().to_string();
    for prefix in [
        "adjusts cape",
        "adjusts cloak",
        "adjusts hood",
        "clears throat",
        "sighs",
        "laughs",
        "chuckles",
        "smirks",
        "grins",
        "shrugs",
        "leans in",
        "pauses",
        "whispers",
        "murmurs",
        "stares",
        "nods",
        "gasps",
        "facepalms",
        "rolls eyes",
    ] {
        let lower = text.to_lowercase();
        if lower.starts_with(prefix) {
            text = text[prefix.len()..]
                .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ':' | '-' | '.' | '!' | '?'))
                .to_string();
        }
    }

    text = text
        .replace("```", " ")
        .replace('`', " ")
        .replace('*', " ")
        .replace('_', " ")
        .replace('~', " ")
        .replace('|', " ")
        .replace('<', " ")
        .replace('>', " ")
        .replace('[', " ")
        .replace(']', " ")
        .replace('(', " ")
        .replace(')', " ")
        .replace('{', " ")
        .replace('}', " ")
        .replace(':', " ");
    text = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || matches!(c, '.' | ',' | '!' | '?' | '\'' | '-') {
                c
            } else {
                ' '
            }
        })
        .collect::<String>();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn save_task_snapshot(state: &SharedState, actor: &str, task: &ScheduledTodo) {
    if let Ok(raw) = serde_json::to_string(task) {
        let _ = state.memory.append("todo", Some(actor), &raw);
    }
}

async fn execute_scheduled_action(
    app: &AppHandle,
    state: &SharedState,
    action: &str,
    send_to_twitch: bool,
) {
    let raw = action.trim();
    if raw.is_empty() {
        return;
    }

    if let Some(query) = raw
        .strip_prefix("search:")
        .map(str::trim)
        .or_else(|| raw.strip_prefix("search ").map(str::trim))
        .or_else(|| raw.strip_prefix("web search ").map(str::trim))
        .or_else(|| raw.strip_prefix("do web search ").map(str::trim))
        .or_else(|| raw.strip_prefix("do a web search ").map(str::trim))
    {
        let search_cfg = state.config.read().search.clone();
        let result = if !search_cfg.enabled {
            "Scheduled search skipped: web search is disabled.".to_string()
        } else {
            match state.search.search(&search_cfg, query).await {
                Ok(r) => r,
                Err(e) => format!("Scheduled search failed: {e}"),
            }
        };
        send_bot_message(app, state, result, send_to_twitch);
        return;
    }

    if let Some(text) = raw.strip_prefix("say:").map(str::trim).or_else(|| raw.strip_prefix("say ").map(str::trim)) {
        send_bot_message(app, state, text.to_string(), send_to_twitch);
        return;
    }

    if let Some(cmd) = raw.strip_prefix("command ").map(str::trim) {
        if !cmd.is_empty() {
            let cmd_trim = cmd.trim();
            if cmd_trim.eq_ignore_ascii_case("help")
                || cmd_trim.eq_ignore_ascii_case("commands")
                || cmd_trim.eq_ignore_ascii_case("menu")
            {
                send_bot_message(app, state, command_help_text(), send_to_twitch);
                return;
            }
            if let Some(rest) = cmd_trim.strip_prefix("search ") {
                let search_cfg = state.config.read().search.clone();
                let result = if !search_cfg.enabled {
                    "Scheduled search skipped: web search is disabled.".to_string()
                } else {
                    match state.search.search(&search_cfg, rest).await {
                        Ok(r) => r,
                        Err(e) => format!("Scheduled search failed: {e}"),
                    }
                };
                send_bot_message(app, state, result, send_to_twitch);
                return;
            }
            if let Some(rest) = cmd_trim.strip_prefix("say ") {
                send_bot_message(app, state, rest.to_string(), send_to_twitch);
                return;
            }
            if let Some(rest) = cmd_trim.strip_prefix("model ") {
                state.config.write().providers.primary.model = rest.to_string();
                send_bot_message(app, state, format!("Model set to {rest}"), send_to_twitch);
                return;
            }
            if cmd_trim.eq_ignore_ascii_case("lurk on") {
                *state.lurk_mode.write() = true;
                send_bot_message(app, state, "Lurk mode enabled.".to_string(), send_to_twitch);
                return;
            }
            if cmd_trim.eq_ignore_ascii_case("lurk off") {
                *state.lurk_mode.write() = false;
                send_bot_message(app, state, "Lurk mode disabled.".to_string(), send_to_twitch);
                return;
            }
            send_bot_message(
                app,
                state,
                format!("Scheduled command not supported: {cmd_trim}"),
                send_to_twitch,
            );
        }
        return;
    }

    send_bot_message(app, state, raw.to_string(), send_to_twitch);
}

fn command_help_text() -> String {
    [
        "Command menu",
        "Use prefix underscore. Aliases bang dot slash also work.",
        "Say command then the command words for voice control.",
        "underscore menu. Show this help.",
        "underscore search your query. Run a web search.",
        "underscore say your text. Force one local bot line.",
        "underscore model model name. Switch model.",
        "underscore lurk on or underscore lurk off. Toggle auto replies.",
        "underscore todo add minutes task. Run once later.",
        "underscore todo every minutes task. Repeat task.",
        "underscore todo list. Show pending tasks.",
        "underscore todo done id. Mark task complete.",
        "underscore todo run id. Run now.",
        "underscore agent commands are just alias names for todo commands.",
    ]
    .join("\n")
}

fn spawn_scheduled_messages(app: AppHandle, state: AppState) {
    tauri::async_runtime::spawn(async move {
        let mut next_checkin_at: Option<chrono::DateTime<chrono::Utc>> = None;
        loop {
            // Run deferred todo tasks every tick.
            let now = chrono::Utc::now();
            let due = load_pending_todos(&state.0, 2000)
                .into_iter()
                .filter(|t| parse_rfc3339_utc(&t.due_at).is_some_and(|d| d <= now))
                .collect::<Vec<_>>();
            for mut task in due {
                let marker = ScheduledTodoMarker {
                    id: task.id.clone(),
                    at: chrono::Utc::now().to_rfc3339(),
                };
                if let Ok(raw) = serde_json::to_string(&marker) {
                    let _ = state.0.memory.append("todo_ran", Some("system"), &raw);
                }
                execute_scheduled_action(&app, &state.0, &task.content, false).await;

                task.run_count = task.run_count.saturating_add(1);
                task.last_run_at = Some(chrono::Utc::now().to_rfc3339());
                if let Some(every) = task.recurring_every_minutes {
                    if every > 0 {
                        task.due_at = (chrono::Utc::now() + chrono::Duration::minutes(every)).to_rfc3339();
                        save_task_snapshot(&state.0, "system", &task);
                    }
                } else {
                    let done = ScheduledTodoMarker {
                        id: task.id.clone(),
                        at: chrono::Utc::now().to_rfc3339(),
                    };
                    if let Ok(raw) = serde_json::to_string(&done) {
                        let _ = state.0.memory.append("todo_done", Some("system"), &raw);
                    }
                }
            }

            let cfg = state.0.config.read().clone();
            let cadence = cfg
                .behavior
                .scheduled_messages_minutes
                .filter(|v| *v > 0)
                .map(|minutes| chrono::Duration::minutes(minutes as i64));

            if let Some(cadence) = cadence {
                if next_checkin_at.is_none() {
                    next_checkin_at = Some(now + cadence);
                }
                if next_checkin_at.is_some_and(|at| now >= at) {
                    if cfg.behavior.cohost_mode && !*state.0.lurk_mode.read() {
                        let _ = state
                            .0
                            .response_queue_tx
                            .send(PipelineInput::LocalChat(ChatMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                user: "system".to_string(),
                                content: "Auto cohost cue: say one short fresh line about what is happening right now, grounded in current chat and stream context, without repeating prior wording.".to_string(),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                                is_bot: false,
                            }))
                            .await;
                    }
                    next_checkin_at = Some(now + cadence);
                }
            } else {
                next_checkin_at = None;
            }

            tokio::time::sleep(Duration::from_secs(15)).await;
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
                let lower = err.to_string().to_lowercase();
                if lower.contains("401 unauthorized")
                    || lower.contains("invalid oauth token")
                    || lower.contains("invalid oauth")
                    || lower.contains("invalid token")
                {
                    let _ = state.0.secrets.clear_twitch_session(&key);
                    let _ = app.emit(
                        "timeline_event",
                        serde_json::json!({
                            "id": uuid::Uuid::new_v4().to_string(),
                            "kind": "eventsub_check",
                            "content": format!("Streamer API check skipped (startup): saved streamer session expired for {broadcaster_login}; reconnect streamer account"),
                            "timestamp": chrono::Utc::now().to_rfc3339()
                        }),
                    );
                    state.0.diagnostics.write().last_error = None;
                    return;
                }
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
    if chat.is_bot {
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

async fn handle_bot_command(
    app: &AppHandle,
    state: &SharedState,
    sender: &str,
    command: &str,
    send_to_twitch: bool,
) -> Result<(), String> {
    let trimmed = strip_command_prefix(command).trim();

    if trimmed.eq_ignore_ascii_case("help")
        || trimmed.eq_ignore_ascii_case("commands")
        || trimmed.eq_ignore_ascii_case("menu")
        || trimmed.eq_ignore_ascii_case("read commands")
    {
        send_bot_message(app, state, command_help_text(), send_to_twitch);
        return Ok(());
    }

    if let Some(rest) = trimmed.strip_prefix("search ") {
        let search_cfg = state.config.read().search.clone();
        let result = state
            .search
            .search(&search_cfg, rest)
            .await
            .map_err(|e| e.to_string())?;
        send_bot_message(app, state, result, send_to_twitch);
        return Ok(());
    }
    if let Some(rest) = trimmed
        .strip_prefix("web search ")
        .or_else(|| trimmed.strip_prefix("do web search "))
        .or_else(|| trimmed.strip_prefix("do a web search "))
    {
        let search_cfg = state.config.read().search.clone();
        let result = state
            .search
            .search(&search_cfg, rest.trim())
            .await
            .map_err(|e| e.to_string())?;
        send_bot_message(app, state, result, send_to_twitch);
        return Ok(());
    }

    if let Some(rest) = trimmed.strip_prefix("say ") {
        send_bot_message(app, state, rest.to_string(), send_to_twitch);
        return Ok(());
    }

    if trimmed.eq_ignore_ascii_case("lurk on") {
        *state.lurk_mode.write() = true;
        let _ = app.emit("status_updated", state.get_status());
        send_bot_message(app, state, "Lurk mode enabled.".to_string(), send_to_twitch);
        return Ok(());
    }
    if trimmed.eq_ignore_ascii_case("lurk off") {
        *state.lurk_mode.write() = false;
        let _ = app.emit("status_updated", state.get_status());
        send_bot_message(app, state, "Lurk mode disabled.".to_string(), send_to_twitch);
        return Ok(());
    }

    if let Some(rest) = trimmed.strip_prefix("model ") {
        state.config.write().providers.primary.model = rest.to_string();
        let _ = app.emit("status_updated", state.get_status());
        send_bot_message(app, state, format!("Model set to {rest}"), send_to_twitch);
        return Ok(());
    }

    if let Some(rest) = trimmed.strip_prefix("todo add ").or_else(|| trimmed.strip_prefix("agent add ")) {
        let mut parts = rest.splitn(2, ' ');
        let minutes_raw = parts.next().unwrap_or_default().trim();
        let content = parts.next().unwrap_or_default().trim();
        let minutes = minutes_raw.parse::<i64>().map_err(|_| "todo add requires minutes as an integer".to_string())?;
        if minutes <= 0 || minutes > 10080 {
            return Err("todo add minutes must be between 1 and 10080".to_string());
        }
        if content.is_empty() {
            return Err("todo add requires content".to_string());
        }
        let task = ScheduledTodo {
            id: uuid::Uuid::new_v4().to_string().chars().take(8).collect(),
            created_at: chrono::Utc::now().to_rfc3339(),
            due_at: (chrono::Utc::now() + chrono::Duration::minutes(minutes)).to_rfc3339(),
            created_by: sender.to_string(),
            content: content.to_string(),
            recurring_every_minutes: None,
            paused: false,
            run_count: 0,
            last_run_at: None,
        };
        save_task_snapshot(state, sender, &task);
        send_bot_message(
            app,
            state,
            format!("Saved task {} due in {}m: {}", task.id, minutes, task.content),
            send_to_twitch,
        );
        return Ok(());
    }

    if let Some(rest) = trimmed.strip_prefix("todo every ").or_else(|| trimmed.strip_prefix("agent every ")) {
        let mut parts = rest.splitn(2, ' ');
        let minutes_raw = parts.next().unwrap_or_default().trim();
        let content = parts.next().unwrap_or_default().trim();
        let minutes = minutes_raw.parse::<i64>().map_err(|_| "todo every requires minutes as an integer".to_string())?;
        if minutes <= 0 || minutes > 1440 {
            return Err("todo every minutes must be between 1 and 1440".to_string());
        }
        if content.is_empty() {
            return Err("todo every requires content".to_string());
        }
        let task = ScheduledTodo {
            id: uuid::Uuid::new_v4().to_string().chars().take(8).collect(),
            created_at: chrono::Utc::now().to_rfc3339(),
            due_at: (chrono::Utc::now() + chrono::Duration::minutes(minutes)).to_rfc3339(),
            created_by: sender.to_string(),
            content: content.to_string(),
            recurring_every_minutes: Some(minutes),
            paused: false,
            run_count: 0,
            last_run_at: None,
        };
        save_task_snapshot(state, sender, &task);
        send_bot_message(
            app,
            state,
            format!("Saved recurring task {} every {}m: {}", task.id, minutes, task.content),
            send_to_twitch,
        );
        return Ok(());
    }

    if trimmed.eq_ignore_ascii_case("todo list") || trimmed.eq_ignore_ascii_case("agent list") {
        let todos = load_pending_todos(state, 2000);
        if todos.is_empty() {
            send_bot_message(app, state, "No pending tasks.".to_string(), send_to_twitch);
            return Ok(());
        }
        let preview = todos
            .iter()
            .take(8)
            .map(|t| {
                let cadence = t
                    .recurring_every_minutes
                    .map(|m| format!("every {m}m"))
                    .unwrap_or_else(|| "once".to_string());
                format!("{} @ {} [{}] => {}", t.id, t.due_at, cadence, t.content)
            })
            .collect::<Vec<_>>()
            .join(" | ");
        send_bot_message(app, state, format!("Pending tasks: {preview}"), send_to_twitch);
        return Ok(());
    }

    if let Some(id) = trimmed
        .strip_prefix("todo done ")
        .or_else(|| trimmed.strip_prefix("agent done "))
        .map(str::trim)
    {
        if id.is_empty() {
            return Err("todo done requires an id".to_string());
        }
        let marker = ScheduledTodoMarker {
            id: id.to_string(),
            at: chrono::Utc::now().to_rfc3339(),
        };
        let raw = serde_json::to_string(&marker).map_err(|e| e.to_string())?;
        state
            .memory
            .append("todo_done", Some(sender), &raw)
            .map_err(|e| e.to_string())?;
        send_bot_message(app, state, format!("Marked task {id} done."), send_to_twitch);
        return Ok(());
    }

    if let Some(id) = trimmed
        .strip_prefix("todo run ")
        .or_else(|| trimmed.strip_prefix("agent run "))
        .map(str::trim)
    {
        if id.is_empty() {
            return Err("todo run requires an id".to_string());
        }
        let todos = load_pending_todos(state, 2000);
        let Some(task) = todos.into_iter().find(|t| t.id.eq_ignore_ascii_case(id)) else {
            return Err(format!("task {id} not found"));
        };
        execute_scheduled_action(app, state, &task.content, send_to_twitch).await;
        if task.recurring_every_minutes.is_none() {
            let marker = ScheduledTodoMarker {
                id: task.id.clone(),
                at: chrono::Utc::now().to_rfc3339(),
            };
            let raw = serde_json::to_string(&marker).map_err(|e| e.to_string())?;
            state
                .memory
                .append("todo_done", Some(sender), &raw)
                .map_err(|e| e.to_string())?;
        } else {
            let mut updated = task.clone();
            if let Some(every) = updated.recurring_every_minutes {
                updated.due_at = (chrono::Utc::now() + chrono::Duration::minutes(every)).to_rfc3339();
                updated.last_run_at = Some(chrono::Utc::now().to_rfc3339());
                updated.run_count = updated.run_count.saturating_add(1);
                save_task_snapshot(state, sender, &updated);
            }
        }
        return Ok(());
    }

    if trimmed.eq_ignore_ascii_case("todo help") || trimmed.eq_ignore_ascii_case("agent help") {
        send_bot_message(
            app,
            state,
            "Task commands: _todo add/every/list/done/run (aliases: _agent ...). Content supports: say:, search:, command <control-command>.".to_string(),
            send_to_twitch,
        );
        return Ok(());
    }

    send_bot_message(
        app,
        state,
        format!("Unknown command: {trimmed}. Use _menu for the full list."),
        send_to_twitch,
    );
    Ok(())
}

fn extract_search_query(input: &str) -> Option<String> {
    let lowered = input.trim().to_lowercase();
    let prefixes = [
        "!search ",
        "search for ",
        "search ",
        "look up ",
        "google ",
        "do a web search for ",
        "do a search for ",
        "search the web for ",
        "web search for ",
    ];
    for prefix in prefixes {
        if let Some(rest) = lowered.strip_prefix(prefix) {
            let q = rest.trim();
            if !q.is_empty() {
                return Some(q.to_string());
            }
        }
    }

    let infix_markers = [
        " do a web search for ",
        " do a search for ",
        " search the web for ",
        " web search for ",
    ];
    for marker in infix_markers {
        if let Some((_, rest)) = lowered.split_once(marker) {
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
