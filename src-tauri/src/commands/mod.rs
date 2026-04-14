use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::PermissionsExt;

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use base64::Engine;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::{
    app,
    browser::service::{open_isolated_twitch_url, open_url_with_fallback, validate_and_open},
    error::{AppError, AppResult},
    headless::{HeadlessStatus, HealthLight, ModuleHealth},
    personality::engine::PersonalityProfile,
    state::{AppState, ChatMessage, ConnectionState, PipelineInput},
    tts::edge_tts_candidates,
    twitch::eventsub::{smoke_test_streamer_api, EventSubStartConfig},
    twitch::oauth,
    voice::{commands::{parse_voice_command, VoiceCommand}, native_mic, stt},
};

fn map_err<T>(value: AppResult<T>) -> Result<T, String> {
    value.map_err(|e| e.to_string())
}

async fn acquire_stt_permit(
    shared: &Arc<crate::state::SharedState>,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    shared
        .stt_gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| "STT lane is busy. Please wait a moment and retry.".to_string())
}

async fn acquire_tts_permit(
    shared: &Arc<crate::state::SharedState>,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    shared
        .tts_gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| "TTS lane is busy. Please wait a moment and retry.".to_string())
}

async fn acquire_search_permit(
    shared: &Arc<crate::state::SharedState>,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    shared
        .search_gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| "Search lane is busy. Please retry in a few seconds.".to_string())
}

async fn acquire_summarize_permit(
    shared: &Arc<crate::state::SharedState>,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    shared
        .summarize_gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| "Summary lane is busy. Please retry in a few seconds.".to_string())
}

async fn acquire_browser_permit(
    shared: &Arc<crate::state::SharedState>,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    shared
        .browser_gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| "Browser lane is busy. Please retry in a few seconds.".to_string())
}

fn is_placeholder(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty()
        || matches!(
            trimmed,
            "your_bot_username"
                | "replace_bot_username"
                | "your_channel_name"
                | "replace_channel"
                | "your_twitch_client_id"
                | "replace_client_id"
        )
}

fn set_connect_error(app_handle: &AppHandle, shared: &std::sync::Arc<crate::state::SharedState>, msg: String) {
    let _ = app_handle.emit("error_banner", msg.clone());
    shared.diagnostics.write().last_error = Some(msg);
    app::update_twitch_state(shared, ConnectionState::Error);
}

fn normalize_login(value: &str) -> String {
    value.trim().trim_start_matches('#').to_lowercase()
}

fn is_invalid_oauth_error_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("401 unauthorized")
        || lower.contains("invalid oauth token")
        || lower.contains("invalid oauth")
        || lower.contains("invalid token")
        || lower.contains("oauth token")
}

fn short_connect_joke() -> String {
    let now = chrono::Local::now();
    let time = now.format("%I:%M %p").to_string().trim_start_matches('0').to_string();
    let jokes = [
        "Chat linked. I promise only medium-bad decisions.",
        "Connected. Chaos now has a schedule.",
        "Online. My humor patch just deployed.",
        "Connected. Sarcasm latency is nominal.",
    ];
    let idx = (now.timestamp() as usize) % jokes.len();
    format!("{time} - {}", jokes[idx])
}

fn first_existing(candidates: &[PathBuf]) -> Option<String> {
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
}

fn normalize_voice_gate_text(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendModuleView {
    pub name: String,
    pub light: String,
    pub message: String,
    pub restarts: u32,
    pub last_started_at: Option<String>,
    pub last_finished_at: Option<String>,
    pub last_duration_ms: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendControlSnapshot {
    pub connected: bool,
    pub addr: String,
    pub status: Option<HeadlessStatus>,
    pub modules: Vec<BackendModuleView>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendConsoleResult {
    pub ok: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub snapshot: BackendControlSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendControlResponse {
    ok: bool,
    result: Option<String>,
    error: Option<String>,
    status: Option<HeadlessStatus>,
    modules: BTreeMap<String, ModuleHealth>,
}

fn map_health_light(light: &HealthLight) -> String {
    match light {
        HealthLight::Red => "red".to_string(),
        HealthLight::Yellow => "yellow".to_string(),
        HealthLight::Green => "green".to_string(),
    }
}

fn backend_control_addr() -> String {
    if let Ok(addr) = std::env::var("COHOSTD_ADDR") {
        return addr;
    }
    #[cfg(unix)]
    {
        return std::env::temp_dir()
            .join("cohostd.sock")
            .to_string_lossy()
            .to_string();
    }
    #[cfg(windows)]
    {
        "127.0.0.1:44777".to_string()
    }
}

fn map_backend_modules(modules: BTreeMap<String, ModuleHealth>) -> Vec<BackendModuleView> {
    modules
        .into_iter()
        .map(|(name, module)| BackendModuleView {
            name,
            light: map_health_light(&module.light),
            message: module.message,
            restarts: module.restarts,
            last_started_at: module.last_started_at,
            last_finished_at: module.last_finished_at,
            last_duration_ms: module.last_duration_ms,
        })
        .collect::<Vec<_>>()
}

fn map_backend_snapshot(
    addr: String,
    ok: bool,
    status: Option<HeadlessStatus>,
    modules: BTreeMap<String, ModuleHealth>,
    error: Option<String>,
    result: Option<String>,
) -> BackendControlSnapshot {
    BackendControlSnapshot {
        connected: ok,
        addr,
        status,
        modules: map_backend_modules(modules),
        error: error.or(result.filter(|v| v != "ok")),
    }
}

fn detect_cohostd_binary(app_handle: Option<&AppHandle>) -> Option<PathBuf> {
    let exe_name = if cfg!(target_os = "windows") { "cohostd.exe" } else { "cohostd" };
    let mut candidates = Vec::new();
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            candidates.push(parent.join(exe_name));
        }
    }
    if let Some(app) = app_handle {
        if let Ok(resource_dir) = app.path().resource_dir() {
            candidates.push(resource_dir.join(exe_name));
            candidates.push(resource_dir.join("bin").join(exe_name));
            if let Some(parent) = resource_dir.parent() {
                candidates.push(parent.join("MacOS").join(exe_name));
            }
        }
    }
    candidates.push(PathBuf::from("./src-tauri/target/debug").join(exe_name));
    candidates.push(PathBuf::from("./target/debug").join(exe_name));
    candidates.into_iter().find(|p| p.exists())
}

fn cohostd_cargo_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
}

async fn query_backend_snapshot(app_handle: &AppHandle) -> Result<BackendControlSnapshot, String> {
    let addr = backend_control_addr();
    let mut cmd = if let Some(bin) = detect_cohostd_binary(Some(app_handle)) {
        let mut cmd = Command::new(&bin);
        cmd.arg("call").arg("status");
        cmd
    } else if command_in_path("cargo") {
        let mut cmd = Command::new("cargo");
        cmd.arg("run")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(cohostd_cargo_manifest())
            .arg("--bin")
            .arg("cohostd")
            .arg("--")
            .arg("call")
            .arg("status");
        cmd
    } else {
        return Ok(BackendControlSnapshot {
            connected: false,
            addr,
            status: None,
            modules: Vec::new(),
            error: Some("cohostd binary not found".to_string()),
        });
    };
    let output = timeout(Duration::from_secs(8), cmd.output())
        .await
        .map_err(|_| "backend status timed out".to_string())?
        .map_err(|e| format!("backend status launch failed: {e}"))?;
    if !output.status.success() {
        return Ok(BackendControlSnapshot {
            connected: false,
            addr,
            status: None,
            modules: Vec::new(),
            error: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        });
    }
    let response: BackendControlResponse = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("invalid backend status payload: {e}"))?;
    Ok(map_backend_snapshot(
        addr,
        response.ok,
        response.status,
        response.modules,
        response.error,
        response.result,
    ))
}

async fn spawn_backend_daemon(app_handle: &AppHandle) -> Result<(), String> {
    if query_backend_snapshot(app_handle).await.ok().is_some_and(|v| v.connected) {
        return Ok(());
    }
    let mut cmd = if let Some(bin) = detect_cohostd_binary(Some(app_handle)) {
        let mut cmd = Command::new(&bin);
        cmd.arg("daemon");
        cmd
    } else if command_in_path("cargo") {
        let mut cmd = Command::new("cargo");
        cmd.arg("run")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(cohostd_cargo_manifest())
            .arg("--bin")
            .arg("cohostd")
            .arg("--")
            .arg("daemon");
        cmd
    } else {
        return Err("cohostd binary not found".to_string());
    };
    let _child = cmd
        .spawn()
        .map_err(|e| format!("failed spawning cohostd daemon: {e}"))?;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if query_backend_snapshot(app_handle).await.ok().is_some_and(|v| v.connected) {
            return Ok(());
        }
    }
    Err("cohostd daemon did not become ready in time".to_string())
}

pub async fn startup_spawn_backend_daemon(app_handle: &AppHandle) -> Result<(), String> {
    spawn_backend_daemon(app_handle).await
}

async fn run_backend_control_request(
    app_handle: &AppHandle,
    command: &str,
    text: Option<&str>,
    path: Option<&str>,
    label: Option<&str>,
    content: Option<&str>,
) -> Result<BackendConsoleResult, String> {
    let addr = backend_control_addr();
    let mut cmd = if let Some(bin) = detect_cohostd_binary(Some(app_handle)) {
        let mut cmd = Command::new(&bin);
        cmd.arg("call").arg(command);
        cmd
    } else if command_in_path("cargo") {
        let mut cmd = Command::new("cargo");
        cmd.arg("run")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(cohostd_cargo_manifest())
            .arg("--bin")
            .arg("cohostd")
            .arg("--")
            .arg("call")
            .arg(command);
        cmd
    } else {
        return Err("cohostd binary not found".to_string());
    };

    match command {
        "prompt" | "tts" | "voice-smoke" => {
            if let Some(value) = text.map(str::trim).filter(|v| !v.is_empty()) {
                cmd.arg(value);
            }
        }
        "stt-file" => {
            if let Some(value) = path.map(str::trim).filter(|v| !v.is_empty()) {
                cmd.arg(value);
            }
        }
        "pin" => {
            let joined = format!(
                "{}::{}",
                label.map(str::trim).unwrap_or_default(),
                content.map(str::trim).unwrap_or_default()
            );
            cmd.arg(joined);
        }
        "pins" | "status" => {}
        other => return Err(format!("unsupported backend control command: {other}")),
    }

    let output = timeout(Duration::from_secs(15), cmd.output())
        .await
        .map_err(|_| format!("backend command '{command}' timed out"))?
        .map_err(|e| format!("backend command '{command}' failed to launch: {e}"))?;

    if !output.status.success() {
        return Ok(BackendConsoleResult {
            ok: false,
            output: None,
            error: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            snapshot: BackendControlSnapshot {
                connected: false,
                addr,
                status: None,
                modules: Vec::new(),
                error: Some("backend command failed".to_string()),
            },
        });
    }

    let response: BackendControlResponse = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("invalid backend command payload: {e}"))?;
    let snapshot = map_backend_snapshot(
        addr,
        response.ok,
        response.status,
        response.modules,
        response.error.clone(),
        response.result.clone(),
    );
    Ok(BackendConsoleResult {
        ok: response.ok,
        output: response.result,
        error: response.error,
        snapshot,
    })
}

fn spawn_backend_terminal_process(bin: Option<&PathBuf>) -> Result<(), String> {
    let command_line = if let Some(bin) = bin {
        format!("\"{}\" shell", bin.to_string_lossy())
    } else if command_in_path("cargo") {
        format!(
            "cargo run --manifest-path \"{}\" --bin cohostd -- shell",
            cohostd_cargo_manifest().to_string_lossy()
        )
    } else {
        return Err("cohostd binary not found".to_string());
    };
    #[cfg(target_os = "linux")]
    {
        let candidates: Vec<(&str, Vec<String>)> = vec![
            (
                "x-terminal-emulator",
                vec![
                    "-e".to_string(),
                    "bash".to_string(),
                    "-lc".to_string(),
                    command_line.clone(),
                ],
            ),
            (
                "gnome-terminal",
                vec![
                    "--".to_string(),
                    "bash".to_string(),
                    "-lc".to_string(),
                    command_line.clone(),
                ],
            ),
            (
                "konsole",
                vec![
                    "-e".to_string(),
                    "bash".to_string(),
                    "-lc".to_string(),
                    command_line.clone(),
                ],
            ),
            (
                "xfce4-terminal",
                vec![
                    "--command".to_string(),
                    command_line.clone(),
                ],
            ),
            (
                "mate-terminal",
                vec![
                    "--command".to_string(),
                    command_line.clone(),
                ],
            ),
            (
                "xterm",
                vec![
                    "-e".to_string(),
                    command_line.clone(),
                ],
            ),
        ];
        for (terminal, args) in candidates {
            if !command_in_path(terminal) {
                continue;
            }
            let mut cmd = std::process::Command::new(terminal);
            cmd.args(args);
            if cmd.spawn().is_ok() {
                return Ok(());
            }
        }
        return Err("no supported terminal emulator found".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Terminal\" to do script \"{} shell\"",
            bin.to_string_lossy().replace('\"', "\\\"")
        );
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn()
            .map_err(|e| format!("failed to launch Terminal: {e}"))?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args([
                "/K",
                &format!("\"{}\" shell", bin.to_string_lossy()),
            ])
            .spawn()
            .map_err(|e| format!("failed to launch console: {e}"))?;
        return Ok(());
    }
}

fn should_drop_voice_transcript(value: &str) -> bool {
    let normalized = normalize_voice_gate_text(value);
    if normalized.is_empty() {
        return true;
    }
    if matches!(
        normalized.as_str(),
        "water"
            | "water splashing"
            | "splashing"
            | "running water"
            | "dripping water"
            | "rain"
            | "wind"
            | "wind noise"
            | "fan noise"
            | "static"
            | "white noise"
            | "background noise"
            | "noise"
            | "keyboard"
            | "keyboard clicking"
            | "typing"
            | "clicking"
            | "door"
            | "door closing"
            | "knocking"
            | "footsteps"
            | "breathing"
            | "heavy breathing"
            | "coughing"
            | "sneezing"
            | "background conversation"
            | "mumbling"
            | "music"
            | "music playing"
            | "applause"
            | "laughter"
            | "laughing"
    ) {
        return true;
    }
    let words = normalized.split_whitespace().collect::<Vec<_>>();
    if words.len() <= 2
        && matches!(
            normalized.as_str(),
            "uh" | "um" | "huh" | "hmm" | "hm" | "mm" | "ah" | "oh" | "er" | "uhh" | "umm"
        )
    {
        return true;
    }
    false
}

