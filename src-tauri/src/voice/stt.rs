use std::{fs, path::PathBuf};

use base64::Engine;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::{config::VoiceConfig, error::{AppError, AppResult}};

pub async fn transcribe_base64_audio(config: &VoiceConfig, base64_data: &str, mime_type: &str) -> AppResult<String> {
    if !config.stt_enabled {
        return Err(AppError::Voice("stt is disabled in config".to_string()));
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| AppError::Voice(format!("invalid base64 audio payload: {e}")))?;

    let ext = match mime_type {
        "audio/wav" => "wav",
        "audio/webm" => "webm",
        "audio/ogg" => "ogg",
        _ => "bin",
    };

    let tmp = tempfile::tempdir().map_err(|e| AppError::Voice(format!("tempdir failed: {e}")))?;
    let input_path = tmp.path().join(format!("voice_input.{ext}"));
    fs::write(&input_path, decoded)
        .map_err(|e| AppError::Voice(format!("failed writing temp audio: {e}")))?;

    if is_vosk_backend(config) && ext != "wav" {
        let wav_path = tmp.path().join("voice_input.wav");
        convert_to_wav(&input_path, &wav_path).await?;
        return transcribe_file(config, &wav_path).await;
    }

    transcribe_file(config, &input_path).await
}

pub async fn transcribe_file(config: &VoiceConfig, input_path: &PathBuf) -> AppResult<String> {
    if !config.stt_enabled {
        return Err(AppError::Voice("stt is disabled in config".to_string()));
    }

    if !is_vosk_backend(config) {
        return Err(AppError::Voice(
            "STT backend is not set to Vosk. Re-run auto-configure to repair the local Vosk setup."
                .to_string(),
        ));
    }

    let model = config
        .stt_model_path
        .clone()
        .ok_or_else(|| AppError::Voice("stt_model_path is required for Vosk STT".to_string()))?;
    run_vosk(&model, input_path).await
}

fn is_vosk_backend(config: &VoiceConfig) -> bool {
    matches!(
        config.stt_binary_path.as_deref().map(str::trim),
        Some("vosk") | Some("vosk-python")
    ) || config
        .stt_model_path
        .as_deref()
        .map(PathBuf::from)
        .is_some_and(|p| p.is_dir())
}

async fn convert_to_wav(input_path: &PathBuf, wav_path: &PathBuf) -> AppResult<()> {
    let mut last_err: Option<String> = None;
    for bin in ["ffmpeg", "/usr/bin/ffmpeg", "/usr/local/bin/ffmpeg"] {
        let mut cmd = Command::new(bin);
        cmd.arg("-y")
            .arg("-i")
            .arg(input_path)
            .arg("-ac")
            .arg("1")
            .arg("-ar")
            .arg("16000")
            .arg(wav_path);
        match timeout(Duration::from_secs(20), cmd.output()).await {
            Ok(Ok(output)) if output.status.success() => return Ok(()),
            Ok(Ok(output)) => {
                last_err = Some(String::from_utf8_lossy(&output.stderr).trim().to_string());
            }
            Ok(Err(e)) => last_err = Some(e.to_string()),
            Err(_) => last_err = Some("ffmpeg timed out".to_string()),
        }
    }
    Err(AppError::Voice(format!(
        "failed converting audio to wav for Vosk: {}",
        last_err.unwrap_or_else(|| "ffmpeg unavailable".to_string())
    )))
}

fn detect_vosk_python() -> Option<String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = vec![
        manifest_dir.join("../.venv-vosk/bin/python").to_string_lossy().to_string(),
        "./.venv-vosk/bin/python".to_string(),
        "../.venv-vosk/bin/python".to_string(),
        "python3".to_string(),
    ];
    for candidate in candidates {
        if candidate == "python3" {
            return Some(candidate);
        }
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path.to_string_lossy().to_string());
        }
    }
    None
}

fn detect_vosk_script() -> Option<String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = vec![
        manifest_dir.join("../scripts/vosk_transcribe.py").to_string_lossy().to_string(),
        "./scripts/vosk_transcribe.py".to_string(),
        "../scripts/vosk_transcribe.py".to_string(),
    ];
    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path.to_string_lossy().to_string());
        }
    }
    None
}

async fn run_vosk(model_dir: &str, input_path: &PathBuf) -> AppResult<String> {
    let model_path = PathBuf::from(model_dir);
    if !model_path.is_dir() {
        return Err(AppError::Voice(format!(
            "vosk model directory not found: {}",
            model_path.display()
        )));
    }
    let wav_path = if input_path.extension().and_then(|e| e.to_str()) == Some("wav") {
        input_path.clone()
    } else {
        return Err(AppError::Voice("Vosk requires wav input".to_string()));
    };
    let python = detect_vosk_python().ok_or_else(|| {
        AppError::Voice("Vosk Python runtime not found. Expected ./.venv-vosk/bin/python or python3.".to_string())
    })?;
    let script = detect_vosk_script().ok_or_else(|| {
        AppError::Voice("Vosk transcription helper script not found.".to_string())
    })?;
    let mut cmd = Command::new(python);
    cmd.arg(script)
        .arg("--model-dir")
        .arg(model_path)
        .arg("--audio")
        .arg(&wav_path);
    let output = timeout(Duration::from_secs(45), cmd.output())
        .await
        .map_err(|_| AppError::Voice("Vosk STT timed out after 45s".to_string()))?
        .map_err(|e| AppError::Voice(format!("failed launching Vosk STT helper: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Voice(format!("Vosk STT failed: {}", stderr.trim())));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
