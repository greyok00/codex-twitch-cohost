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

    if is_vosk_backend(config) {
        let model = config
            .stt_model_path
            .clone()
            .ok_or_else(|| AppError::Voice("stt_model_path is required for Vosk STT".to_string()))?;
        return run_vosk(&model, input_path).await;
    }

    let mut binaries: Vec<String> = Vec::new();
    if let Some(bin) = config.stt_binary_path.clone() {
        let trimmed = bin.trim();
        if !trimmed.is_empty() {
            binaries.push(trimmed.to_string());
        }
    }
    for candidate in [
        "/usr/bin/whisper-cli",
        "/usr/local/bin/whisper-cli",
        "/usr/bin/whisper",
        "/usr/local/bin/whisper",
        "whisper-cli",
        "whisper",
    ] {
        if !binaries.iter().any(|b| b == candidate) {
            binaries.push(candidate.to_string());
        }
    }

    let model = config
        .stt_model_path
        .clone()
        .ok_or_else(|| AppError::Voice("stt_model_path is required for local STT".to_string()))?;

    let tmp = tempfile::tempdir().map_err(|e| AppError::Voice(format!("tempdir failed: {e}")))?;
    let output_prefix = tmp.path().join("transcript");

    let try_gpu = std::env::var("COHOST_STT_GPU")
        .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);

    let mut last_err: Option<AppError> = None;
    let mut completed = false;
    for binary in binaries {
        let first = run_whisper(&binary, &model, input_path, &output_prefix, try_gpu).await;
        match first {
            Ok(()) => {
                completed = true;
                break;
            }
            Err(e) => {
                if try_gpu {
                    match run_whisper(&binary, &model, input_path, &output_prefix, false).await {
                        Ok(()) => {
                            completed = true;
                            break;
                        }
                        Err(e2) => {
                            last_err = Some(e2);
                        }
                    }
                } else {
                    last_err = Some(e);
                }
            }
        }
    }
    if !completed {
        return Err(last_err.unwrap_or_else(|| {
            AppError::Voice(
                "local STT failed: no usable whisper executable found (tried whisper-cli/whisper)"
                    .to_string(),
            )
        }));
    }

    let text = read_transcript_output(&output_prefix, input_path, tmp.path())?;

    Ok(text.trim().to_string())
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
    let candidates = [
        "./.venv-vosk/bin/python",
        "../.venv-vosk/bin/python",
        "python3",
    ];
    for candidate in candidates {
        if candidate == "python3" {
            return Some(candidate.to_string());
        }
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path.to_string_lossy().to_string());
        }
    }
    None
}

fn detect_vosk_script() -> Option<String> {
    let candidates = [
        "./scripts/vosk_transcribe.py",
        "../scripts/vosk_transcribe.py",
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
    let output = timeout(Duration::from_secs(20), cmd.output())
        .await
        .map_err(|_| AppError::Voice("Vosk STT timed out after 20s".to_string()))?
        .map_err(|e| AppError::Voice(format!("failed launching Vosk STT helper: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Voice(format!("Vosk STT failed: {}", stderr.trim())));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn read_transcript_output(output_prefix: &PathBuf, input_path: &PathBuf, work_dir: &std::path::Path) -> AppResult<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.push(output_prefix.with_extension("txt"));
    candidates.push(work_dir.join("transcript.txt"));

    let input_name = input_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("voice_input.wav");
    candidates.push(work_dir.join(format!("{input_name}.txt")));

    let input_stem = input_path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("voice_input");
    candidates.push(work_dir.join(format!("{input_stem}.txt")));
    candidates.push(input_path.with_extension("txt"));
    candidates.push(PathBuf::from(format!("{}.txt", input_path.to_string_lossy())));

    if let Some(path) = candidates.iter().find(|p| p.exists()) {
        return fs::read_to_string(path)
            .map_err(|e| AppError::Voice(format!("failed reading transcript output {}: {e}", path.display())));
    }

    if let Ok(entries) = fs::read_dir(work_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("txt") {
                return fs::read_to_string(&p)
                    .map_err(|e| AppError::Voice(format!("failed reading transcript output {}: {e}", p.display())));
            }
        }
    }

    Ok(String::new())
}

async fn run_whisper(
    binary: &str,
    model: &str,
    input_path: &PathBuf,
    output_prefix: &PathBuf,
    use_gpu: bool,
) -> AppResult<()> {
    let txt_path = output_prefix.with_extension("txt");
    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 4))
        .unwrap_or(2)
        .to_string();
    // Precreate/clear so downstream always has a deterministic target path.
    let _ = fs::write(&txt_path, "");

    let mut cmd = Command::new(binary);
    cmd
        .arg("-m")
        .arg(model)
        .arg("-l")
        .arg("en")
        .arg("--prompt")
        .arg("Transcribe exact English speech. Keep profanity, slang, gamer words, usernames, and casual speech exactly as spoken. Do not censor swear words.")
        .arg("-nt")
        .arg("-np")
        .arg("-t")
        .arg(&thread_count)
        .arg("-bo")
        .arg("1")
        .arg("-bs")
        .arg("1")
        .arg("-nf")
        .arg("-mc")
        .arg("0")
        .arg("-f")
        .arg(input_path)
        .arg("-otxt")
        .arg("-of")
        .arg(output_prefix);
    // whisper.cpp CLI variants differ across builds. Avoid unsupported GPU flags
    // and only force CPU mode with -ng when we explicitly fall back.
    if !use_gpu {
        cmd.arg("-ng");
    }

    let output = cmd.output();
    let output = timeout(Duration::from_secs(20), output)
        .await
        .map_err(|_| AppError::Voice("local STT timed out after 20s".to_string()))?
        .map_err(|e| AppError::Voice(format!(
            "failed running STT binary {binary}: {e}. Install whisper-cli or bundle it under src-tauri/assets/whisper/"
        )))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Voice(
            format!(
                "local STT process failed ({}) {}",
                if use_gpu { "gpu-mode" } else { "cpu-mode" },
                stderr.trim()
            ),
        ));
    }

    // Some whisper builds return transcript on stdout/stderr instead of creating -otxt files.
    let current = fs::read_to_string(&txt_path).unwrap_or_default();
    if current.trim().is_empty() {
        let stdout_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr_text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let fallback = if !stdout_text.is_empty() {
            stdout_text
        } else if !stderr_text.is_empty() {
            stderr_text
        } else {
            String::new()
        };
        if !fallback.is_empty() {
            fs::write(&txt_path, fallback).map_err(|e| {
                AppError::Voice(format!("failed writing transcript fallback: {e}"))
            })?;
        }
    }
    Ok(())
}
