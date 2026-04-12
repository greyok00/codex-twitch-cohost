use std::path::PathBuf;
use std::sync::Arc;
#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::PermissionsExt;

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use serde::{Deserialize, Serialize};
use base64::Engine;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::{
    app,
    browser::service::{open_isolated_twitch_url, open_url_with_fallback, validate_and_open},
    error::{AppError, AppResult},
    personality::engine::{PersonalityEngine, PersonalityProfile},
    state::{AppState, ChatMessage, ConnectionState, PipelineInput},
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

fn first_existing(candidates: &[PathBuf]) -> Option<String> {
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
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

fn detect_fast_whisper_model(app_handle: Option<&AppHandle>) -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(app) = app_handle {
        if let Ok(resource_dir) = app.path().resource_dir() {
            candidates.push(resource_dir.join("assets").join("whisper").join("ggml-base.en.bin"));
            candidates.push(resource_dir.join("assets").join("whisper").join("ggml-tiny.en.bin"));
            candidates.push(resource_dir.join("assets").join("whisper").join("ggml-small.en.bin"));
            candidates.push(resource_dir.join("whisper").join("ggml-base.en.bin"));
            candidates.push(resource_dir.join("whisper").join("ggml-tiny.en.bin"));
            candidates.push(resource_dir.join("whisper").join("ggml-small.en.bin"));
            if cfg!(target_os = "windows") {
                candidates.push(resource_dir.join("assets").join("whisper-win").join("ggml-base.en.bin"));
                candidates.push(resource_dir.join("assets").join("whisper-win").join("ggml-tiny.en.bin"));
            } else if cfg!(target_os = "macos") {
                candidates.push(resource_dir.join("assets").join("whisper-macos").join("ggml-base.en.bin"));
                candidates.push(resource_dir.join("assets").join("whisper-macos").join("ggml-tiny.en.bin"));
            } else {
                candidates.push(resource_dir.join("assets").join("whisper-linux").join("ggml-base.en.bin"));
                candidates.push(resource_dir.join("assets").join("whisper-linux").join("ggml-tiny.en.bin"));
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
        candidates.push(PathBuf::from(&home).join("models").join("ggml-base.en.bin"));
        candidates.push(PathBuf::from(&home).join("models").join("whisper").join("ggml-base.en.bin"));
    }
    candidates.push(PathBuf::from("./models/ggml-base.en.bin"));
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

    let current_bin = cfg.stt_binary_path.clone().unwrap_or_default();
    let bin_ok = !current_bin.trim().is_empty() && can_execute_binary(current_bin.trim());
    if !bin_ok {
        if let Some(bin) = detect_fast_whisper_binary(Some(app_handle)) {
            cfg.stt_binary_path = Some(bin);
            changed = true;
        }
    }

    let current_model = cfg.stt_model_path.clone().unwrap_or_default();
    let model_ok = !current_model.trim().is_empty() && PathBuf::from(current_model.trim()).is_file();
    if !model_ok {
        if let Some(model) = detect_fast_whisper_model(Some(app_handle)) {
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
            .is_some_and(|m| !m.trim().is_empty() && PathBuf::from(m.trim()).is_file());

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

fn edge_tts_candidates() -> Vec<String> {
    let mut bins = Vec::new();
    if let Ok(bin) = std::env::var("COHOST_EDGE_TTS_BIN") {
        if !bin.trim().is_empty() {
            bins.push(bin);
        }
    }
    bins.push("../.venv-edge-tts/bin/edge-tts".to_string());
    bins.push("./.venv-edge-tts/bin/edge-tts".to_string());
    bins.push("/home/grey/codex-twitch-cohost/.venv-edge-tts/bin/edge-tts".to_string());
    bins.push("edge-tts".to_string());
    bins
}

async fn synthesize_tts_cloud_with_voice(clean: &str, voice: &str) -> Result<String, String> {
    let tmp = tempfile::tempdir().map_err(|e| format!("tempdir failed: {e}"))?;
    let audio_path = tmp.path().join("edge_tts.mp3");
    let mut last_err: Option<String> = None;

    for bin in edge_tts_candidates() {
        let mut cmd = Command::new(&bin);
        cmd.arg("--voice")
            .arg(voice)
            .arg("--rate")
            .arg("+0%")
            .arg("--text")
            .arg(clean)
            .arg("--write-media")
            .arg(&audio_path);

        let run = timeout(Duration::from_secs(20), cmd.output()).await;
        match run {
            Ok(Ok(output)) => {
                if output.status.success() && audio_path.exists() {
                    let bytes = tokio::fs::read(&audio_path)
                        .await
                        .map_err(|e| format!("failed reading synthesized audio: {e}"))?;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                    return Ok(format!("data:audio/mpeg;base64,{b64}"));
                }
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                last_err = Some(format!("edge-tts failed for {bin}: {}", stderr.trim()));
            }
            Ok(Err(e)) => {
                last_err = Some(format!("edge-tts launch failed for {bin}: {e}"));
            }
            Err(_) => {
                last_err = Some(format!("edge-tts timed out for {bin}"));
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        "edge-tts not available; install it and set COHOST_EDGE_TTS_BIN if needed".to_string()
    }))
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
pub struct SttConfigView {
    pub stt_enabled: bool,
    pub stt_binary_path: Option<String>,
    pub stt_model_path: Option<String>,
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

fn resolved_providers(state: &AppState) -> (crate::config::ProviderConfig, Vec<crate::config::ProviderConfig>) {
    fn normalize_provider(p: &mut crate::config::ProviderConfig) {
        let model = p.model.trim().to_lowercase();
        if model.contains("qwen2.5vl") {
            if p.name.eq_ignore_ascii_case("local-ollama") {
                p.model = "llama3.1:8b-instruct".to_string();
            } else {
                p.model = "qwen3-coder:480b-cloud".to_string();
            }
        }
        if p.name.eq_ignore_ascii_case("ollama-cloud") && p.timeout_ms < 45_000 {
            p.timeout_ms = 45_000;
        }
        if p.name.eq_ignore_ascii_case("local-ollama") && p.timeout_ms < 12_000 {
            p.timeout_ms = 12_000;
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
    let bot_username = normalize_login(&cfg.twitch.bot_username);
    let broadcaster_login = cfg
        .twitch
        .broadcaster_login
        .as_deref()
        .map(normalize_login)
        .filter(|v| !v.is_empty());

    let bot_token_present = if !bot_username.is_empty() {
        state
            .0
            .secrets
            .get_twitch_token(&bot_username)
            .map_err(|e| e.to_string())?
            .is_some()
    } else {
        false
    };

    let streamer_token_present = if let Some(login) = broadcaster_login.as_ref() {
        state
            .0
            .secrets
            .get_twitch_token(&broadcaster_token_key(login))
            .map_err(|e| e.to_string())?
            .is_some()
    } else {
        false
    };
    let visible_broadcaster_login = if streamer_token_present {
        broadcaster_login.clone()
    } else {
        None
    };

    let visible_channel = visible_broadcaster_login.clone().unwrap_or_default();
    Ok(AuthSessionsView {
        bot_username,
        bot_token_present,
        channel: visible_channel,
        broadcaster_login: visible_broadcaster_login,
        streamer_token_present,
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
        } else {
            detect_fast_whisper_binary(Some(&app_handle))
        };
        match resolved_bin.as_ref() {
            Some(bin) => push("STT binary", "pass", format!("Using STT binary: {bin}")),
            None => push(
                "STT binary",
                "fail",
                "No usable whisper executable found. Configure STT binary path or install whisper-cli.".to_string(),
            ),
        }

        let requested_model = cfg.stt_model_path.clone().unwrap_or_default();
        let resolved_model = if !requested_model.trim().is_empty() && PathBuf::from(&requested_model).is_file() {
            Some(requested_model)
        } else {
            detect_fast_whisper_model(Some(&app_handle))
        };
        match resolved_model.as_ref() {
            Some(model) => push("STT model", "pass", format!("Using STT model: {model}")),
            None => push(
                "STT model",
                "fail",
                "No whisper model found. Configure STT model path or run STT auto-configure.".to_string(),
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
                        "Whisper process launched and returned a transcript payload.".to_string(),
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
    emit_stt_progress(&app_handle, "scan_binary", 8, "Checking Whisper executable...");
    let detected_binary = match detect_fast_whisper_binary(Some(&app_handle)) {
        Some(v) => Some(v),
        None => try_provision_whisper_binary(&app_handle).await?,
    };
    emit_stt_progress(&app_handle, "scan_model", 58, "Checking Whisper model...");
    let mut detected_model = detect_fast_whisper_model(Some(&app_handle));
    if detected_model.is_none() {
        detected_model = try_download_fast_whisper_model(&app_handle).await?;
    }
    let mut cfg = state.0.config.write();
    cfg.voice.stt_binary_path = detected_binary.clone();
    cfg.voice.stt_model_path = detected_model.clone();
    cfg.voice.stt_enabled = detected_model.is_some() && detected_binary.is_some();
    cfg.voice.allow_mic_commands = cfg.voice.stt_enabled;
    cfg.save_to_disk().map_err(|e| e.to_string())?;

    let applied = cfg.voice.stt_enabled;
    let message = if applied {
        "Fast STT config applied (model + whisper executable ready).".to_string()
    } else if detected_model.is_some() && detected_binary.is_none() {
        "Whisper model is ready, but whisper executable was not found. Install whisper.cpp (whisper-cli) or set binary path in Advanced Paths.".to_string()
    } else if detected_model.is_none() && detected_binary.is_some() {
        "Whisper executable is ready, but model was not found/downloaded. Retry Install/Repair Whisper.".to_string()
    } else {
        "Whisper setup incomplete: missing model and executable.".to_string()
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
pub async fn run_self_test(state: tauri::State<'_, AppState>) -> Result<SelfTestReport, String> {
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

#[tauri::command]
pub async fn configure_cloud_only_mode(
    state: tauri::State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    if model.trim().is_empty() {
        return Err("model is required".to_string());
    }
    {
        let mut cfg = state.0.config.write();
        cfg.providers.primary.name = "ollama-cloud".to_string();
        cfg.providers.primary.base_url = "https://ollama.com".to_string();
        cfg.providers.primary.model = model.trim().to_string();
        cfg.providers.primary.enabled = true;
        cfg.providers.primary.timeout_ms = 60000;
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
                model: "llama3.1:8b-instruct".to_string(),
                api_key: None,
                timeout_ms: 12000,
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
                        fallback.model = "llama3.1:8b-instruct".to_string();
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
pub async fn get_personality_profile(state: tauri::State<'_, AppState>) -> Result<PersonalityProfile, String> {
    Ok(state.0.personality.read().clone())
}

#[tauri::command]
pub async fn set_personality_profile(
    state: tauri::State<'_, AppState>,
    profile: PersonalityProfile,
) -> Result<(), String> {
    PersonalityEngine::save(&state.0.config.read().personality_path, &profile)
        .map_err(|e| e.to_string())?;
    *state.0.personality.write() = profile;
    Ok(())
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
pub async fn transcribe_local_audio(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    base64_audio: String,
    mime_type: String,
) -> Result<String, String> {
    let _permit = acquire_stt_permit(&state.0).await?;
    let cfg = resolve_or_repair_stt_config(&app_handle, &state.0).await?;
    map_err(stt::transcribe_base64_audio(&cfg, &base64_audio, &mime_type).await)
}

#[tauri::command]
pub async fn transcribe_mic_chunk(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    duration_ms: u64,
) -> Result<String, String> {
    let _permit = acquire_stt_permit(&state.0).await?;
    let cfg = resolve_or_repair_stt_config(&app_handle, &state.0).await?;
    let audio_b64 = map_err(native_mic::capture_wav_base64(duration_ms).await)?;
    map_err(stt::transcribe_base64_audio(&cfg, &audio_b64, "audio/wav").await)
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
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let user = state
        .0
        .config
        .read()
        .twitch
        .broadcaster_login
        .clone()
        .unwrap_or_else(|| "streamer".to_string());
    let chat = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        user: user.clone(),
        content: trimmed.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        is_bot: false,
    };
    // Local streamer prompts stay local; do not send to Twitch chat.
    let _ = app_handle.emit("chat_message", &chat);
    map_err(
        state
            .0
            .response_queue_tx
            .send(PipelineInput::LocalChat(chat))
            .await
            .map_err(|e| AppError::Internal(e.to_string())),
    )
}
