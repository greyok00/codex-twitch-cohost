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

    transcribe_file(config, &input_path).await
}

pub async fn transcribe_file(config: &VoiceConfig, input_path: &PathBuf) -> AppResult<String> {
    if !config.stt_enabled {
        return Err(AppError::Voice("stt is disabled in config".to_string()));
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
        .map(|v| v != "0")
        .unwrap_or(true);

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
    // Precreate/clear so downstream always has a deterministic target path.
    let _ = fs::write(&txt_path, "");

    let mut cmd = Command::new(binary);
    cmd
        .arg("-m")
        .arg(model)
        .arg("-l")
        .arg("en")
        .arg("-nt")
        .arg("-np")
        .arg("-t")
        .arg("2")
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