fn command_in_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&paths) {
        let full = dir.join(name);
        if let Ok(meta) = std::fs::metadata(&full) {
            if !meta.is_file() {
                continue;
            }
            #[cfg(target_os = "windows")]
            {
                return true;
            }
            #[cfg(not(target_os = "windows"))]
            {
                if meta.permissions().mode() & 0o111 != 0 {
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SttSetupProgressEvent {
    stage: String,
    progress: u8,
    message: String,
}

fn emit_stt_progress(app_handle: &AppHandle, stage: &str, progress: u8, message: impl Into<String>) {
    let _ = app_handle.emit(
        "stt_setup_progress",
        SttSetupProgressEvent {
            stage: stage.to_string(),
            progress,
            message: message.into(),
        },
    );
}

fn detect_fast_whisper_binary(app_handle: Option<&AppHandle>) -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(app) = app_handle {
        if let Ok(resource_dir) = app.path().resource_dir() {
            let exe = if cfg!(target_os = "windows") {
                "whisper-cli.exe"
            } else {
                "whisper-cli"
            };
            candidates.push(resource_dir.join("assets").join("whisper").join(exe));
            candidates.push(resource_dir.join("whisper").join(exe));
            if cfg!(target_os = "windows") {
                candidates.push(resource_dir.join("assets").join("whisper-win").join(exe));
            } else if cfg!(target_os = "macos") {
                candidates.push(resource_dir.join("assets").join("whisper-macos").join(exe));
            } else {
                candidates.push(resource_dir.join("assets").join("whisper-linux").join(exe));
            }
        }
    }
    let exe = if cfg!(target_os = "windows") {
        "whisper-cli.exe"
    } else {
        "whisper-cli"
    };
    candidates.push(PathBuf::from("./src-tauri/assets/whisper").join(exe));
    if cfg!(target_os = "windows") {
        candidates.push(PathBuf::from("./src-tauri/assets/whisper-win").join(exe));
    } else if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from("./src-tauri/assets/whisper-macos").join(exe));
    } else {
        candidates.push(PathBuf::from("./src-tauri/assets/whisper-linux").join(exe));
    }
    candidates.push(PathBuf::from("/usr/bin/whisper-cli"));
    candidates.push(PathBuf::from("/usr/local/bin/whisper-cli"));
    candidates.push(PathBuf::from("/usr/bin/whisper"));
    candidates.push(PathBuf::from("/usr/local/bin/whisper"));
    candidates.push(PathBuf::from("/opt/homebrew/bin/whisper-cli"));
    candidates.push(PathBuf::from("/opt/homebrew/bin/whisper"));
    if let Some(found) = first_existing(&candidates) {
        return Some(found);
    }
    if command_in_path("whisper-cli") {
        return Some("whisper-cli".to_string());
    }
    if command_in_path("whisper") {
        return Some("whisper".to_string());
    }
    None
}

fn detect_vosk_python_runtime() -> Option<String> {
    let candidates = if cfg!(target_os = "windows") {
        vec![
            PathBuf::from("./.venv-vosk/Scripts/python.exe"),
            PathBuf::from("../.venv-vosk/Scripts/python.exe"),
        ]
    } else {
        vec![
            PathBuf::from("./.venv-vosk/bin/python"),
            PathBuf::from("../.venv-vosk/bin/python"),
        ]
    };
    first_existing(&candidates)
}

fn detect_vosk_model(app_handle: Option<&AppHandle>) -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let model_names = [
        "vosk-model-en-us-0.22",
        "vosk-model-en-us-0.22-lgraph",
        "vosk-model-small-en-us-0.15",
    ];
    if let Some(app) = app_handle {
        if let Ok(resource_dir) = app.path().resource_dir() {
            for name in model_names {
                candidates.push(resource_dir.join("assets").join("vosk").join(name));
                candidates.push(resource_dir.join("vosk").join(name));
            }
        }
        if let Ok(app_data) = app.path().app_data_dir() {
            for name in model_names {
                candidates.push(app_data.join("models").join("vosk").join(name));
            }
        }
    }
    for name in model_names {
        candidates.push(PathBuf::from("./src-tauri/assets/vosk").join(name));
    }
    if let Ok(home) = std::env::var("HOME") {
        for name in model_names {
            candidates.push(PathBuf::from(&home).join(".cache").join("vosk").join(name));
            candidates.push(PathBuf::from(&home).join("models").join("vosk").join(name));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .map(|p| p.to_string_lossy().to_string())
}

fn is_vosk_backend_name(value: &str) -> bool {
    matches!(value.trim(), "vosk" | "vosk-python")
}

fn detect_fast_whisper_model(app_handle: Option<&AppHandle>) -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(app) = app_handle {
        if let Ok(resource_dir) = app.path().resource_dir() {
            candidates.push(resource_dir.join("assets").join("whisper").join("ggml-tiny.en.bin"));
            candidates.push(resource_dir.join("assets").join("whisper").join("ggml-base.en.bin"));
            candidates.push(resource_dir.join("assets").join("whisper").join("ggml-small.en.bin"));
            candidates.push(resource_dir.join("whisper").join("ggml-tiny.en.bin"));
            candidates.push(resource_dir.join("whisper").join("ggml-base.en.bin"));
            candidates.push(resource_dir.join("whisper").join("ggml-small.en.bin"));
            if cfg!(target_os = "windows") {
                candidates.push(resource_dir.join("assets").join("whisper-win").join("ggml-tiny.en.bin"));
                candidates.push(resource_dir.join("assets").join("whisper-win").join("ggml-base.en.bin"));
            } else if cfg!(target_os = "macos") {
                candidates.push(resource_dir.join("assets").join("whisper-macos").join("ggml-tiny.en.bin"));
                candidates.push(resource_dir.join("assets").join("whisper-macos").join("ggml-base.en.bin"));
            } else {
                candidates.push(resource_dir.join("assets").join("whisper-linux").join("ggml-tiny.en.bin"));
                candidates.push(resource_dir.join("assets").join("whisper-linux").join("ggml-base.en.bin"));
            }
        }
        if let Ok(app_data) = app.path().app_data_dir() {
            candidates.push(app_data.join("models").join("whisper").join("ggml-tiny.en.bin"));
            candidates.push(app_data.join("models").join("whisper").join("ggml-base.en.bin"));
            candidates.push(app_data.join("models").join("whisper").join("ggml-small.en.bin"));
        }
    }
    candidates.push(PathBuf::from("./src-tauri/assets/whisper/ggml-tiny.en.bin"));
    candidates.push(PathBuf::from("./src-tauri/assets/whisper/ggml-base.en.bin"));
    candidates.push(PathBuf::from("./src-tauri/assets/whisper/ggml-small.en.bin"));
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(
            PathBuf::from(&home)
                .join(".cache")
                .join("whisper.cpp")
                .join("ggml-tiny.en.bin"),
        );
        candidates.push(
            PathBuf::from(&home)
                .join(".cache")
                .join("whisper.cpp")
                .join("ggml-base.en.bin"),
        );
        candidates.push(
            PathBuf::from(&home)
                .join(".cache")
                .join("whisper.cpp")
                .join("ggml-small.en.bin"),
        );
        candidates.push(PathBuf::from(&home).join("models").join("ggml-tiny.en.bin"));
        candidates.push(PathBuf::from(&home).join("models").join("ggml-base.en.bin"));
        candidates.push(PathBuf::from(&home).join("models").join("whisper").join("ggml-tiny.en.bin"));
        candidates.push(PathBuf::from(&home).join("models").join("whisper").join("ggml-base.en.bin"));
    }
    candidates.push(PathBuf::from("./models/ggml-tiny.en.bin"));
    candidates.push(PathBuf::from("./models/ggml-base.en.bin"));
    candidates.push(PathBuf::from("./models/whisper/ggml-tiny.en.bin"));
    candidates.push(PathBuf::from("./models/whisper/ggml-base.en.bin"));
    first_existing(&candidates)
}

fn is_path_like(binary: &str) -> bool {
    binary.contains('/') || binary.contains('\\')
}

fn can_execute_binary(binary: &str) -> bool {
    if binary.trim().is_empty() {
        return false;
    }
    if is_vosk_backend_name(binary) {
        return detect_vosk_python_runtime().is_some();
    }
    if is_path_like(binary) {
        let path = PathBuf::from(binary);
        if let Ok(meta) = std::fs::metadata(&path) {
            if !meta.is_file() {
                return false;
            }
            #[cfg(target_os = "windows")]
            {
                return true;
            }
            #[cfg(not(target_os = "windows"))]
            {
                return meta.permissions().mode() & 0o111 != 0;
            }
        }
        return false;
    }
    command_in_path(binary)
}

async fn resolve_or_repair_stt_config(
    app_handle: &AppHandle,
    shared: &Arc<crate::state::SharedState>,
) -> Result<crate::config::VoiceConfig, String> {
    let mut cfg = shared.config.read().voice.clone();
    let mut changed = false;

    if let (Some(_runtime), Some(model)) = (detect_vosk_python_runtime(), detect_vosk_model(Some(app_handle))) {
        if !is_vosk_backend_name(cfg.stt_binary_path.as_deref().unwrap_or_default())
            || cfg.stt_model_path.as_deref() != Some(model.as_str())
        {
            cfg.stt_binary_path = Some("vosk".to_string());
            cfg.stt_model_path = Some(model);
            changed = true;
        }
    }

    let current_bin = cfg.stt_binary_path.clone().unwrap_or_default();
    let using_vosk = is_vosk_backend_name(&current_bin);
    let bin_ok = !current_bin.trim().is_empty() && can_execute_binary(current_bin.trim());
    if !bin_ok {
        if let Some(model) = detect_vosk_model(Some(app_handle)) {
            if detect_vosk_python_runtime().is_some() {
                cfg.stt_binary_path = Some("vosk".to_string());
                cfg.stt_model_path = Some(model);
                changed = true;
            }
        } else if let Some(bin) = detect_fast_whisper_binary(Some(app_handle)) {
            cfg.stt_binary_path = Some(bin);
            changed = true;
        }
    }

    let current_model = cfg.stt_model_path.clone().unwrap_or_default();
    let model_ok = if using_vosk || is_vosk_backend_name(cfg.stt_binary_path.as_deref().unwrap_or_default()) {
        !current_model.trim().is_empty() && PathBuf::from(current_model.trim()).is_dir()
    } else {
        !current_model.trim().is_empty() && PathBuf::from(current_model.trim()).is_file()
    };
    if !model_ok {
        if is_vosk_backend_name(cfg.stt_binary_path.as_deref().unwrap_or_default()) {
            if let Some(model) = detect_vosk_model(Some(app_handle)) {
                cfg.stt_model_path = Some(model);
                changed = true;
            }
        } else if let Some(model) = detect_fast_whisper_model(Some(app_handle)) {
            cfg.stt_model_path = Some(model);
            changed = true;
        }
    }

    let ready = cfg
        .stt_binary_path
        .as_deref()
        .is_some_and(|b| !b.trim().is_empty() && can_execute_binary(b))
        && cfg
            .stt_model_path
            .as_deref()
            .is_some_and(|m| {
                let path = PathBuf::from(m.trim());
                if is_vosk_backend_name(cfg.stt_binary_path.as_deref().unwrap_or_default()) {
                    path.is_dir()
                } else {
                    path.is_file()
                }
            });

    if ready && !cfg.stt_enabled {
        cfg.stt_enabled = true;
        changed = true;
    }

    if changed {
        let mut full = shared.config.write();
        full.voice = cfg.clone();
        full.save_to_disk().map_err(|e| e.to_string())?;
    }

    if !ready {
        return Err("STT runtime is missing. Use Voice -> Auto-configure STT, then retry mic.".to_string());
    }

    Ok(cfg)
}

