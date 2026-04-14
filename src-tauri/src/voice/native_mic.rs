use base64::Engine;
use serde::Serialize;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureDebugInfo {
    pub backend: String,
    pub wav_path: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub duration_ms: u64,
}

pub async fn capture_wav_base64(duration_ms: u64) -> AppResult<String> {
    let safe_ms = duration_ms.clamp(700, 8_000);
    let tmp = tempfile::tempdir().map_err(|e| AppError::Voice(format!("tempdir failed: {e}")))?;
    let wav_path = tmp.path().join("mic_chunk.wav");
    let _backend = capture_to_wav(&wav_path.to_string_lossy(), safe_ms).await?;

    let bytes = tokio::fs::read(&wav_path)
        .await
        .map_err(|e| AppError::Voice(format!("failed reading mic capture wav: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::Voice("captured audio is empty".to_string()));
    }

    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub async fn capture_wav_base64_with_debug(duration_ms: u64) -> AppResult<(String, CaptureDebugInfo)> {
    let safe_ms = duration_ms.clamp(700, 8_000);

    let tmp = tempfile::tempdir().map_err(|e| AppError::Voice(format!("tempdir failed: {e}")))?;
    let wav_path = tmp.path().join("mic_chunk.wav");

    let backend = capture_to_wav(&wav_path.to_string_lossy(), safe_ms).await?;

    let bytes = tokio::fs::read(&wav_path)
        .await
        .map_err(|e| AppError::Voice(format!("failed reading mic capture wav: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::Voice("captured audio is empty".to_string()));
    }

    let debug_dir = std::env::temp_dir().join("cohost-mic-debug");
    let _ = tokio::fs::create_dir_all(&debug_dir).await;
    let debug_path = debug_dir.join(format!(
        "capture-{}.wav",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ")
    ));
    let _ = tokio::fs::write(&debug_path, &bytes).await;

    Ok((
        base64::engine::general_purpose::STANDARD.encode(bytes),
        CaptureDebugInfo {
            backend,
            wav_path: debug_path.to_string_lossy().to_string(),
            sample_rate_hz: 16_000,
            channels: 1,
            duration_ms: safe_ms,
        },
    ))
}

async fn capture_to_wav(path: &str, duration_ms: u64) -> AppResult<String> {
    let seconds_precise = format!("{:.2}", (duration_ms as f64) / 1000.0);
    #[cfg(target_os = "linux")]
    let seconds_int = ((duration_ms as f64) / 1000.0).ceil() as u64;
    let clean_filter = "highpass=f=70,lowpass=f=7800,speechnorm=e=6.5:r=0.0001:l=1,volume=1.25";

    #[cfg(target_os = "linux")]
    {
        if run_capture_cmd("ffmpeg", &["-y", "-f", "pulse", "-i", "default", "-t", &seconds_precise, "-ac", "1", "-ar", "16000", "-af", clean_filter, path]).await.is_ok() {
            return Ok("ffmpeg:pulse:default".to_string());
        }

        if run_capture_cmd("ffmpeg", &["-y", "-f", "alsa", "-i", "default", "-t", &seconds_precise, "-ac", "1", "-ar", "16000", "-af", clean_filter, path]).await.is_ok() {
            return Ok("ffmpeg:alsa:default".to_string());
        }

        if run_capture_cmd("arecord", &["-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-d", &seconds_int.to_string(), "-t", "wav", path]).await.is_ok() {
            return Ok("arecord:default".to_string());
        }

        return Err(AppError::Voice(
            "No working mic capture backend found. Install alsa-utils (arecord) or ffmpeg.".to_string(),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        if run_capture_cmd("ffmpeg", &["-y", "-f", "avfoundation", "-i", ":0", "-t", &seconds_precise, "-ac", "1", "-ar", "16000", "-af", clean_filter, path]).await.is_ok() {
            return Ok("ffmpeg:avfoundation".to_string());
        }

        if run_capture_cmd("sox", &["-d", "-c", "1", "-r", "16000", path, "trim", "0.0", &seconds_precise]).await.is_ok() {
            return Ok("sox:default".to_string());
        }

        return Err(AppError::Voice(
            "No working mic capture backend found. Install ffmpeg or sox.".to_string(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        if run_capture_cmd(
            "ffmpeg",
            &["-y", "-f", "dshow", "-i", "audio=default", "-t", &seconds_precise, "-ac", "1", "-ar", "16000", "-af", clean_filter, path],
        )
        .await
        .is_ok()
        {
            return Ok("ffmpeg:dshow:default".to_string());
        }

        return Err(AppError::Voice(
            "No working mic capture backend found. Install ffmpeg and ensure it is on PATH.".to_string(),
        ));
    }
}

async fn run_capture_cmd(bin: &str, args: &[&str]) -> AppResult<()> {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    let output = timeout(Duration::from_secs(20), cmd.output())
        .await
        .map_err(|_| AppError::Voice(format!("{bin} timed out")))?
        .map_err(|e| AppError::Voice(format!("failed launching {bin}: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(AppError::Voice(format!("{bin} failed: {}", stderr.trim())))
}