fn silence_wav_base64(duration_ms: u32) -> String {
    let sample_rate: u32 = 16_000;
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let bytes_per_sample = (bits_per_sample / 8) as u32;
    let num_samples = (sample_rate as u64 * duration_ms as u64 / 1000) as u32;
    let data_size = num_samples * bytes_per_sample * channels as u32;
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample;
    let block_align = channels * (bits_per_sample / 8);
    let chunk_size = 36 + data_size;

    let mut out = Vec::with_capacity((44 + data_size) as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&chunk_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    out.resize((44 + data_size) as usize, 0);
    base64::engine::general_purpose::STANDARD.encode(out)
}

async fn synthesize_tts_cloud_with_voice(clean: &str, voice: &str) -> Result<String, String> {
    crate::tts::synthesize_tts_with_voice(clean, voice).await
}

async fn try_download_fast_whisper_model(app_handle: &AppHandle) -> Result<Option<String>, String> {
    let app_data = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
    let dir = app_data.join("models").join("whisper");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("failed creating whisper model directory: {e}"))?;
    let target = dir.join("ggml-tiny.en.bin");
    if target.exists() {
        emit_stt_progress(app_handle, "model_detected", 92, "Whisper model already available.");
        return Ok(Some(target.to_string_lossy().to_string()));
    }

    emit_stt_progress(app_handle, "model_download", 60, "Downloading Whisper tiny model...");
    let url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin";
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("failed downloading whisper model: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("whisper model download failed with status {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&target)
        .await
        .map_err(|e| format!("failed creating whisper model file: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(next) = stream.next().await {
        let chunk = next.map_err(|e| format!("failed reading whisper model payload: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("failed writing whisper model file: {e}"))?;
        downloaded += chunk.len() as u64;
        let progress = if total > 0 {
            let span = ((downloaded.saturating_mul(30)) / total).min(30) as u8;
            60_u8.saturating_add(span)
        } else {
            75
        };
        emit_stt_progress(
            app_handle,
            "model_download",
            progress,
            format!("Downloading model... {} MB", downloaded / (1024 * 1024)),
        );
    }
    file.flush()
        .await
        .map_err(|e| format!("failed finalizing whisper model file: {e}"))?;
    emit_stt_progress(app_handle, "model_download", 92, "Whisper model download complete.");
    Ok(Some(target.to_string_lossy().to_string()))
}

fn detect_built_whisper_binary(src_root: &std::path::Path) -> Option<PathBuf> {
    let mut candidates = vec![
        src_root.join("build").join("bin").join("whisper-cli"),
        src_root.join("build").join("bin").join("main"),
    ];
    if cfg!(target_os = "windows") {
        candidates.push(src_root.join("build").join("bin").join("whisper-cli.exe"));
        candidates.push(src_root.join("build").join("bin").join("main.exe"));
    }
    candidates.into_iter().find(|p| p.exists())
}

async fn try_provision_whisper_binary(app_handle: &AppHandle) -> Result<Option<String>, String> {
    if let Some(found) = detect_fast_whisper_binary(Some(app_handle)) {
        emit_stt_progress(app_handle, "binary_detected", 35, "Whisper executable already available.");
        return Ok(Some(found));
    }

    let app_data = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
    let runtime_dir = app_data.join("whisper-runtime");
    let src_dir = runtime_dir.join("src");
    let bin_dir = runtime_dir.join("bin");
    let target_name = if cfg!(target_os = "windows") { "whisper-cli.exe" } else { "whisper-cli" };
    let target_bin = bin_dir.join(target_name);
    tokio::fs::create_dir_all(&bin_dir)
        .await
        .map_err(|e| format!("failed creating whisper runtime dirs: {e}"))?;

    if target_bin.exists() {
        emit_stt_progress(app_handle, "binary_detected", 35, "Whisper executable already available.");
        return Ok(Some(target_bin.to_string_lossy().to_string()));
    }

    // Build whisper.cpp locally into app data when no system binary exists.
    if !src_dir.exists() {
        emit_stt_progress(app_handle, "clone", 18, "Cloning whisper.cpp...");
        let clone = tokio::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "https://github.com/ggml-org/whisper.cpp.git",
            ])
            .arg(&src_dir)
            .output()
            .await
            .map_err(|e| format!("failed launching git clone: {e}"))?;
        if !clone.status.success() {
            return Err(format!(
                "failed cloning whisper.cpp; ensure git/network is available: {}",
                String::from_utf8_lossy(&clone.stderr).trim()
            ));
        }
    }

    emit_stt_progress(app_handle, "cmake_configure", 28, "Configuring whisper build...");
    let cmake_cfg = tokio::process::Command::new("cmake")
        .current_dir(&src_dir)
        .args([
            "-B",
            "build",
            "-DWHISPER_BUILD_TESTS=OFF",
            "-DWHISPER_BUILD_SERVER=OFF",
            "-DWHISPER_BUILD_EXAMPLES=ON",
        ])
        .output()
        .await
        .map_err(|e| format!("failed launching cmake configure: {e}"))?;
    if !cmake_cfg.status.success() {
        return Err(format!(
            "cmake configure failed: {}",
            String::from_utf8_lossy(&cmake_cfg.stderr).trim()
        ));
    }

    emit_stt_progress(app_handle, "cmake_build", 42, "Building whisper executable...");
    let cmake_build = tokio::process::Command::new("cmake")
        .current_dir(&src_dir)
        .args(["--build", "build", "--config", "Release", "-j", "2"])
        .output()
        .await
        .map_err(|e| format!("failed launching cmake build: {e}"))?;
    if !cmake_build.status.success() {
        return Err(format!(
            "cmake build failed: {}",
            String::from_utf8_lossy(&cmake_build.stderr).trim()
        ));
    }

    let built = detect_built_whisper_binary(&src_dir).ok_or_else(|| {
        "whisper build completed but executable was not found in build/bin".to_string()
    })?;

    tokio::fs::copy(&built, &target_bin)
        .await
        .map_err(|e| format!("failed copying built whisper binary: {e}"))?;
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&target_bin)
            .await
            .map_err(|e| format!("failed reading binary permissions: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&target_bin, perms)
            .await
            .map_err(|e| format!("failed setting executable permissions: {e}"))?;
    }
    emit_stt_progress(app_handle, "binary_ready", 55, "Whisper executable is ready.");
    Ok(Some(target_bin.to_string_lossy().to_string()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthProfileUpdatedEvent {
    bot_username: String,
    channel: String,
    broadcaster_login: Option<String>,
}

fn emit_oauth_profile_updated(
    app_handle: &AppHandle,
    bot_username: String,
    _channel: String,
    broadcaster_login: Option<String>,
) {
    let normalized_broadcaster = broadcaster_login
        .as_deref()
        .map(normalize_login)
        .filter(|v| !v.is_empty());
    let effective_channel = normalized_broadcaster.clone().unwrap_or_default();
    let _ = app_handle.emit(
        "oauth_profile_updated",
        OAuthProfileUpdatedEvent {
            bot_username: normalize_login(&bot_username),
            channel: effective_channel,
            broadcaster_login: normalized_broadcaster,
        },
    );
}

async fn run_streamer_api_smoke_check(
    app_handle: &AppHandle,
    shared: &std::sync::Arc<crate::state::SharedState>,
    source: &str,
) {
    let cfg = shared.config.read().clone();
    let broadcaster_login = cfg
        .twitch
        .broadcaster_login
        .as_deref()
        .map(normalize_login)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| normalize_login(&cfg.twitch.channel));

    if broadcaster_login.is_empty() {
        let _ = app_handle.emit(
            "timeline_event",
            serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "kind": "eventsub_check",
                "content": format!("Streamer API check skipped ({source}): broadcaster login is not set"),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }),
        );
        return;
    }

    let key = broadcaster_token_key(&broadcaster_login);
    let token = match shared.secrets.get_twitch_token(&key).ok().flatten() {
        Some(value) => value,
        None => {
            let msg = format!(
                "Streamer API check failed ({source}): streamer login is required for EventSub ({broadcaster_login})"
            );
            let _ = app_handle.emit("error_banner", msg.clone());
            let _ = app_handle.emit(
                "timeline_event",
                serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "kind": "eventsub_check",
                    "content": msg,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            );
            shared.diagnostics.write().last_error = Some(
                "Streamer API check failed: streamer authentication is missing".to_string(),
            );
            return;
        }
    };

    match smoke_test_streamer_api(&cfg.twitch.client_id, &token, &broadcaster_login).await {
        Ok(summary) => {
            let _ = app_handle.emit(
                "timeline_event",
                serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "kind": "eventsub_check",
                    "content": format!("{summary} ({source})"),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            );
        }
        Err(err) => {
            if is_invalid_oauth_error_message(&err.to_string()) {
                let _ = shared
                    .secrets
                    .clear_twitch_session(&broadcaster_token_key(&broadcaster_login));
                let msg = format!(
                    "Streamer API check skipped ({source}): streamer session expired for {broadcaster_login}; reconnect streamer account"
                );
                let _ = app_handle.emit(
                    "timeline_event",
                    serde_json::json!({
                        "id": uuid::Uuid::new_v4().to_string(),
                        "kind": "eventsub_check",
                        "content": msg,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }),
                );
                shared.diagnostics.write().last_error = None;
                return;
            }
            let msg = format!("Streamer API check failed ({source}): {err}");
            let _ = app_handle.emit(
                "timeline_event",
                serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "kind": "eventsub_check",
                    "content": msg,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            );
            shared.diagnostics.write().last_error = Some(err.to_string());
        }
    }
}

async fn resolve_saved_token(
    shared: &std::sync::Arc<crate::state::SharedState>,
    cfg: &crate::config::AppConfig,
) -> Result<(String, String), String> {
    let key = normalize_login(&cfg.twitch.bot_username);
    if key.is_empty() {
        return Err("Bot username is empty. Connect Bot first.".to_string());
    }
    let token = shared
        .secrets
        .get_twitch_token(&key)
        .map_err(|e| e.to_string())?;

    match token {
        Some(t) => Ok((key, t)),
        None => Err(format!("No Twitch bot token available for '{}'. Run Connect Bot first.", key)),
    }
}

fn broadcaster_token_key(login: &str) -> String {
    format!("broadcaster:{}", normalize_login(login))
}

fn extract_xml_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn select_youtube_caption_track(track_list_xml: &str) -> Option<(String, String)> {
    let mut first: Option<(String, String)> = None;
    for chunk in track_list_xml.split("<track ").skip(1) {
        let tag = match chunk.split('>').next() {
            Some(value) => value,
            None => continue,
        };
        let lang = extract_xml_attr(tag, "lang_code").unwrap_or_else(|| "en".to_string());
        let name = extract_xml_attr(tag, "name").unwrap_or_default();
        if first.is_none() {
            first = Some((lang.clone(), name.clone()));
        }
        if lang.starts_with("en") {
            return Some((lang, name));
        }
    }
    first
}

fn find_balanced_json_array(input: &str, start_idx: usize) -> Option<&str> {
    let bytes = input.as_bytes();
    if *bytes.get(start_idx)? != b'[' {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    for (offset, byte) in bytes[start_idx..].iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if *byte == b'\\' {
                escape = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = start_idx + offset + 1;
                    return input.get(start_idx..end);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_watch_caption_url(page_html: &str) -> Option<String> {
    let marker = "\"captionTracks\":";
    let marker_idx = page_html.find(marker)? + marker.len();
    let relative_array_idx = page_html.get(marker_idx..)?.find('[')?;
    let array_idx = marker_idx + relative_array_idx;
    let raw_array = find_balanced_json_array(page_html, array_idx)?;
    let parsed: serde_json::Value = serde_json::from_str(raw_array).ok()?;
    let tracks = parsed.as_array()?;

    let select = |prefer_en: bool, allow_asr: bool| -> Option<String> {
        tracks.iter().find_map(|track| {
            let base_url = track.get("baseUrl")?.as_str()?.trim();
            let language_code = track
                .get("languageCode")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let kind = track
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            if prefer_en && !language_code.starts_with("en") {
                return None;
            }
            if !allow_asr && kind == "asr" {
                return None;
            }
            if base_url.is_empty() {
                return None;
            }
            Some(base_url.to_string())
        })
    };

    select(true, false)
        .or_else(|| select(true, true))
        .or_else(|| select(false, false))
        .or_else(|| select(false, true))
}

fn account_roles_are_distinct(bot_username: &str, streamer_login: &str) -> bool {
    let bot = normalize_login(bot_username);
    let streamer = normalize_login(streamer_login);
    bot.is_empty() || streamer.is_empty() || bot != streamer
}

fn auth_sessions_view(shared: &Arc<crate::state::SharedState>, cfg: &crate::config::AppConfig) -> AuthSessionsView {
    let bot_username = normalize_login(&cfg.twitch.bot_username);
    let broadcaster_login = cfg
        .twitch
        .broadcaster_login
        .as_deref()
        .map(normalize_login)
        .filter(|v| !v.is_empty());

    let bot_token_present = if !bot_username.is_empty() {
        shared
            .secrets
            .get_twitch_token(&bot_username)
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };

    let streamer_token_present = if let Some(login) = broadcaster_login.as_ref() {
        shared
            .secrets
            .get_twitch_token(&broadcaster_token_key(login))
            .ok()
            .flatten()
            .is_some()
    } else {
        false
    };

    let visible_broadcaster_login = if streamer_token_present {
        broadcaster_login.clone()
    } else {
        None
    };

    AuthSessionsView {
        bot_username,
        bot_token_present,
        channel: visible_broadcaster_login.clone().unwrap_or_default(),
        broadcaster_login: visible_broadcaster_login,
        streamer_token_present,
    }
}

fn service_item_status(
    configured: bool,
    available: bool,
    authenticated: bool,
    active: bool,
    auth_optional: bool,
    active_optional: bool,
) -> String {
    if !configured || !available || (!auth_optional && !authenticated) {
        return "fail".to_string();
    }
    if !active_optional && !active {
        return "warn".to_string();
    }
    "pass".to_string()
}

fn has_streamer_session(
    shared: &std::sync::Arc<crate::state::SharedState>,
    cfg: &crate::config::AppConfig,
) -> bool {
    let broadcaster_login = cfg
        .twitch
        .broadcaster_login
        .as_deref()
        .map(normalize_login)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| normalize_login(&cfg.twitch.channel));
    if broadcaster_login.is_empty() {
        return false;
    }
    shared
        .secrets
        .get_twitch_token(&broadcaster_token_key(&broadcaster_login))
        .ok()
        .flatten()
        .is_some()
}

fn has_bot_session(
    shared: &std::sync::Arc<crate::state::SharedState>,
    cfg: &crate::config::AppConfig,
) -> bool {
    let bot_username = normalize_login(&cfg.twitch.bot_username);
    if bot_username.is_empty() {
        return false;
    }
    shared
        .secrets
        .get_twitch_token(&bot_username)
        .ok()
        .flatten()
        .is_some()
}

async fn resolve_valid_token(
    shared: &std::sync::Arc<crate::state::SharedState>,
    cfg: &crate::config::AppConfig,
) -> Result<(String, String), String> {
    let (key, token) = resolve_saved_token(shared, cfg).await?;
    match oauth::validate_access_token(&token).await {
        Ok(true) => return Ok((key, token)),
        Ok(false) => {}
        Err(_) => return Ok((key, token)),
    }

    let refresh = shared
        .secrets
        .get_twitch_refresh_token(&key)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Twitch access token expired and no refresh token was found. Reconnect Twitch.".to_string())?;
    let client_secret = shared
        .secrets
        .get_twitch_client_secret(&cfg.twitch.client_id)
        .map_err(|e| e.to_string())?;

    let refreshed = oauth::refresh_access_token(&cfg.twitch.client_id, &refresh, client_secret.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    shared
        .secrets
        .set_twitch_token(&key, &refreshed.access_token)
        .map_err(|e| e.to_string())?;
    if let Some(new_refresh) = refreshed.refresh_token {
        let _ = shared.secrets.set_twitch_refresh_token(&key, &new_refresh);
    }
    Ok((key, refreshed.access_token))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchOAuthSettingsInput {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub bot_username: Option<String>,
    pub channel: Option<String>,
    pub broadcaster_login: Option<String>,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchOAuthSettingsView {
    pub client_id: String,
    pub bot_username: String,
    pub channel: String,
    pub broadcaster_login: Option<String>,
    pub redirect_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionsView {
    pub bot_username: String,
    pub bot_token_present: bool,
    pub channel: String,
    pub broadcaster_login: Option<String>,
    pub streamer_token_present: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorSettingsView {
    pub cohost_mode: bool,
    pub scheduled_messages_minutes: Option<u64>,
    pub minimum_reply_interval_ms: Option<u64>,
    pub post_bot_messages_to_twitch: bool,
    pub topic_continuation_mode: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSettingsView {
    pub mode: String,
    pub max_turns_before_pause: u8,
    pub allow_external_topic_changes: bool,
    pub secondary_character_slug: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterStudioSettingsView {
    pub selected_preset: String,
    pub warmth: u8,
    pub humor: u8,
    pub flirt: u8,
    pub edge: u8,
    pub energy: u8,
    pub story: u8,
    pub extra_direction: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarRigSettingsView {
    pub mouth_x: i16,
    pub mouth_y: i16,
    pub mouth_width: u16,
    pub mouth_open: u16,
    pub mouth_softness: u16,
    pub mouth_smile: i16,
    pub mouth_tilt: i16,
    pub mouth_color: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicCallSettingsView {
    pub enabled: bool,
    pub token: String,
    pub default_character_slug: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SttConfigView {
    pub stt_enabled: bool,
    pub stt_binary_path: Option<String>,
    pub stt_model_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicDebugView {
    pub backend: String,
    pub wav_path: String,
    pub transcript: String,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SttAutoConfigResult {
    pub applied: bool,
    pub message: String,
    pub stt_enabled: bool,
    pub stt_binary_path: Option<String>,
    pub stt_model_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsVoiceView {
    pub enabled: bool,
    pub voice_name: Option<String>,
    pub volume_percent: Option<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRuntimeCheck {
    pub name: String,
    pub status: String,
    pub details: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRuntimeReport {
    pub generated_at: String,
    pub overall: String,
    pub stt_ready: bool,
    pub tts_ready: bool,
    pub checks: Vec<VoiceRuntimeCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarImageView {
    pub data_url: String,
    pub file_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTestCheck {
    pub name: String,
    pub status: String,
    pub details: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTestReport {
    pub generated_at: String,
    pub overall: String,
    pub checks: Vec<SelfTestCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealthItem {
    pub id: String,
    pub label: String,
    pub configured: bool,
    pub available: bool,
    pub authenticated: bool,
    pub active: bool,
    pub status: String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealthReport {
    pub generated_at: String,
    pub overall: String,
    pub services: Vec<ServiceHealthItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugBundleResult {
    pub generated_at: String,
    pub path: String,
    pub sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySnapshotView {
    pub log_path: String,
    pub recent: Vec<crate::memory::store::MemoryRecord>,
    pub pinned: Vec<crate::memory::store::PinnedMemoryRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedMemoryInput {
    pub label: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInputFramePayload {
    pub session_id: String,
    pub mode: String,
    pub engine: String,
    pub transcript: String,
    pub normalized_transcript: String,
    pub command_hint: Option<String>,
    pub name_hint: Option<String>,
    pub time_context_iso: String,
    pub final_latency_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubeRemarkRequest {
    pub context: Value,
    pub humor_style: String,
    pub max_remark_length_seconds: u8,
    pub relevance_strictness: u8,
    #[serde(default)]
    pub model_mode: Option<String>,
    pub repetition_memory: Vec<String>,
    #[serde(default)]
    pub topic_history: Option<Vec<String>>,
    #[serde(default)]
    pub recent_remarks: Option<Vec<String>>,
    pub personality_prompt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubeRemarkResponse {
    pub should_speak: bool,
    pub remark: String,
    pub anchor: String,
    pub topic: String,
    pub confidence: f32,
    pub style: String,
    pub estimated_duration_seconds: u8,
    pub skip_reason: Option<String>,
}

fn extract_json_object(raw: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut start = None;
    for (idx, ch) in raw.char_indices() {
        if ch == '{' {
            if start.is_none() {
                start = Some(idx);
            }
            depth += 1;
        } else if ch == '}' {
            if depth == 0 {
                continue;
            }
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start {
                    return Some(raw[s..=idx].to_string());
                }
            }
        }
    }
    None
}

fn resolved_providers(state: &AppState) -> (crate::config::ProviderConfig, Vec<crate::config::ProviderConfig>) {
    fn normalize_provider(p: &mut crate::config::ProviderConfig) {
        let model = p.model.trim().to_lowercase();
        let cloud = p.name.eq_ignore_ascii_case("ollama-cloud");
        if model.contains("qwen2.5vl")
            || model.contains("mistral-small:24b-instruct")
            || model.contains("qwen2.5:14b-instruct")
            || (cloud && (model.contains("llama3.1:8b-instruct") || model.contains("llama3.3:70b-instruct") || model.contains("phi4:14b")))
        {
            if cloud {
                p.model = "qwen3:8b".to_string();
            } else if p.name.eq_ignore_ascii_case("local-ollama") {
                p.model = "llama3.2:3b".to_string();
            } else {
                p.model = "llama3.2:3b".to_string();
            }
        }
        if p.model.trim().is_empty() {
            p.model = if cloud {
                "qwen3:8b".to_string()
            } else {
                "llama3.2:3b".to_string()
            };
        }
        if p.name.eq_ignore_ascii_case("ollama-cloud") && p.timeout_ms < 18_000 {
            p.timeout_ms = 18_000;
        }
        if p.name.eq_ignore_ascii_case("local-ollama") && p.timeout_ms < 8_000 {
            p.timeout_ms = 8_000;
        }
    }

    let cfg = state.0.config.read().clone();
    let mut primary = cfg.providers.primary;
    normalize_provider(&mut primary);
    if primary.api_key.is_none() {
        primary.api_key = state.0.secrets.get_provider_key(&primary.name).ok().flatten();
    }
    let mut fallbacks = cfg.providers.fallbacks;
    for provider in &mut fallbacks {
        normalize_provider(provider);
        if provider.api_key.is_none() {
            provider.api_key = state.0.secrets.get_provider_key(&provider.name).ok().flatten();
        }
    }
    (primary, fallbacks)
}

async fn build_voice_runtime_report(
    app_handle: &AppHandle,
    state: &AppState,
) -> Result<VoiceRuntimeReport, String> {
    let cfg = state.0.config.read().voice.clone();
    let mut checks: Vec<VoiceRuntimeCheck> = Vec::new();
    let mut stt_ready = false;
    let mut tts_ready = false;
    let mut has_fail = false;
    let mut has_warn = false;

    let mut push = |name: &str, status: &str, details: String| {
        if status == "fail" {
            has_fail = true;
        } else if status == "warn" {
            has_warn = true;
        }
        checks.push(VoiceRuntimeCheck {
            name: name.to_string(),
            status: status.to_string(),
            details,
        });
    };

    if cfg.stt_enabled {
        let requested_bin = cfg.stt_binary_path.clone().unwrap_or_default();
        let resolved_bin = if can_execute_binary(&requested_bin) {
            Some(requested_bin)
        } else if detect_vosk_python_runtime().is_some() && detect_vosk_model(Some(app_handle)).is_some() {
            Some("vosk".to_string())
        } else {
            detect_fast_whisper_binary(Some(app_handle))
        };
        match resolved_bin.as_ref() {
            Some(bin) if is_vosk_backend_name(bin) => push(
                "STT backend",
                "pass",
                "Using local Vosk runtime via the project venv.".to_string(),
            ),
            Some(bin) => push("STT binary", "pass", format!("Using STT binary: {bin}")),
            None => push(
                "STT backend",
                "fail",
                "No usable local STT backend found. Install Vosk or whisper-cli.".to_string(),
            ),
        }

        let requested_model = cfg.stt_model_path.clone().unwrap_or_default();
        let resolved_model = if !requested_model.trim().is_empty()
            && if is_vosk_backend_name(cfg.stt_binary_path.as_deref().unwrap_or_default()) {
                PathBuf::from(&requested_model).is_dir()
            } else {
                PathBuf::from(&requested_model).is_file()
            }
        {
            Some(requested_model)
        } else if is_vosk_backend_name(resolved_bin.as_deref().unwrap_or_default()) {
            detect_vosk_model(Some(app_handle))
        } else {
            detect_fast_whisper_model(Some(app_handle))
        };
        match resolved_model.as_ref() {
            Some(model) => push("STT model", "pass", format!("Using STT model: {model}")),
            None => push(
                "STT model",
                "fail",
                "No local STT model found. Configure STT model path or run STT auto-configure.".to_string(),
            ),
        }

        if let (Some(bin), Some(model)) = (resolved_bin, resolved_model) {
            let _permit = acquire_stt_permit(&state.0).await?;
            let mut smoke_cfg = cfg.clone();
            smoke_cfg.stt_enabled = true;
            smoke_cfg.stt_binary_path = Some(bin);
            smoke_cfg.stt_model_path = Some(model);
            let sample = silence_wav_base64(700);
            match timeout(
                Duration::from_secs(20),
                stt::transcribe_base64_audio(&smoke_cfg, &sample, "audio/wav"),
            )
            .await
            {
                Ok(Ok(_)) => {
                    stt_ready = true;
                    push(
                        "STT process smoke test",
                        "pass",
                        if is_vosk_backend_name(smoke_cfg.stt_binary_path.as_deref().unwrap_or_default()) {
                            "Vosk runtime launched and returned a transcript payload.".to_string()
                        } else {
                            "Whisper process launched and returned a transcript payload.".to_string()
                        },
                    );
                }
                Ok(Err(err)) => push("STT process smoke test", "fail", err.to_string()),
                Err(_) => push(
                    "STT process smoke test",
                    "fail",
                    "Timed out while loading STT runtime.".to_string(),
                ),
            }
        }
    } else {
        push(
            "STT enabled",
            "warn",
            "STT is disabled in settings. Enable STT for mic transcription.".to_string(),
        );
    }

    if cfg.enabled {
        let voice = cfg
            .voice_name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
            .unwrap_or("en-US-JennyNeural");
        let _permit = acquire_tts_permit(&state.0).await?;
        match timeout(
            Duration::from_secs(25),
            synthesize_tts_cloud_with_voice("voice runtime check", voice),
        )
        .await
        {
            Ok(Ok(payload)) => {
                if payload.starts_with("data:audio/") {
                    tts_ready = true;
                    push(
                        "TTS process smoke test",
                        "pass",
                        format!("edge-tts synthesis succeeded with voice {voice}."),
                    );
                } else {
                    push(
                        "TTS process smoke test",
                        "fail",
                        "TTS returned an invalid audio payload.".to_string(),
                    );
                }
            }
            Ok(Err(err)) => push("TTS process smoke test", "fail", err),
            Err(_) => push(
                "TTS process smoke test",
                "fail",
                "Timed out while loading TTS runtime.".to_string(),
            ),
        }
    } else {
        push(
            "TTS enabled",
            "warn",
            "TTS is disabled in settings. Enable voice output for spoken replies.".to_string(),
        );
    }

    let overall = if has_fail {
        "fail"
    } else if has_warn {
        "warn"
    } else {
        "pass"
    };

    Ok(VoiceRuntimeReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        overall: overall.to_string(),
        stt_ready,
        tts_ready,
        checks,
    })
}

fn sanitized_config_value(cfg: &crate::config::AppConfig) -> Value {
    let mut safe = serde_json::to_value(cfg).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(twitch) = safe.get_mut("twitch").and_then(|v| v.as_object_mut()) {
        twitch.insert("client_secret".to_string(), Value::Null);
        twitch.insert("bot_token".to_string(), Value::Null);
    }
    if let Some(primary) = safe
        .get_mut("providers")
        .and_then(|v| v.get_mut("primary"))
        .and_then(|v| v.as_object_mut())
    {
        primary.insert("api_key".to_string(), Value::Null);
    }
    if let Some(fallbacks) = safe
        .get_mut("providers")
        .and_then(|v| v.get_mut("fallbacks"))
        .and_then(|v| v.as_array_mut())
    {
        for item in fallbacks {
            if let Some(obj) = item.as_object_mut() {
                obj.insert("api_key".to_string(), Value::Null);
            }
        }
    }
    if let Some(search) = safe.get_mut("search").and_then(|v| v.as_object_mut()) {
        search.insert("api_key".to_string(), Value::Null);
    }
    safe
}

async fn build_service_health_report(
    app_handle: &AppHandle,
    state: &AppState,
) -> Result<ServiceHealthReport, String> {
    let shared = state.0.clone();
    let cfg = shared.config.read().clone();
    let auth = auth_sessions_view(&shared, &cfg);
    let diagnostics = shared.diagnostics.read().clone();
    let (primary, _) = resolved_providers(state);
    let provider_available = shared.llm.healthcheck(&primary).await;

    let stt_requested_bin = cfg.voice.stt_binary_path.clone().unwrap_or_default();
    let stt_bin_available = if stt_requested_bin.trim().is_empty() {
        detect_vosk_python_runtime().is_some() || detect_fast_whisper_binary(Some(app_handle)).is_some()
    } else {
        can_execute_binary(&stt_requested_bin)
    };
    let stt_requested_model = cfg.voice.stt_model_path.clone().unwrap_or_default();
    let stt_model_available = if stt_requested_model.trim().is_empty() {
        detect_vosk_model(Some(app_handle)).is_some() || detect_fast_whisper_model(Some(app_handle)).is_some()
    } else if is_vosk_backend_name(&stt_requested_bin) {
        PathBuf::from(&stt_requested_model).is_dir()
    } else {
        PathBuf::from(&stt_requested_model).is_file()
    };
    let tts_available = edge_tts_candidates().into_iter().any(|bin| can_execute_binary(&bin));

    let mut services = Vec::new();
    let push_item = |services: &mut Vec<ServiceHealthItem>,
                     id: &str,
                     label: &str,
                     configured: bool,
                     available: bool,
                     authenticated: bool,
                     active: bool,
                     auth_optional: bool,
                     active_optional: bool,
                     details: Vec<String>| {
        services.push(ServiceHealthItem {
            id: id.to_string(),
            label: label.to_string(),
            configured,
            available,
            authenticated,
            active,
            status: service_item_status(
                configured,
                available,
                authenticated,
                active,
                auth_optional,
                active_optional,
            ),
            details,
        });
    };

    push_item(
        &mut services,
        "twitch_oauth",
        "Twitch OAuth",
        !cfg.twitch.client_id.trim().is_empty(),
        true,
        auth.bot_token_present || auth.streamer_token_present,
        auth.bot_token_present || auth.streamer_token_present,
        false,
        true,
        vec![
            format!("Client ID configured: {}", !cfg.twitch.client_id.trim().is_empty()),
            format!("Redirect URL: {}", cfg.twitch.redirect_url),
        ],
    );
    push_item(
        &mut services,
        "bot_account",
        "Bot Account",
        !auth.bot_username.trim().is_empty(),
        auth.bot_token_present,
        auth.bot_token_present,
        auth.bot_token_present,
        false,
        true,
        vec![
            format!("Bot username: {}", if auth.bot_username.is_empty() { "<unset>" } else { &auth.bot_username }),
            format!("Distinct from streamer: {}", account_roles_are_distinct(&auth.bot_username, auth.broadcaster_login.as_deref().unwrap_or_default())),
        ],
    );
    push_item(
        &mut services,
        "streamer_account",
        "Streamer Account",
        auth.broadcaster_login.as_ref().is_some_and(|v| !v.trim().is_empty()),
        auth.streamer_token_present,
        auth.streamer_token_present,
        auth.streamer_token_present,
        false,
        true,
        vec![
            format!(
                "Broadcaster login: {}",
                auth.broadcaster_login.as_deref().unwrap_or("<unset>")
            ),
            format!(
                "Distinct from bot: {}",
                account_roles_are_distinct(&auth.bot_username, auth.broadcaster_login.as_deref().unwrap_or_default())
            ),
        ],
    );
    push_item(
        &mut services,
        "irc_chat",
        "Twitch IRC Chat",
        auth.bot_token_present && auth.streamer_token_present,
        shared.twitch.is_connected() || matches!(diagnostics.twitch_state, ConnectionState::Connecting),
        auth.bot_token_present,
        shared.twitch.is_connected(),
        false,
        false,
        vec![
            format!("Runtime state: {:?}", diagnostics.twitch_state),
            format!("Target channel: {}", cfg.twitch.channel),
        ],
    );
    push_item(
        &mut services,
        "eventsub",
        "EventSub",
        cfg.twitch.use_eventsub,
        cfg.twitch.use_eventsub,
        auth.streamer_token_present,
        shared.eventsub.is_running(),
        false,
        true,
        vec![format!("EventSub enabled: {}", cfg.twitch.use_eventsub)],
    );
    push_item(
        &mut services,
        "llm_provider",
        "Primary LLM Provider",
        !primary.name.trim().is_empty() && !primary.model.trim().is_empty() && !primary.base_url.trim().is_empty(),
        provider_available,
        primary.name.eq_ignore_ascii_case("local-ollama") || primary.api_key.as_ref().is_some_and(|k| !k.trim().is_empty()),
        matches!(diagnostics.provider_state, ConnectionState::Connected),
        primary.name.eq_ignore_ascii_case("local-ollama"),
        true,
        vec![
            format!("Provider: {}", primary.name),
            format!("Model: {}", primary.model),
            format!("Base URL: {}", primary.base_url),
        ],
    );
    push_item(
        &mut services,
        "web_search",
        "Web Search",
        cfg.search.enabled,
        cfg.search.enabled,
        cfg.search.api_key.as_ref().is_some_and(|k| !k.trim().is_empty()) || cfg.search.provider.eq_ignore_ascii_case("duckduckgo"),
        cfg.search.enabled,
        cfg.search.provider.eq_ignore_ascii_case("duckduckgo"),
        true,
        vec![
            format!("Provider: {}", cfg.search.provider),
            format!("Enabled: {}", cfg.search.enabled),
        ],
    );
    push_item(
        &mut services,
        "stt",
        "Speech To Text",
        cfg.voice.stt_enabled && cfg.voice.stt_binary_path.as_ref().is_some_and(|v| !v.trim().is_empty()) && cfg.voice.stt_model_path.as_ref().is_some_and(|v| !v.trim().is_empty()),
        stt_bin_available && stt_model_available,
        true,
        cfg.voice.stt_enabled,
        true,
        true,
        vec![
            format!("Backend: {}", cfg.voice.stt_binary_path.clone().unwrap_or_else(|| "auto".to_string())),
            format!("Binary available: {}", stt_bin_available),
            format!("Model available: {}", stt_model_available),
        ],
    );
    push_item(
        &mut services,
        "tts",
        "Text To Speech",
        cfg.voice.enabled,
        tts_available,
        true,
        cfg.voice.enabled,
        true,
        true,
        vec![
            format!("Configured voice: {}", cfg.voice.voice_name.clone().unwrap_or_else(|| "auto".to_string())),
            format!("edge-tts available: {}", tts_available),
        ],
    );
    let overall = if services.iter().any(|s| s.status == "fail") {
        "fail"
    } else if services.iter().any(|s| s.status == "warn") {
        "warn"
    } else {
        "pass"
    };

    Ok(ServiceHealthReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        overall: overall.to_string(),
        services,
    })
}

#[tauri::command]
pub async fn get_status(state: tauri::State<'_, AppState>) -> Result<crate::state::AppStatus, String> {
    Ok(state.0.get_status())
}

#[tauri::command]
pub async fn get_twitch_oauth_settings(
    state: tauri::State<'_, AppState>,
) -> Result<TwitchOAuthSettingsView, String> {
    let cfg = state.0.config.read().clone();
    Ok(TwitchOAuthSettingsView {
        client_id: cfg.twitch.client_id,
        bot_username: cfg.twitch.bot_username,
        channel: cfg.twitch.channel,
        broadcaster_login: cfg.twitch.broadcaster_login,
        redirect_url: cfg.twitch.redirect_url,
    })
}

#[tauri::command]
pub async fn get_auth_sessions(
    state: tauri::State<'_, AppState>,
) -> Result<AuthSessionsView, String> {
    let cfg = state.0.config.read().clone();
    Ok(auth_sessions_view(&state.0, &cfg))
}

#[tauri::command]
pub async fn get_behavior_settings(
    state: tauri::State<'_, AppState>,
) -> Result<BehaviorSettingsView, String> {
    let cfg = state.0.config.read().clone();
    Ok(BehaviorSettingsView {
        cohost_mode: cfg.behavior.cohost_mode,
        scheduled_messages_minutes: cfg.behavior.scheduled_messages_minutes,
        minimum_reply_interval_ms: Some(cfg.moderation.minimum_reply_interval_ms),
        post_bot_messages_to_twitch: cfg.behavior.post_bot_messages_to_twitch,
        topic_continuation_mode: cfg.behavior.topic_continuation_mode,
    })
}

#[tauri::command]
pub async fn set_behavior_settings(
    state: tauri::State<'_, AppState>,
    cohost_mode: bool,
    scheduled_messages_minutes: Option<u64>,
    minimum_reply_interval_ms: Option<u64>,
    post_bot_messages_to_twitch: Option<bool>,
    topic_continuation_mode: Option<bool>,
) -> Result<(), String> {
    let mut cfg = state.0.config.write();
    cfg.behavior.cohost_mode = cohost_mode;
    cfg.behavior.scheduled_messages_minutes = scheduled_messages_minutes.filter(|v| *v > 0);
    if let Some(value) = minimum_reply_interval_ms {
        cfg.moderation.minimum_reply_interval_ms = value.clamp(1200, 60_000);
    }
    if let Some(value) = post_bot_messages_to_twitch {
        cfg.behavior.post_bot_messages_to_twitch = value;
    }
    if let Some(value) = topic_continuation_mode {
        cfg.behavior.topic_continuation_mode = value;
    }
    cfg.save_to_disk().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_public_call_settings(
    state: tauri::State<'_, AppState>,
) -> Result<PublicCallSettingsView, String> {
    let cfg = state.0.config.read().clone();
    Ok(PublicCallSettingsView {
        enabled: cfg.public_call.enabled,
        token: cfg.public_call.token,
        default_character_slug: cfg.public_call.default_character_slug,
    })
}

#[tauri::command]
pub async fn set_public_call_settings(
    state: tauri::State<'_, AppState>,
    enabled: bool,
    default_character_slug: Option<String>,
) -> Result<PublicCallSettingsView, String> {
    let mut cfg = state.0.config.write();
    cfg.public_call.enabled = enabled;
    if let Some(slug) = default_character_slug {
        let clean = slug.trim();
        if !clean.is_empty() {
            cfg.public_call.default_character_slug = clean.to_string();
        }
    }
    if cfg.public_call.token.trim().is_empty() {
        cfg.public_call.token = uuid::Uuid::new_v4().to_string();
    }
    cfg.save_to_disk().map_err(|e| e.to_string())?;
    Ok(PublicCallSettingsView {
        enabled: cfg.public_call.enabled,
        token: cfg.public_call.token.clone(),
        default_character_slug: cfg.public_call.default_character_slug.clone(),
    })
}

#[tauri::command]
pub async fn rotate_public_call_token(
    state: tauri::State<'_, AppState>,
) -> Result<PublicCallSettingsView, String> {
    let mut cfg = state.0.config.write();
    cfg.public_call.token = uuid::Uuid::new_v4().to_string();
    cfg.save_to_disk().map_err(|e| e.to_string())?;
    Ok(PublicCallSettingsView {
        enabled: cfg.public_call.enabled,
        token: cfg.public_call.token.clone(),
        default_character_slug: cfg.public_call.default_character_slug.clone(),
    })
}

#[tauri::command]
pub async fn get_stt_config(state: tauri::State<'_, AppState>) -> Result<SttConfigView, String> {
    let cfg = state.0.config.read().clone();
    Ok(SttConfigView {
        stt_enabled: cfg.voice.stt_enabled,
        stt_binary_path: cfg.voice.stt_binary_path,
        stt_model_path: cfg.voice.stt_model_path,
    })
}

#[tauri::command]
pub async fn set_stt_config(
    state: tauri::State<'_, AppState>,
    stt_enabled: bool,
    stt_binary_path: Option<String>,
    stt_model_path: Option<String>,
) -> Result<(), String> {
    let mut cfg = state.0.config.write();
    cfg.voice.stt_enabled = stt_enabled;
    cfg.voice.allow_mic_commands = stt_enabled;
    cfg.voice.stt_binary_path = stt_binary_path
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    cfg.voice.stt_model_path = stt_model_path
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    cfg.save_to_disk().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_tts_voice(state: tauri::State<'_, AppState>) -> Result<TtsVoiceView, String> {
    let cfg = state.0.config.read().clone();
    Ok(TtsVoiceView {
        enabled: cfg.voice.enabled,
        voice_name: cfg.voice.voice_name,
        volume_percent: cfg.voice.volume_percent,
    })
}

#[tauri::command]
pub async fn set_tts_voice(
    state: tauri::State<'_, AppState>,
    voice_name: Option<String>,
) -> Result<(), String> {
    let mut cfg = state.0.config.write();
    cfg.voice.voice_name = voice_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    cfg.save_to_disk().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_tts_volume(
    state: tauri::State<'_, AppState>,
    volume_percent: u8,
) -> Result<(), String> {
    let mut cfg = state.0.config.write();
    cfg.voice.volume_percent = Some(volume_percent.min(100));
    cfg.save_to_disk().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn verify_voice_runtime(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<VoiceRuntimeReport, String> {
    build_voice_runtime_report(&app_handle, &state).await
}

fn avatar_store_paths(app_handle: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let app_data = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
    let dir = app_data.join("avatar");
    let json = dir.join("avatar.json");
    Ok((dir, json))
}

#[tauri::command]
pub async fn save_avatar_image(
    app_handle: AppHandle,
    data_url: String,
    file_name: Option<String>,
) -> Result<AvatarImageView, String> {
    if !data_url.starts_with("data:image/") {
        return Err("avatar must be a data:image payload".to_string());
    }
    if data_url.len() > 15_000_000 {
        return Err("avatar image is too large. Please use a smaller image.".to_string());
    }
    let file_name = file_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);

    let payload = AvatarImageView { data_url, file_name };
    let (dir, json_path) = avatar_store_paths(&app_handle)?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("failed creating avatar store dir: {e}"))?;
    let raw = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    tokio::fs::write(&json_path, raw)
        .await
        .map_err(|e| format!("failed saving avatar image: {e}"))?;
    Ok(payload)
}

#[tauri::command]
pub async fn get_saved_avatar_image(
    app_handle: AppHandle,
) -> Result<Option<AvatarImageView>, String> {
    let (_, json_path) = avatar_store_paths(&app_handle)?;
    if !json_path.exists() {
        return Ok(None);
    }
    let raw = tokio::fs::read(&json_path)
        .await
        .map_err(|e| format!("failed reading saved avatar image: {e}"))?;
    let payload: AvatarImageView =
        serde_json::from_slice(&raw).map_err(|e| format!("saved avatar is invalid: {e}"))?;
    if !payload.data_url.starts_with("data:image/") {
        return Ok(None);
    }
    Ok(Some(payload))
}

#[tauri::command]
pub async fn auto_configure_stt_fast(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SttAutoConfigResult, String> {
    let _permit = acquire_stt_permit(&state.0).await?;
    emit_stt_progress(&app_handle, "start", 3, "Starting Whisper setup...");
    emit_stt_progress(&app_handle, "scan_binary", 8, "Checking local STT runtime...");
    let vosk_runtime = detect_vosk_python_runtime();
    emit_stt_progress(&app_handle, "scan_model", 32, "Checking Vosk model...");
    let vosk_model = detect_vosk_model(Some(&app_handle));
    let (detected_binary, detected_model, backend_label) = if vosk_runtime.is_some() && vosk_model.is_some() {
        (Some("vosk".to_string()), vosk_model, "Vosk")
    } else {
        emit_stt_progress(&app_handle, "fallback_whisper", 48, "Falling back to Whisper runtime...");
        let detected_binary = match detect_fast_whisper_binary(Some(&app_handle)) {
            Some(v) => Some(v),
            None => try_provision_whisper_binary(&app_handle).await?,
        };
        let mut detected_model = detect_fast_whisper_model(Some(&app_handle));
        if detected_model.is_none() {
            detected_model = try_download_fast_whisper_model(&app_handle).await?;
        }
        (detected_binary, detected_model, "Whisper")
    };
    let mut cfg = state.0.config.write();
    cfg.voice.stt_binary_path = detected_binary.clone();
    cfg.voice.stt_model_path = detected_model.clone();
    cfg.voice.stt_enabled = detected_model.is_some() && detected_binary.is_some();
    cfg.voice.allow_mic_commands = cfg.voice.stt_enabled;
    cfg.save_to_disk().map_err(|e| e.to_string())?;

    let applied = cfg.voice.stt_enabled;
    let message = if applied && backend_label == "Vosk" {
        "Fast STT config applied (local Vosk model ready).".to_string()
    } else if applied {
        "Fast STT config applied (model + whisper executable ready).".to_string()
    } else if detected_model.is_some() && detected_binary.is_none() {
        "STT model is ready, but no usable local STT runtime was found.".to_string()
    } else if detected_model.is_none() && detected_binary.is_some() {
        "STT runtime is ready, but model was not found/downloaded. Retry auto-configure.".to_string()
    } else {
        "STT setup incomplete: missing model or runtime.".to_string()
    };
    emit_stt_progress(&app_handle, if applied { "done" } else { "incomplete" }, 100, message.clone());

    Ok(SttAutoConfigResult {
        applied,
        message,
        stt_enabled: cfg.voice.stt_enabled,
        stt_binary_path: cfg.voice.stt_binary_path.clone(),
        stt_model_path: cfg.voice.stt_model_path.clone(),
    })
}

#[tauri::command]
pub async fn clear_auth_sessions(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.0.twitch.disconnect().await;
    state.0.eventsub.stop().await;
    app::update_twitch_state(&state.0, ConnectionState::Disconnected);

    state
        .0
        .secrets
        .clear_all_twitch_sessions()
        .map_err(|e| e.to_string())?;

    {
        let mut cfg = state.0.config.write();
        cfg.twitch.bot_username.clear();
        cfg.twitch.channel.clear();
        cfg.twitch.broadcaster_login = None;
        cfg.twitch.bot_token = None;
        cfg.save_to_disk().map_err(|e| e.to_string())?;
    }

    emit_oauth_profile_updated(&app_handle, String::new(), String::new(), None);
    let _ = app_handle.emit("timeline_event", serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "kind": "oauth",
        "content": "Cleared Twitch auth sessions and reset channel profile",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));
    Ok(())
}

#[tauri::command]
pub async fn clear_bot_session(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.0.twitch.disconnect().await;
    state.0.eventsub.stop().await;
    app::update_twitch_state(&state.0, ConnectionState::Disconnected);

    let (bot_username, channel, broadcaster_login) = {
        let cfg = state.0.config.read();
        (
            normalize_login(&cfg.twitch.bot_username),
            normalize_login(&cfg.twitch.channel),
            cfg.twitch
                .broadcaster_login
                .as_deref()
                .map(normalize_login)
                .filter(|v| !v.is_empty()),
        )
    };

    if !bot_username.is_empty() {
        state
            .0
            .secrets
            .clear_twitch_session(&bot_username)
            .map_err(|e| e.to_string())?;
    }
    if !channel.is_empty() && channel != bot_username {
        let _ = state.0.secrets.clear_twitch_session(&channel);
    }

    {
        let mut cfg = state.0.config.write();
        cfg.twitch.bot_username.clear();
        cfg.twitch.bot_token = None;
        cfg.save_to_disk().map_err(|e| e.to_string())?;
    }

    emit_oauth_profile_updated(&app_handle, String::new(), channel, broadcaster_login);
    let _ = app_handle.emit("timeline_event", serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "kind": "oauth",
        "content": "Cleared Bot auth session",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));
    Ok(())
}

#[tauri::command]
pub async fn clear_streamer_session(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.0.twitch.disconnect().await;
    state.0.eventsub.stop().await;
    app::update_twitch_state(&state.0, ConnectionState::Disconnected);

    let (bot_username, channel, broadcaster_login) = {
        let cfg = state.0.config.read();
        (
            normalize_login(&cfg.twitch.bot_username),
            normalize_login(&cfg.twitch.channel),
            cfg.twitch
                .broadcaster_login
                .as_deref()
                .map(normalize_login)
                .filter(|v| !v.is_empty()),
        )
    };

    if let Some(login) = broadcaster_login.as_ref() {
        state
            .0
            .secrets
            .clear_twitch_session(&broadcaster_token_key(login))
            .map_err(|e| e.to_string())?;
    }

    {
        let mut cfg = state.0.config.write();
        cfg.twitch.broadcaster_login = None;
        cfg.save_to_disk().map_err(|e| e.to_string())?;
    }

    emit_oauth_profile_updated(&app_handle, bot_username, channel, None);
    let _ = app_handle.emit("timeline_event", serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "kind": "oauth",
        "content": "Cleared Streamer auth session",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));
    Ok(())
}

#[tauri::command]
pub async fn synthesize_tts_cloud(
    state: tauri::State<'_, AppState>,
    text: String,
    voice_name: Option<String>,
) -> Result<String, String> {
    let _permit = acquire_tts_permit(&state.0).await?;
    let clean = text.trim();
    if clean.is_empty() {
        return Err("text is required".to_string());
    }
    let voice = voice_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
        .unwrap_or("en-US-JennyNeural");
    synthesize_tts_cloud_with_voice(clean, voice).await
}

#[tauri::command]
pub async fn synthesize_tts_reaction(
    state: tauri::State<'_, AppState>,
    reaction: String,
    voice_name: Option<String>,
) -> Result<String, String> {
    let _permit = acquire_tts_permit(&state.0).await?;
    let voice = voice_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
        .unwrap_or("en-US-JennyNeural");
    let lowered = reaction.trim().to_lowercase();
    let cue = match lowered.as_str() {
        "soft hum" => "mmm...".to_string(),
        "thinking hum" => "hmm...".to_string(),
        "surprised" => "oh!".to_string(),
        "excited" => "ooh!".to_string(),
        "delighted" => "ahh!".to_string(),
        "playful" => "mm-hmm!".to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => return Err("reaction is required".to_string()),
    };
    synthesize_tts_cloud_with_voice(&cue, voice).await
}

#[tauri::command]
pub async fn get_backend_control_snapshot(app_handle: AppHandle) -> Result<BackendControlSnapshot, String> {
    query_backend_snapshot(&app_handle).await
}

#[tauri::command]
pub async fn start_backend_daemon(app_handle: AppHandle) -> Result<BackendControlSnapshot, String> {
    spawn_backend_daemon(&app_handle).await?;
    query_backend_snapshot(&app_handle).await
}

#[tauri::command]
pub async fn run_backend_console_command(
    app_handle: AppHandle,
    command: String,
    text: Option<String>,
    path: Option<String>,
    label: Option<String>,
    content: Option<String>,
) -> Result<BackendConsoleResult, String> {
    run_backend_control_request(
        &app_handle,
        command.trim(),
        text.as_deref(),
        path.as_deref(),
        label.as_deref(),
        content.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn launch_backend_terminal(app_handle: AppHandle) -> Result<(), String> {
    let bin = detect_cohostd_binary(Some(&app_handle));
    spawn_backend_terminal_process(bin.as_ref())
}

#[tauri::command]
pub async fn get_service_health(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ServiceHealthReport, String> {
    build_service_health_report(&app_handle, &state).await
}

#[tauri::command]
pub async fn run_self_test(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SelfTestReport, String> {
    let shared = state.0.clone();
    let cfg = shared.config.read().clone();
    let mut checks: Vec<SelfTestCheck> = Vec::new();
    let mut has_fail = false;
    let mut has_warn = false;

    let mut push = |name: &str, status: &str, details: String| {
        if status == "fail" {
            has_fail = true;
        } else if status == "warn" {
            has_warn = true;
        }
        checks.push(SelfTestCheck {
            name: name.to_string(),
            status: status.to_string(),
            details,
        });
    };

    let channel = normalize_login(&cfg.twitch.channel);
    if channel.is_empty() {
        push("Channel configured", "fail", "Target channel is empty.".to_string());
    } else {
        push("Channel configured", "pass", format!("Target channel is #{}", channel));
    }

    let bot_username = normalize_login(&cfg.twitch.bot_username);
    let bot_token_present = !bot_username.is_empty()
        && shared
            .secrets
            .get_twitch_token(&bot_username)
            .ok()
            .flatten()
            .is_some();
    if bot_token_present {
        push(
            "Bot auth session",
            "pass",
            format!("Bot token is available for {}", bot_username),
        );
    } else {
        push("Bot auth session", "fail", "Bot token missing. Connect Bot first.".to_string());
    }

    let broadcaster_login = cfg
        .twitch
        .broadcaster_login
        .as_deref()
        .map(normalize_login)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| channel.clone());
    let streamer_token = if broadcaster_login.is_empty() {
        None
    } else {
        shared
            .secrets
            .get_twitch_token(&broadcaster_token_key(&broadcaster_login))
            .ok()
            .flatten()
    };
    if cfg.twitch.use_eventsub {
        if streamer_token.is_some() {
            push(
                "Streamer auth session",
                "pass",
                format!("Streamer token present for {}", broadcaster_login),
            );
        } else {
            push(
                "Streamer auth session",
                "fail",
                "Streamer token missing. Connect Streamer first.".to_string(),
            );
        }
    } else {
        push(
            "Streamer auth session",
            "warn",
            "EventSub is disabled; streamer auth not required.".to_string(),
        );
    }

    let diagnostics = shared.diagnostics.read().clone();
    push(
        "Twitch runtime state",
        if matches!(diagnostics.twitch_state, ConnectionState::Connected) {
            "pass"
        } else {
            "warn"
        },
        format!("Current twitch_state={:?}", diagnostics.twitch_state),
    );

    push(
        "IRC transport",
        if shared.twitch.is_connected() { "pass" } else { "warn" },
        if shared.twitch.is_connected() {
            "IRC writer/runtime is active.".to_string()
        } else {
            "IRC runtime not active.".to_string()
        },
    );

    push(
        "EventSub runtime",
        if cfg.twitch.use_eventsub && shared.eventsub.is_running() {
            "pass"
        } else if cfg.twitch.use_eventsub {
            "warn"
        } else {
            "warn"
        },
        if cfg.twitch.use_eventsub {
            if shared.eventsub.is_running() {
                "EventSub runtime is active.".to_string()
            } else {
                "EventSub runtime not active yet.".to_string()
            }
        } else {
            "EventSub disabled by config.".to_string()
        },
    );

    let distinct_roles = account_roles_are_distinct(
        &cfg.twitch.bot_username,
        cfg.twitch.broadcaster_login.as_deref().unwrap_or(&cfg.twitch.channel),
    );
    push(
        "Account role separation",
        if distinct_roles { "pass" } else { "fail" },
        if distinct_roles {
            "Bot and streamer accounts are distinct.".to_string()
        } else {
            "Bot and streamer accounts currently resolve to the same login.".to_string()
        },
    );

    let mut primary = cfg.providers.primary.clone();
    if primary.api_key.is_none() {
        primary.api_key = shared.secrets.get_provider_key(&primary.name).ok().flatten();
    }
    if shared.llm.healthcheck(&primary).await {
        push("Model provider health", "pass", format!("Provider {} is reachable.", primary.name));
    } else {
        push(
            "Model provider health",
            "warn",
            format!("Provider {} healthcheck failed.", primary.name),
        );
    }

    if cfg.twitch.use_eventsub {
        if let Some(token) = streamer_token {
            match smoke_test_streamer_api(&cfg.twitch.client_id, &token, &broadcaster_login).await {
                Ok(msg) => push("Streamer API smoke test", "pass", msg),
                Err(err) => push("Streamer API smoke test", "fail", err.to_string()),
            }
        } else {
            push(
                "Streamer API smoke test",
                "fail",
                "Skipped: streamer token missing.".to_string(),
            );
        }
    } else {
        push(
            "Streamer API smoke test",
            "warn",
            "Skipped: EventSub disabled in config.".to_string(),
        );
    }

    let service_health = build_service_health_report(&app_handle, &state).await?;
    for svc in service_health.services {
        push(
            &format!("Service: {}", svc.label),
            &svc.status,
            svc.details.join(" | "),
        );
    }

    let voice_report = build_voice_runtime_report(&app_handle, &state).await?;
    for check in voice_report.checks {
        push(
            &format!("Voice: {}", check.name),
            &check.status,
            check.details,
        );
    }

    let overall = if has_fail {
        "fail"
    } else if has_warn {
        "warn"
    } else {
        "pass"
    };

    Ok(SelfTestReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        overall: overall.to_string(),
        checks,
    })
}

#[tauri::command]
pub async fn export_debug_bundle(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<DebugBundleResult, String> {
    let generated_at = chrono::Utc::now().to_rfc3339();
    let shared = state.0.clone();
    let cfg = shared.config.read().clone();
    let diagnostics = shared.diagnostics.read().clone();
    let app_status = shared.get_status();
    let auth_sessions = auth_sessions_view(&shared, &cfg);
    let service_health = build_service_health_report(&app_handle, &state).await?;
    let voice_report = build_voice_runtime_report(&app_handle, &state).await?;
    let self_test = run_self_test(app_handle.clone(), state).await?;
    let memory = shared.memory.recent(25).unwrap_or_default();
    let recent_chat = shared.recent_chat.read().iter().cloned().collect::<Vec<_>>();

    let mut payload = BTreeMap::<String, Value>::new();
    payload.insert("generatedAt".to_string(), Value::String(generated_at.clone()));
    payload.insert("config".to_string(), sanitized_config_value(&cfg));
    payload.insert("diagnostics".to_string(), serde_json::to_value(diagnostics).map_err(|e| e.to_string())?);
    payload.insert("appStatus".to_string(), serde_json::to_value(app_status).map_err(|e| e.to_string())?);
    payload.insert("authSessions".to_string(), serde_json::to_value(auth_sessions).map_err(|e| e.to_string())?);
    payload.insert("serviceHealth".to_string(), serde_json::to_value(service_health).map_err(|e| e.to_string())?);
    payload.insert("voiceRuntime".to_string(), serde_json::to_value(voice_report).map_err(|e| e.to_string())?);
    payload.insert("selfTest".to_string(), serde_json::to_value(self_test).map_err(|e| e.to_string())?);
    payload.insert("recentChat".to_string(), serde_json::to_value(recent_chat).map_err(|e| e.to_string())?);
    payload.insert("memory".to_string(), serde_json::to_value(memory).map_err(|e| e.to_string())?);

    let bundle_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("debug-bundles");
    std::fs::create_dir_all(&bundle_dir).map_err(|e| e.to_string())?;
    let file_path = bundle_dir.join(format!(
        "cohost-debug-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    std::fs::write(
        &file_path,
        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(DebugBundleResult {
        generated_at,
        path: file_path.to_string_lossy().to_string(),
        sections: payload.keys().cloned().collect(),
    })
}

#[tauri::command]
pub async fn start_twitch_oauth(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    force_reauth: Option<bool>,
    auth_profile: Option<String>,
    oauth_role: Option<String>,
) -> Result<(), String> {
    let shared = state.0.clone();
    let role = oauth_role
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("bot")
        .to_lowercase();
    let is_streamer_role = role == "streamer";
    {
        let cfg = shared.config.read();
        let invalid_client = cfg.twitch.client_id.trim().is_empty()
            || cfg.twitch.client_id == "your_twitch_client_id"
            || cfg.twitch.client_id == "replace_client_id";

        if invalid_client {
            let msg = "OAuth is not configured. Set twitch.client_id and register redirect URL http://127.0.0.1:37219/callback in your Twitch app."
                .to_string();
            shared.diagnostics.write().last_error = Some(msg.clone());
            let _ = app_handle.emit("error_banner", msg);
            app::update_twitch_state(&shared, ConnectionState::Error);
            return Ok(());
        }
    }

    let force_reauth = force_reauth.unwrap_or(false);
    let cfg = shared.config.read().clone();
    if !force_reauth && !is_streamer_role {
        if let Ok((_key, token)) = resolve_valid_token(&shared, &cfg).await {
            let mut next = cfg.clone();
            if let Ok(user) = oauth::fetch_current_user(&cfg.twitch.client_id, &token).await {
                next.twitch.bot_username = normalize_login(&user.login);
                if is_placeholder(&next.twitch.channel) {
                    next.twitch.channel = next
                        .twitch
                        .broadcaster_login
                        .as_deref()
                        .map(normalize_login)
                        .filter(|v| !v.is_empty())
                        .unwrap_or_default();
                } else {
                    next.twitch.channel = normalize_login(&next.twitch.channel);
                }
                if next
                    .twitch
                    .broadcaster_login
                    .as_ref()
                    .is_none_or(|v| is_placeholder(v))
                    && !next.twitch.channel.is_empty()
                {
                    next.twitch.broadcaster_login = Some(next.twitch.channel.clone());
                }
                let _ = next.save_to_disk();
                *shared.config.write() = next.clone();
                emit_oauth_profile_updated(
                    &app_handle,
                    next.twitch.bot_username.clone(),
                    next.twitch.channel.clone(),
                    next.twitch.broadcaster_login.clone(),
                );
            }

            let _ = app_handle.emit("timeline_event", serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "kind": "oauth",
                "content": "Using saved Twitch session",
                "timestamp": chrono::Utc::now().to_rfc3339()
            }));
            if let Err(err) = connect_twitch_chat_internal(&app_handle, shared.clone()).await {
                let _ = app_handle.emit(
                    "error_banner",
                    format!("Saved Twitch session found, but join chat failed: {err}"),
                );
                return Err(err);
            }
            return Ok(());
        }
    } else if !force_reauth && is_streamer_role {
        let cfg_now = shared.config.read().clone();
        if let Some(login) = cfg_now
            .twitch
            .broadcaster_login
            .as_deref()
            .map(normalize_login)
            .filter(|v| !v.is_empty())
        {
            let key = broadcaster_token_key(&login);
            if let Ok(Some(token)) = shared.secrets.get_twitch_token(&key) {
                if oauth::validate_access_token(&token).await.unwrap_or(false) {
                    let _ = app_handle.emit("timeline_event", serde_json::json!({
                        "id": uuid::Uuid::new_v4().to_string(),
                        "kind": "oauth",
                        "content": "Using saved streamer session",
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }));
                    run_streamer_api_smoke_check(
                        &app_handle,
                        &shared,
                        "saved streamer login",
                    )
                    .await;
                    return Ok(());
                }
            }
        }
    } else {
        let _ = app_handle.emit("timeline_event", serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "kind": "oauth",
            "content": "Starting Twitch account switch flow",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }));
    }

    app::update_twitch_state(&shared, ConnectionState::Connecting);

    tauri::async_runtime::spawn(async move {
        let cfg = shared.config.read().clone();
        let device_flow = oauth::start_device_code_flow(&cfg).await;
        let device_flow = match device_flow {
            Ok(v) => v,
            Err(err) => {
                let _ = app_handle.emit("error_banner", err.to_string());
                shared.diagnostics.write().last_error = Some(err.to_string());
                app::update_twitch_state(&shared, ConnectionState::Error);
                return;
            }
        };

        let _ = app_handle.emit("timeline_event", serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "kind": "oauth",
            "content": format!("Open {} and confirm code {}", device_flow.verification_uri, device_flow.user_code),
            "timestamp": chrono::Utc::now().to_rfc3339()
        }));
        let verification_url = device_flow
            .verification_uri_complete
            .clone()
            .unwrap_or_else(|| device_flow.verification_uri.clone());
        let _ = app_handle.emit(
            "oauth_device_code",
            serde_json::json!({
                "userCode": device_flow.user_code,
                "verificationUri": device_flow.verification_uri,
                "verificationUrl": verification_url,
                "role": if is_streamer_role { "streamer" } else { "bot" },
            }),
        );

        let profile_name = auth_profile
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("bot-default");
        if let Err(err) = open_isolated_twitch_url(&app_handle, profile_name, &verification_url) {
            let _ = app_handle.emit(
                "timeline_event",
                serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "kind": "oauth",
                    "content": format!("Isolated browser launch failed ({err}), falling back to default browser"),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            );
            if let Err(fallback_err) = open_url_with_fallback(&verification_url) {
                let msg = format!("failed opening browser: {fallback_err}");
                shared.diagnostics.write().last_error = Some(msg.clone());
                let _ = app_handle.emit("error_banner", msg);
                app::update_twitch_state(&shared, ConnectionState::Error);
                return;
            }
        }

        match oauth::poll_device_code_for_token(
            &cfg,
            &device_flow.device_code,
            device_flow.interval,
            device_flow.expires_in,
        )
        .await
        {
            Ok(token_resp) => {
                let mut cfg = shared.config.read().clone();
                let mut token_channel_key = normalize_login(&cfg.twitch.bot_username);
                let existing_broadcaster = cfg
                    .twitch
                    .broadcaster_login
                    .as_deref()
                    .map(normalize_login)
                    .filter(|v| !v.is_empty());
                match oauth::fetch_current_user(&cfg.twitch.client_id, &token_resp.access_token).await {
                    Ok(user) => {
                        let auth_login = normalize_login(&user.login);
                        let current_bot = normalize_login(&cfg.twitch.bot_username);
                        let current_streamer = cfg
                            .twitch
                            .broadcaster_login
                            .as_deref()
                            .map(normalize_login)
                            .filter(|v| !v.is_empty());
                        if is_streamer_role && !current_bot.is_empty() && auth_login == current_bot {
                            let msg = "Streamer account must be different from Bot account. Please sign in with a separate streamer account.".to_string();
                            let _ = app_handle.emit("error_banner", msg.clone());
                            let _ = app_handle.emit("timeline_event", serde_json::json!({
                                "id": uuid::Uuid::new_v4().to_string(),
                                "kind": "oauth",
                                "content": msg,
                                "timestamp": chrono::Utc::now().to_rfc3339()
                            }));
                            app::update_twitch_state(&shared, ConnectionState::Disconnected);
                            return;
                        }
                        if !is_streamer_role
                            && current_streamer.as_ref().is_some_and(|streamer| *streamer == auth_login)
                        {
                            let msg = "Bot account must be different from Streamer account. Please sign in with a separate bot account.".to_string();
                            let _ = app_handle.emit("error_banner", msg.clone());
                            let _ = app_handle.emit("timeline_event", serde_json::json!({
                                "id": uuid::Uuid::new_v4().to_string(),
                                "kind": "oauth",
                                "content": msg,
                                "timestamp": chrono::Utc::now().to_rfc3339()
                            }));
                            app::update_twitch_state(&shared, ConnectionState::Disconnected);
                            return;
                        }
                        if is_streamer_role {
                            cfg.twitch.broadcaster_login = Some(auth_login.clone());
                            cfg.twitch.channel = auth_login.clone();
                            token_channel_key = broadcaster_token_key(&auth_login);
                        } else {
                            cfg.twitch.bot_username = auth_login.clone();
                            cfg.twitch.channel = existing_broadcaster.clone().unwrap_or_default();
                            if cfg
                                .twitch
                                .broadcaster_login
                                .as_ref()
                                .is_none_or(|v| is_placeholder(v))
                                && !cfg.twitch.channel.is_empty()
                            {
                                cfg.twitch.broadcaster_login = Some(cfg.twitch.channel.clone());
                            }
                            token_channel_key = normalize_login(&cfg.twitch.bot_username);
                        }
                    }
                    Err(err) => {
                        let _ = app_handle.emit(
                            "error_banner",
                            format!("OAuth worked, but auto profile fetch failed: {err}"),
                        );
                    }
                }

                if let Err(err) = shared
                    .secrets
                    .set_twitch_token(&token_channel_key, &token_resp.access_token)
                {
                    let _ = app_handle.emit(
                        "error_banner",
                        format!("failed storing OAuth token in keychain: {err}"),
                    );
                }
                if let Some(refresh_token) = token_resp.refresh_token {
                    let _ = shared
                        .secrets
                        .set_twitch_refresh_token(&token_channel_key, &refresh_token);
                }
                cfg.twitch.bot_token = None;
                cfg.twitch.bot_username = normalize_login(&cfg.twitch.bot_username);
                cfg.twitch.channel = normalize_login(&cfg.twitch.channel);
                cfg.twitch.broadcaster_login = cfg
                    .twitch
                    .broadcaster_login
                    .as_deref()
                    .map(normalize_login)
                    .filter(|v| !v.is_empty());
                let _ = cfg.save_to_disk();
                *shared.config.write() = cfg;
                let latest = shared.config.read().clone();
                emit_oauth_profile_updated(
                    &app_handle,
                    latest.twitch.bot_username.clone(),
                    latest.twitch.channel.clone(),
                    latest.twitch.broadcaster_login.clone(),
                );
                app::update_twitch_state(&shared, ConnectionState::Disconnected);
                let _ = app_handle.emit("timeline_event", serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "kind": "oauth",
                    "content": if is_streamer_role {
                        "Streamer authentication successful (secondary)"
                    } else {
                        "Bot authentication successful (primary)"
                    },
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }));
                if !is_streamer_role {
                    let latest = shared.config.read().clone();
                    if !has_streamer_session(&shared, &latest) {
                        let _ = app_handle.emit("timeline_event", serde_json::json!({
                            "id": uuid::Uuid::new_v4().to_string(),
                            "kind": "oauth",
                            "content": "Bot authenticated. Waiting for streamer login before joining chat.",
                            "timestamp": chrono::Utc::now().to_rfc3339()
                        }));
                    } else if let Err(err) = connect_twitch_chat_internal(&app_handle, shared.clone()).await {
                        let _ = app_handle.emit(
                            "error_banner",
                            format!("Authenticated but failed to join chat: {err}"),
                        );
                    }
                } else {
                    let latest = shared.config.read().clone();
                    let bot_ready = shared
                        .secrets
                        .get_twitch_token(&normalize_login(&latest.twitch.bot_username))
                        .ok()
                        .flatten()
                        .is_some();
                    run_streamer_api_smoke_check(
                        &app_handle,
                        &shared,
                        "streamer login success",
                    )
                    .await;
                    if bot_ready {
                        if let Err(err) = connect_twitch_chat_internal(&app_handle, shared.clone()).await {
                            let _ = app_handle.emit(
                                "timeline_event",
                                serde_json::json!({
                                    "id": uuid::Uuid::new_v4().to_string(),
                                    "kind": "irc_error",
                                    "content": format!("Streamer connected, but auto-join failed: {err}"),
                                    "timestamp": chrono::Utc::now().to_rfc3339()
                                }),
                            );
                        }
                    }
                }
            }
            Err(err) => {
                let _ = app_handle.emit("error_banner", err.to_string());
                shared.diagnostics.write().last_error = Some(err.to_string());
                app::update_twitch_state(&shared, ConnectionState::Error);
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn set_twitch_oauth_settings(
    state: tauri::State<'_, AppState>,
    input: TwitchOAuthSettingsInput,
) -> Result<(), String> {
    if input.client_id.trim().is_empty() {
        return Err("clientId is required".to_string());
    }
    {
        let mut cfg = state.0.config.write();
        cfg.twitch.client_id = input.client_id.trim().to_string();
        if let Some(bot_username) = input.bot_username.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            cfg.twitch.bot_username = normalize_login(bot_username);
        }
        if let Some(channel) = input.channel.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            cfg.twitch.channel = normalize_login(channel);
        }
        if let Some(broadcaster_login) = input
            .broadcaster_login
            .as_ref()
            .map(|s| normalize_login(s))
            .filter(|s| !s.is_empty())
        {
            cfg.twitch.broadcaster_login = Some(broadcaster_login);
        }
        if let Some(redirect) = input.redirect_url.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            cfg.twitch.redirect_url = redirect.to_string();
        }
        cfg.twitch.client_secret = None;
        cfg.save_to_disk().map_err(|e| e.to_string())?;
    }

    if let Some(secret) = input.client_secret.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        state
            .0
            .secrets
            .set_twitch_client_secret(&input.client_id, secret)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn connect_twitch_chat_internal(
    app_handle: &AppHandle,
    shared: std::sync::Arc<crate::state::SharedState>,
) -> Result<(), String> {
    app::update_twitch_state(&shared, ConnectionState::Connecting);
    app::try_provider_health_probe(shared.clone());

    let cfg = shared.config.read().clone();
    if !has_bot_session(&shared, &cfg) {
        app::update_twitch_state(&shared, ConnectionState::Disconnected);
        let msg = "Bot login required. Connect Bot first.".to_string();
        let _ = app_handle.emit("timeline_event", serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "kind": "oauth",
            "content": msg.clone(),
            "timestamp": chrono::Utc::now().to_rfc3339()
        }));
        return Err(msg);
    }
    if !has_streamer_session(&shared, &cfg) {
        app::update_twitch_state(&shared, ConnectionState::Disconnected);
        let msg = "Streamer login required. Connect Streamer before joining chat.".to_string();
        let _ = app_handle.emit("timeline_event", serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "kind": "oauth",
            "content": msg.clone(),
            "timestamp": chrono::Utc::now().to_rfc3339()
        }));
        return Err(msg);
    }
    let mut channel = cfg
        .twitch
        .broadcaster_login
        .as_deref()
        .map(normalize_login)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| normalize_login(&cfg.twitch.channel));
    let mut bot_username = normalize_login(&cfg.twitch.bot_username);
    let client_id = cfg.twitch.client_id.clone();
    let token = if let Some(token) = cfg.twitch.bot_token.clone() {
        token
    } else {
        match resolve_valid_token(&shared, &cfg).await {
            Ok((_key, token)) => token,
            Err(err) => {
                set_connect_error(&app_handle, &shared, err.clone());
                return Err(err);
            }
        }
    };

    if is_placeholder(&bot_username) || is_placeholder(&channel) {
        let identity = match oauth::fetch_current_user(&cfg.twitch.client_id, &token).await {
            Ok(v) => v,
            Err(err) => {
                let msg = format!("Failed to resolve Twitch identity for chat connect: {err}");
                set_connect_error(&app_handle, &shared, msg.clone());
                return Err(msg);
            }
        };
        bot_username = identity.login.clone();
        if is_placeholder(&channel) {
            channel = cfg
                .twitch
                .broadcaster_login
                .as_deref()
                .map(normalize_login)
                .filter(|v| !v.is_empty())
                .unwrap_or_default();
        }
        let mut next = cfg.clone();
        next.twitch.bot_username = normalize_login(&bot_username);
        next.twitch.channel = channel.clone();
        if next
            .twitch
            .broadcaster_login
            .as_ref()
            .is_none_or(|v| is_placeholder(v))
        {
            next.twitch.broadcaster_login = Some(channel.clone());
        }
        let _ = next.save_to_disk();
        *shared.config.write() = next;
    }

    if channel.is_empty() {
        let msg = "Target channel is empty. Set the target channel in Advanced OAuth settings or connect the streamer account first.".to_string();
        set_connect_error(app_handle, &shared, msg.clone());
        return Err(msg);
    }
    if bot_username.is_empty() {
        let msg = "Bot username is empty after OAuth. Reconnect Twitch and approve chat scopes.".to_string();
        set_connect_error(app_handle, &shared, msg.clone());
        return Err(msg);
    }

    // Prevent account-role mixups: bot token must not resolve to the streamer account.
    let broadcaster_login = cfg
        .twitch
        .broadcaster_login
        .as_deref()
        .map(normalize_login)
        .filter(|v| !v.is_empty())
        .unwrap_or_default();
    if !broadcaster_login.is_empty() {
        if let Ok(identity) = oauth::fetch_current_user(&cfg.twitch.client_id, &token).await {
            let token_login = normalize_login(&identity.login);
            if token_login == broadcaster_login {
                let _ = shared.secrets.clear_twitch_session(&bot_username);
                let msg = "Bot account token matches streamer account. Reconnect Bot with a separate account.".to_string();
                let _ = app_handle.emit("timeline_event", serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "kind": "oauth",
                    "content": msg.clone(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }));
                app::update_twitch_state(&shared, ConnectionState::Disconnected);
                return Err(msg);
            }
        }
    }

    let _ = shared.secrets.set_twitch_token(&bot_username, &token);

    if let Err(err) = shared
        .twitch
        .connect(
            app_handle.clone(),
            token.clone(),
            bot_username.clone(),
            channel.clone(),
            shared.response_queue_tx.clone(),
        )
        .await
    {
        let msg = format!("Twitch IRC connect failed: {err}");
        set_connect_error(&app_handle, &shared, msg.clone());
        return Err(msg);
    }

    if cfg.twitch.use_eventsub {
        let current_cfg = shared.config.read().clone();
        let broadcaster_login = current_cfg
            .twitch
            .broadcaster_login
            .clone()
            .unwrap_or_else(|| channel.clone());
        let eventsub_token = shared
            .secrets
            .get_twitch_token(&broadcaster_token_key(&broadcaster_login))
            .ok()
            .flatten()
            .unwrap_or_else(|| token.clone());

        let eventsub_cfg = EventSubStartConfig {
            token: eventsub_token,
            client_id: client_id,
            broadcaster_login,
            bot_login: bot_username,
        };

        if let Err(err) = shared
            .eventsub
            .start(app_handle.clone(), eventsub_cfg, shared.response_queue_tx.clone())
            .await
        {
            let _ = app_handle.emit("error_banner", format!("EventSub start failed: {err}"));
        }
    }

    run_streamer_api_smoke_check(app_handle, &shared, "chat connect").await;

    app::update_twitch_state(&shared, ConnectionState::Connected);
    let _ = app_handle.emit("status_updated", shared.get_status());
    let _ = app_handle.emit(
        "bot_response",
        ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            user: shared.config.read().twitch.bot_username.clone(),
            content: short_connect_joke(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            is_bot: true,
        },
    );
    emit_oauth_profile_updated(
        app_handle,
        normalize_login(&shared.config.read().twitch.bot_username),
        normalize_login(&channel),
        shared
            .config
            .read()
            .twitch
            .broadcaster_login
            .as_deref()
            .map(normalize_login)
            .filter(|v| !v.is_empty()),
    );
    Ok(())
}

#[tauri::command]
pub async fn connect_twitch_chat(app_handle: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    connect_twitch_chat_internal(&app_handle, state.0.clone()).await
}

#[tauri::command]
pub async fn disconnect_twitch_chat(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.0.twitch.disconnect().await;
    state.0.eventsub.stop().await;
    app::update_twitch_state(&state.0, ConnectionState::Disconnected);
    Ok(())
}

#[tauri::command]
pub async fn send_chat_message(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    content: String,
) -> Result<(), String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let echo = crate::state::ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        user: state.0.config.read().twitch.bot_username.clone(),
        content: trimmed.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        is_bot: true,
    };
    let _ = app_handle.emit("bot_response", &echo);
    let _ = app_handle.emit("timeline_event", serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "kind": "irc",
        "content": format!("Local bot message queued as {} (Twitch posting disabled)", echo.user),
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));
    Ok(())
}

#[tauri::command]
pub async fn set_model(state: tauri::State<'_, AppState>, model: String) -> Result<(), String> {
    state.0.config.write().providers.primary.model = model;
    map_err(state.0.config.read().save_to_disk())
}

#[tauri::command]
pub async fn set_provider_api_key(
    state: tauri::State<'_, AppState>,
    provider_name: String,
    api_key: String,
) -> Result<(), String> {
    state
        .0
        .secrets
        .set_provider_key(&provider_name, &api_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_provider_api_key(
    state: tauri::State<'_, AppState>,
    provider_name: String,
) -> Result<Option<String>, String> {
    state
        .0
        .secrets
        .get_provider_key(&provider_name)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct ProviderTagsResponse {
    models: Vec<ProviderTagModel>,
}

#[derive(Debug, Deserialize)]
struct ProviderTagModel {
    name: String,
}

#[tauri::command]
pub async fn get_provider_models(
    state: tauri::State<'_, AppState>,
    provider_name: String,
) -> Result<Vec<String>, String> {
    let provider_name = provider_name.trim().to_string();
    if provider_name.is_empty() {
        return Err("provider_name is required".to_string());
    }

    let cfg = state.0.config.read().clone();
    let mut provider = if cfg.providers.primary.name.eq_ignore_ascii_case(&provider_name) {
        cfg.providers.primary
    } else if let Some(found) = cfg
        .providers
        .fallbacks
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&provider_name))
        .cloned()
    {
        found
    } else if provider_name.eq_ignore_ascii_case("ollama-cloud") {
        crate::config::ProviderConfig {
            name: "ollama-cloud".to_string(),
            base_url: "https://ollama.com".to_string(),
            model: "qwen3:8b".to_string(),
            api_key: None,
            timeout_ms: 18_000,
            enabled: true,
        }
    } else {
        return Err(format!("provider {} not found in config", provider_name));
    };

    if provider.api_key.is_none() {
        provider.api_key = state.0.secrets.get_provider_key(&provider.name).ok().flatten();
    }

    let url = format!("{}/api/tags", provider.base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if let Some(key) = provider.api_key {
        req = req.bearer_auth(key);
    }

    let response = timeout(Duration::from_millis(provider.timeout_ms), req.send())
        .await
        .map_err(|_| "provider request timed out".to_string())?
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "<empty>".to_string());
        return Err(format!("provider {} returned {}: {}", provider.name, status, body));
    }

    let payload: ProviderTagsResponse = response.json().await.map_err(|e| e.to_string())?;
    let mut models = payload
        .models
        .into_iter()
        .map(|m| m.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    Ok(models)
}

#[tauri::command]
pub async fn fetch_youtube_timedtext(video_id: String) -> Result<String, String> {
    let video_id = video_id.trim();
    if video_id.is_empty() {
        return Err("video_id is required".to_string());
    }

    let client = reqwest::Client::builder()
        .user_agent("greyok-cohost/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    let track_list_url = format!(
        "https://video.google.com/timedtext?type=list&v={}",
        url::form_urlencoded::byte_serialize(video_id.as_bytes()).collect::<String>()
    );
    let track_list_response = client
        .get(&track_list_url)
        .send()
        .await
        .map_err(|e| format!("timedtext track request failed: {e}"))?;
    if !track_list_response.status().is_success() {
        return Err(format!(
            "timedtext track request failed with {}",
            track_list_response.status()
        ));
    }
    let track_list_xml = track_list_response.text().await.map_err(|e| e.to_string())?;
    let caption_url = if let Some((lang, name)) = select_youtube_caption_track(&track_list_xml) {
        let mut url = format!(
            "https://video.google.com/timedtext?v={}&lang={}&fmt=srv3",
            url::form_urlencoded::byte_serialize(video_id.as_bytes()).collect::<String>(),
            url::form_urlencoded::byte_serialize(lang.as_bytes()).collect::<String>()
        );
        if !name.is_empty() {
            url.push_str("&name=");
            url.push_str(&url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>());
        }
        url
    } else {
        let watch_url = format!(
            "https://www.youtube.com/watch?v={}",
            url::form_urlencoded::byte_serialize(video_id.as_bytes()).collect::<String>()
        );
        let watch_response = client
            .get(&watch_url)
            .send()
            .await
            .map_err(|e| format!("watch page caption fallback failed: {e}"))?;
        if !watch_response.status().is_success() {
            return Err("no published caption tracks were found for this video".to_string());
        }
        let watch_html = watch_response.text().await.map_err(|e| e.to_string())?;
        let Some(mut url) = extract_watch_caption_url(&watch_html) else {
            return Err("no published caption tracks were found for this video".to_string());
        };
        if !url.contains("fmt=") {
            url.push_str(if url.contains('?') { "&fmt=srv3" } else { "?fmt=srv3" });
        }
        url
    };

    let caption_response = client
        .get(&caption_url)
        .send()
        .await
        .map_err(|e| format!("caption request failed: {e}"))?;
    if !caption_response.status().is_success() {
        return Err(format!(
            "caption request failed with {}",
            caption_response.status()
        ));
    }
    caption_response.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn configure_cloud_only_mode(
    state: tauri::State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    if model.trim().is_empty() {
        return Err("model is required".to_string());
    }
    let model_norm = {
        let m = model.trim().to_lowercase();
        if m.contains("qwen2.5vl")
            || m.contains("mistral-small:24b-instruct")
            || m.contains("qwen2.5:14b-instruct")
            || m.contains("llama3.1:8b-instruct")
            || m.contains("llama3.3:70b-instruct")
            || m.contains("phi4:14b")
        {
            "qwen3:8b".to_string()
        } else {
            model.trim().to_string()
        }
    };
    {
        let mut cfg = state.0.config.write();
        cfg.providers.primary.name = "ollama-cloud".to_string();
        cfg.providers.primary.base_url = "https://ollama.com".to_string();
        cfg.providers.primary.model = model_norm;
        cfg.providers.primary.enabled = true;
        cfg.providers.primary.timeout_ms = 18_000;
        cfg.providers.primary.api_key = None;
        let has_local_fallback = cfg
            .providers
            .fallbacks
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case("local-ollama"));
        if !has_local_fallback {
            cfg.providers.fallbacks.push(crate::config::ProviderConfig {
                name: "local-ollama".to_string(),
                base_url: "http://127.0.0.1:11434".to_string(),
                model: "llama3.2:3b".to_string(),
                api_key: None,
                timeout_ms: 8000,
                enabled: true,
            });
        } else {
            for fallback in &mut cfg.providers.fallbacks {
                if fallback.name.eq_ignore_ascii_case("local-ollama") {
                    fallback.enabled = true;
                    if fallback.base_url.trim().is_empty() {
                        fallback.base_url = "http://127.0.0.1:11434".to_string();
                    }
                    if fallback.model.trim().is_empty() || fallback.model.to_lowercase().contains("qwen2.5vl") {
                        fallback.model = "llama3.2:3b".to_string();
                    }
                }
            }
        }
        cfg.save_to_disk().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_voice_enabled(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    *state.0.voice_enabled.write() = enabled;
    {
        let mut cfg = state.0.config.write();
        cfg.voice.enabled = enabled;
        cfg.save_to_disk().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_lurk_mode(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    *state.0.lurk_mode.write() = enabled;
    Ok(())
}

#[tauri::command]
pub async fn search_web(state: tauri::State<'_, AppState>, query: String) -> Result<String, String> {
    let _permit = acquire_search_permit(&state.0).await?;
    let search_cfg = state.0.config.read().search.clone();
    if !search_cfg.enabled {
        return Ok("Web search is disabled in Settings. Enable Search to use web search commands.".to_string());
    }
    map_err(state.0.search.search(&search_cfg, &query).await)
}

#[tauri::command]
pub async fn open_external_url(state: tauri::State<'_, AppState>, url: String) -> Result<(), String> {
    let _permit = acquire_browser_permit(&state.0).await?;
    map_err(validate_and_open(&state.0.config.read().browser, &url))
}

#[tauri::command]
pub async fn open_isolated_twitch_window(
    app_handle: AppHandle,
    profile_name: Option<String>,
    url: String,
) -> Result<(), String> {
    let profile = profile_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("default");
    map_err(open_isolated_twitch_url(&app_handle, profile, &url))
}

#[tauri::command]
pub async fn summarize_chat(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let _permit = acquire_summarize_permit(&state.0).await?;
    summarize_chat_unlocked(state.0.clone()).await
}

async fn summarize_chat_unlocked(state: Arc<crate::state::SharedState>) -> Result<String, String> {
    let recents = map_err(state.memory.recent(20))?;
    if recents.is_empty() {
        return Ok("No memory available to summarize yet.".to_string());
    }

    let content = recents
        .iter()
        .map(|r| format!("{} {}", r.kind, r.content))
        .collect::<Vec<_>>()
        .join("\n");

    let app_state = AppState(state.clone());
    let (primary, fallbacks) = resolved_providers(&app_state);
    let system = "You summarize Twitch chat for streamer context. Keep it concise.";
    map_err(
        state
            .llm
            .generate(
                &primary,
                &fallbacks,
                system,
                &format!("Summarize this recent memory:\n{content}"),
            )
            .await,
    )
}

#[tauri::command]
pub async fn generate_youtube_remark(
    state: tauri::State<'_, AppState>,
    input: YoutubeRemarkRequest,
) -> Result<YoutubeRemarkResponse, String> {
    let previous_excerpt = input
        .context
        .get("previousSegments")
        .and_then(|v| v.as_array())
        .map(|segments| {
            segments
                .iter()
                .rev()
                .take(5)
                .filter_map(|segment| segment.get("text").and_then(|v| v.as_str()))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default();
    let next_excerpt = input
        .context
        .get("nextSegments")
        .and_then(|v| v.as_array())
        .map(|segments| {
            segments
                .iter()
                .take(4)
                .filter_map(|segment| segment.get("text").and_then(|v| v.as_str()))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default();
    let entities = input
        .context
        .get("entities")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let tone = input
        .context
        .get("tone")
        .and_then(|v| v.as_str())
        .unwrap_or("neutral")
        .trim()
        .to_string();
    let topic = input
        .context
        .get("topicSummary")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown topic")
        .trim()
        .to_string();
    let current_segment = input
        .context
        .get("currentSegment")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let seriousness = input
        .context
        .get("seriousnessScore")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let humor_opportunity = input
        .context
        .get("humorOpportunityScore")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let pause_confidence = input
        .context
        .get("pauseConfidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;

    if seriousness >= 0.74 {
        return Ok(YoutubeRemarkResponse {
            should_speak: false,
            remark: String::new(),
            anchor: current_segment.clone(),
            topic,
            confidence: 0.0,
            style: input.humor_style,
            estimated_duration_seconds: 3,
            skip_reason: Some("sensitive segment".to_string()),
        });
    }

    let max_len = input.max_remark_length_seconds.clamp(4, 12);
    let strictness = input.relevance_strictness.clamp(0, 100);
    let repetition = if input.repetition_memory.is_empty() {
        "none".to_string()
    } else {
        input.repetition_memory.iter().take(8).cloned().collect::<Vec<_>>().join(" | ")
    };
    let topic_history = input
        .topic_history
        .iter()
        .flat_map(|items| items.iter().take(8))
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ");
    let recent_remarks = input
        .recent_remarks
        .iter()
        .flat_map(|items| items.iter().take(8))
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ");
    let personality = input
        .personality_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Be funny, sharp, and deeply grounded in context. Prefer relevance and freshness over speed.");

    let system_prompt = "You are a live YouTube co-host.
Return JSON only with keys:
shouldSpeak,remark,anchor,topic,confidence,style,estimatedDurationSeconds,skipReason
Rules:
- Stay anchored to transcript/context evidence.
- Keep remark short and conversational.
- Prioritize context and comedic insight over raw speed.
- Avoid repeated joke structures, repeated targets, and repeated phrasing.
- Mention or clearly imply the current segment topic, claim, behavior, wording, or tone.
- If evidence is weak, shouldSpeak=false with skipReason.
- Do not output markdown, explanation, or extra keys.";

    let user_prompt = format!(
        "style: {}\nmaxRemarkLengthSeconds: {}\nrelevanceStrictness: {}\npersonality: {}\n\
         topicSummary: {}\ncurrentSegment: {}\npreviousTranscript: {}\nnextTranscript: {}\nentities: {}\ntone: {}\n\
         seriousnessScore: {:.3}\nhumorOpportunityScore: {:.3}\npauseConfidence: {:.3}\n\
         recentTopicHistory: {}\nrecentRemarksToAvoid: {}\nrepetitionMemory: {}\n\
         contextJson: {}\n\
         Generate one remark JSON object now. If you cannot clearly anchor the joke to the transcript, return shouldSpeak=false.",
        input.humor_style,
        max_len,
        strictness,
        personality,
        topic,
        current_segment,
        previous_excerpt,
        next_excerpt,
        entities,
        tone,
        seriousness,
        humor_opportunity,
        pause_confidence,
        topic_history,
        recent_remarks,
        repetition,
        input.context
    );

    let requested_mode = input
        .model_mode
        .as_deref()
        .map(str::trim)
        .unwrap_or("medium")
        .to_lowercase();
    let (mut primary, fallbacks) = resolved_providers(&state);
    if primary.name.eq_ignore_ascii_case("ollama-cloud") {
        primary.model = match requested_mode.as_str() {
            "fast" => "qwen3:8b".to_string(),
            "long_context" => "phi4:14b".to_string(),
            _ => "gemma3:12b".to_string(),
        };
    }
    let raw = map_err(
        state
            .0
            .llm
            .generate(&primary, &fallbacks, system_prompt, &user_prompt)
            .await,
    )?;

    let parsed = extract_json_object(&raw)
        .and_then(|json| serde_json::from_str::<YoutubeRemarkResponse>(&json).ok());

    if let Some(mut resp) = parsed {
        resp.style = if resp.style.trim().is_empty() {
            input.humor_style
        } else {
            resp.style
        };
        if resp.estimated_duration_seconds == 0 {
            resp.estimated_duration_seconds = max_len.min(6);
        }
        if resp.remark.trim().is_empty() && resp.should_speak {
            resp.should_speak = false;
            resp.skip_reason = Some("empty remark".to_string());
        }
        return Ok(resp);
    }

    Ok(YoutubeRemarkResponse {
        should_speak: false,
        remark: String::new(),
        anchor: current_segment,
        topic,
        confidence: 0.0,
        style: input.humor_style,
        estimated_duration_seconds: max_len.min(6),
        skip_reason: Some("model output was not valid JSON".to_string()),
    })
}

#[tauri::command]
pub async fn get_personality_profile(state: tauri::State<'_, AppState>) -> Result<PersonalityProfile, String> {
    Ok(state.0.personality.read().clone())
}

#[tauri::command]
pub async fn set_personality_profile(
    state: tauri::State<'_, AppState>,
    profile: PersonalityProfile,
) -> Result<(), String> {
    *state.0.personality.write() = profile;
    {
        let mut cfg = state.0.config.write();
        cfg.personality = state.0.personality.read().clone();
        map_err(cfg.save_to_disk())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_scene_settings(state: tauri::State<'_, AppState>) -> Result<SceneSettingsView, String> {
    let scene = state.0.config.read().scene.clone();
    Ok(SceneSettingsView {
        mode: scene.mode,
        max_turns_before_pause: scene.max_turns_before_pause,
        allow_external_topic_changes: scene.allow_external_topic_changes,
        secondary_character_slug: scene.secondary_character_slug,
    })
}

#[tauri::command]
pub async fn set_scene_settings(
    state: tauri::State<'_, AppState>,
    mode: String,
    max_turns_before_pause: u8,
    allow_external_topic_changes: bool,
    secondary_character_slug: String,
) -> Result<(), String> {
    let mut cfg = state.0.config.write();
    cfg.scene.mode = match mode.trim() {
        "dual_debate" => "dual_debate".to_string(),
        "chat_topic" => "chat_topic".to_string(),
        _ => "solo".to_string(),
    };
    cfg.scene.max_turns_before_pause = max_turns_before_pause.clamp(1, 6);
    cfg.scene.allow_external_topic_changes = allow_external_topic_changes;
    cfg.scene.secondary_character_slug = secondary_character_slug.trim().to_string();
    map_err(cfg.save_to_disk())
}

#[tauri::command]
pub async fn get_character_studio_settings(
    state: tauri::State<'_, AppState>,
) -> Result<CharacterStudioSettingsView, String> {
    let studio = state.0.config.read().character_studio.clone();
    Ok(CharacterStudioSettingsView {
        selected_preset: studio.selected_preset,
        warmth: studio.warmth,
        humor: studio.humor,
        flirt: studio.flirt,
        edge: studio.edge,
        energy: studio.energy,
        story: studio.story,
        extra_direction: studio.extra_direction,
    })
}

#[tauri::command]
pub async fn set_character_studio_settings(
    state: tauri::State<'_, AppState>,
    input: CharacterStudioSettingsView,
) -> Result<(), String> {
    let mut cfg = state.0.config.write();
    cfg.character_studio.selected_preset = input.selected_preset.trim().to_string();
    cfg.character_studio.warmth = input.warmth.clamp(0, 100);
    cfg.character_studio.humor = input.humor.clamp(0, 100);
    cfg.character_studio.flirt = input.flirt.clamp(0, 100);
    cfg.character_studio.edge = input.edge.clamp(0, 100);
    cfg.character_studio.energy = input.energy.clamp(0, 100);
    cfg.character_studio.story = input.story.clamp(0, 100);
    cfg.character_studio.extra_direction = input.extra_direction.trim().to_string();
    map_err(cfg.save_to_disk())
}

#[tauri::command]
pub async fn get_avatar_rig_settings(
    state: tauri::State<'_, AppState>,
) -> Result<AvatarRigSettingsView, String> {
    let rig = state.0.config.read().avatar_rig.clone();
    Ok(AvatarRigSettingsView {
        mouth_x: rig.mouth_x,
        mouth_y: rig.mouth_y,
        mouth_width: rig.mouth_width,
        mouth_open: rig.mouth_open,
        mouth_softness: rig.mouth_softness,
        mouth_smile: rig.mouth_smile,
        mouth_tilt: rig.mouth_tilt,
        mouth_color: rig.mouth_color,
        brow_x: rig.brow_x,
        brow_y: rig.brow_y,
        brow_spacing: rig.brow_spacing,
        brow_arch: rig.brow_arch,
        brow_tilt: rig.brow_tilt,
        brow_thickness: rig.brow_thickness,
        brow_color: rig.brow_color,
        eye_open: rig.eye_open,
        eye_squint: rig.eye_squint,
        head_tilt: rig.head_tilt,
        head_scale: rig.head_scale,
        glow: rig.glow,
        popup_width: rig.popup_width,
        popup_height: rig.popup_height,
    })
}

#[tauri::command]
pub async fn set_avatar_rig_settings(
    state: tauri::State<'_, AppState>,
    input: AvatarRigSettingsView,
) -> Result<(), String> {
    let mut cfg = state.0.config.write();
    cfg.avatar_rig.mouth_x = input.mouth_x.clamp(-100, 100);
    cfg.avatar_rig.mouth_y = input.mouth_y.clamp(-100, 100);
    cfg.avatar_rig.mouth_width = input.mouth_width.clamp(10, 90);
    cfg.avatar_rig.mouth_open = input.mouth_open.clamp(0, 100);
    cfg.avatar_rig.mouth_softness = input.mouth_softness.clamp(0, 100);
    cfg.avatar_rig.mouth_smile = input.mouth_smile.clamp(-60, 60);
    cfg.avatar_rig.mouth_tilt = input.mouth_tilt.clamp(-45, 45);
    cfg.avatar_rig.mouth_color = input.mouth_color.trim().to_string();
    cfg.avatar_rig.brow_x = input.brow_x.clamp(-80, 80);
    cfg.avatar_rig.brow_y = input.brow_y.clamp(-100, 100);
    cfg.avatar_rig.brow_spacing = input.brow_spacing.clamp(12, 90);
    cfg.avatar_rig.brow_arch = input.brow_arch.clamp(-50, 50);
    cfg.avatar_rig.brow_tilt = input.brow_tilt.clamp(-45, 45);
    cfg.avatar_rig.brow_thickness = input.brow_thickness.clamp(2, 30);
    cfg.avatar_rig.brow_color = input.brow_color.trim().to_string();
    cfg.avatar_rig.eye_open = input.eye_open.clamp(0, 100);
    cfg.avatar_rig.eye_squint = input.eye_squint.clamp(0, 100);
    cfg.avatar_rig.head_tilt = input.head_tilt.clamp(-30, 30);
    cfg.avatar_rig.head_scale = input.head_scale.clamp(60, 150);
    cfg.avatar_rig.glow = input.glow.clamp(0, 100);
    cfg.avatar_rig.popup_width = input.popup_width.clamp(220, 640);
    cfg.avatar_rig.popup_height = input.popup_height.clamp(240, 760);
    map_err(cfg.save_to_disk())
}

#[tauri::command]
pub async fn clear_memory(state: tauri::State<'_, AppState>) -> Result<(), String> {
    map_err(state.0.memory.clear())?;
    map_err(
        state
            .0
            .response_queue_tx
            .send(PipelineInput::Manual("Memory reset complete.".to_string()))
            .await
            .map_err(|e| AppError::Internal(e.to_string())),
    )?;
    Ok(())
}

#[tauri::command]
pub async fn get_memory_snapshot(state: tauri::State<'_, AppState>) -> Result<MemorySnapshotView, String> {
    Ok(MemorySnapshotView {
        log_path: state.0.memory.log_path(),
        recent: map_err(state.0.memory.tail(40))?,
        pinned: map_err(state.0.memory.list_pinned())?,
    })
}

#[tauri::command]
pub async fn open_memory_log(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let path = state.0.memory.log_path();
    open::that_detached(&path).map_err(|e| format!("failed opening memory log {path}: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn upsert_pinned_memory(
    state: tauri::State<'_, AppState>,
    input: PinnedMemoryInput,
) -> Result<crate::memory::store::PinnedMemoryRecord, String> {
    let label = input.label.trim();
    let content = input.content.trim();
    if label.is_empty() {
        return Err("Pinned memory label is required.".to_string());
    }
    if content.is_empty() {
        return Err("Pinned memory content is required.".to_string());
    }
    map_err(state.0.memory.upsert_pinned(label, content))
}

#[tauri::command]
pub async fn delete_pinned_memory(
    state: tauri::State<'_, AppState>,
    label: String,
) -> Result<(), String> {
    let clean = label.trim();
    if clean.is_empty() {
        return Err("Pinned memory label is required.".to_string());
    }
    let _ = map_err(state.0.memory.delete_pinned(clean))?;
    Ok(())
}

#[tauri::command]
pub async fn transcribe_local_audio(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    base64_audio: String,
    mime_type: String,
) -> Result<String, String> {
    let cfg = resolve_or_repair_stt_config(&app_handle, &state.0).await?;
    let _permit = acquire_stt_permit(&state.0).await?;
    map_err(stt::transcribe_base64_audio(&cfg, &base64_audio, &mime_type).await)
}

#[tauri::command]
pub async fn transcribe_mic_chunk(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    duration_ms: u64,
) -> Result<String, String> {
    let cfg = resolve_or_repair_stt_config(&app_handle, &state.0).await?;
    let audio_b64 = map_err(native_mic::capture_wav_base64(duration_ms).await)?;
    let _permit = acquire_stt_permit(&state.0).await?;
    map_err(stt::transcribe_base64_audio(&cfg, &audio_b64, "audio/wav").await)
}

#[tauri::command]
pub async fn capture_mic_debug(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    duration_ms: u64,
) -> Result<MicDebugView, String> {
    let cfg = resolve_or_repair_stt_config(&app_handle, &state.0).await?;
    let (audio_b64, debug) = map_err(native_mic::capture_wav_base64_with_debug(duration_ms).await)?;
    let _permit = acquire_stt_permit(&state.0).await?;
    let transcript = map_err(stt::transcribe_base64_audio(&cfg, &audio_b64, "audio/wav").await)?;
    Ok(MicDebugView {
        backend: debug.backend,
        wav_path: debug.wav_path,
        transcript,
        duration_ms: debug.duration_ms,
    })
}

#[tauri::command]
pub async fn handle_voice_command(
    state: tauri::State<'_, AppState>,
    input: String,
) -> Result<String, String> {
    let parsed = parse_voice_command(&input);
    match parsed {
        VoiceCommand::Search(query) => {
            let _permit = acquire_search_permit(&state.0).await?;
            let mut search_cfg = state.0.config.read().search.clone();
            // Keep voice command search consistent with conversational search behavior.
            search_cfg.enabled = true;
            let result = map_err(state.0.search.search(&search_cfg, &query).await)?;
            Ok(format!("Search result: {result}"))
        }
        VoiceCommand::Open(url) => {
            let _permit = acquire_browser_permit(&state.0).await?;
            map_err(validate_and_open(&state.0.config.read().browser, &url))?;
            Ok(format!("Opened: {url}"))
        }
        VoiceCommand::Reply(text) => {
            Ok(format!(
                "Twitch posting is disabled. Local reply only: {}",
                text.trim()
            ))
        }
        VoiceCommand::SwitchModel(model) => {
            {
                let mut cfg = state.0.config.write();
                cfg.providers.primary.model = model.clone();
                cfg.save_to_disk().map_err(|e| e.to_string())?;
            }
            Ok(format!("Switched model to {model}"))
        }
        VoiceCommand::ToggleLurk => {
            let next = !*state.0.lurk_mode.read();
            *state.0.lurk_mode.write() = next;
            Ok(format!("Lurk mode {}", if next { "enabled" } else { "disabled" }))
        }
        VoiceCommand::ToggleTts => {
            let next = !*state.0.voice_enabled.read();
            *state.0.voice_enabled.write() = next;
            {
                let mut cfg = state.0.config.write();
                cfg.voice.enabled = next;
                cfg.save_to_disk().map_err(|e| e.to_string())?;
            }
            Ok(format!("TTS {}", if next { "enabled" } else { "disabled" }))
        }
        VoiceCommand::Summarize => {
            let _permit = acquire_summarize_permit(&state.0).await?;
            summarize_chat_unlocked(state.0.clone()).await
        }
        VoiceCommand::Unknown => Ok("No voice command matched. Use phrases like 'search for ...' or 'reply to chat ...'.".to_string()),
    }
}

#[tauri::command]
pub async fn submit_streamer_prompt(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    text: String,
) -> Result<(), String> {
    submit_voice_session_prompt_internal(&app_handle, &state.0, text, None).await
}

async fn submit_voice_session_prompt_internal(
    app_handle: &AppHandle,
    shared: &std::sync::Arc<crate::state::SharedState>,
    text: String,
    caller_name: Option<String>,
) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let configured = shared
        .config
        .read()
        .twitch
        .broadcaster_login
        .clone()
        .unwrap_or_else(|| "streamer".to_string());
    let user = caller_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(&configured)
        .to_string();
    let chat = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        user,
        content: trimmed.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        is_bot: false,
    };
    let _ = app_handle.emit("chat_message", &chat);
    map_err(
        shared
            .response_queue_tx
            .send(PipelineInput::LocalChat(chat))
            .await
            .map_err(|e| AppError::Internal(e.to_string())),
    )
}

async fn submit_voice_session_frame_internal(
    app_handle: &AppHandle,
    shared: &std::sync::Arc<crate::state::SharedState>,
    frame: VoiceInputFramePayload,
    caller_name: Option<String>,
) -> Result<(), String> {
    if should_drop_voice_transcript(&frame.transcript)
        || should_drop_voice_transcript(&frame.normalized_transcript)
    {
        return Ok(());
    }
    let frame_json = serde_json::to_string(&frame).map_err(|e| e.to_string())?;
    let _ = shared.memory.append_structured(
        "voice_frame",
        caller_name.as_deref(),
        &frame_json,
        frame.name_hint.clone().or_else(|| caller_name.clone()),
        3,
        vec![
            frame.mode.clone(),
            frame.engine.clone(),
            "voice-frame".to_string(),
        ],
        serde_json::to_value(&frame).ok(),
    );
    submit_voice_session_prompt_internal(app_handle, shared, frame.transcript, caller_name).await
}

#[tauri::command]
pub async fn submit_voice_session_prompt(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    text: String,
    caller_name: Option<String>,
) -> Result<(), String> {
    submit_voice_session_prompt_internal(&app_handle, &state.0, text, caller_name).await
}

#[tauri::command]
pub async fn submit_voice_session_frame(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    frame: VoiceInputFramePayload,
    caller_name: Option<String>,
) -> Result<(), String> {
    submit_voice_session_frame_internal(&app_handle, &state.0, frame, caller_name).await
}

#[cfg(test)]
mod tests {
    use super::{
        account_roles_are_distinct, can_execute_binary, extract_watch_caption_url,
        sanitized_config_value, service_item_status,
    };
    use crate::config::AppConfig;

    #[test]
    fn detects_account_role_collisions() {
        assert!(!account_roles_are_distinct("GreyOK__", "greyok__"));
        assert!(account_roles_are_distinct("bot_account", "streamer_account"));
        assert!(account_roles_are_distinct("", "streamer_account"));
    }

    #[test]
    fn service_status_distinguishes_fail_warn_and_pass() {
        assert_eq!(
            service_item_status(false, true, true, true, false, false),
            "fail"
        );
        assert_eq!(
            service_item_status(true, true, true, false, false, false),
            "warn"
        );
        assert_eq!(
            service_item_status(true, true, true, true, false, false),
            "pass"
        );
    }

    #[test]
    fn config_redaction_removes_sensitive_fields() {
        let mut cfg = AppConfig::default();
        cfg.twitch.client_secret = Some("secret".to_string());
        cfg.twitch.bot_token = Some("oauth:test".to_string());
        cfg.providers.primary.api_key = Some("provider".to_string());
        cfg.search.api_key = Some("search".to_string());
        let safe = sanitized_config_value(&cfg);

        assert!(safe["twitch"]["client_secret"].is_null());
        assert!(safe["twitch"]["bot_token"].is_null());
        assert!(safe["providers"]["primary"]["api_key"].is_null());
        assert!(safe["search"]["api_key"].is_null());
    }

    #[test]
    fn executable_detection_requires_real_executable_bits() {
        use std::io::Write;
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fake-bin");
        let mut file = std::fs::File::create(&path).expect("create file");
        writeln!(file, "#!/bin/sh\necho ok").expect("write file");
        #[cfg(unix)]
        {
            let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).expect("chmod");
        }
        assert!(can_execute_binary(path.to_string_lossy().as_ref()));
        assert!(!can_execute_binary(""));
    }

    #[test]
    fn watch_page_caption_fallback_prefers_english_tracks() {
        let html = r#"
            <html><body>
            "captionTracks":[
              {"baseUrl":"https://www.youtube.com/api/timedtext?v=abc123\u0026lang=es","languageCode":"es","kind":""},
              {"baseUrl":"https://www.youtube.com/api/timedtext?v=abc123\u0026lang=en","languageCode":"en","kind":"asr"}
            ]
            </body></html>
        "#;

        let url = extract_watch_caption_url(html).expect("expected caption track");
        assert!(url.contains("lang=en"));
        assert!(url.contains("timedtext"));
    }
}
