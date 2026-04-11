use base64::Engine;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::error::{AppError, AppResult};

pub async fn capture_wav_base64(duration_ms: u64) -> AppResult<String> {
    let safe_ms = duration_ms.clamp(800, 10_000);
    let seconds = ((safe_ms as f64) / 1000.0).ceil() as u64;

    let tmp = tempfile::tempdir().map_err(|e| AppError::Voice(format!("tempdir failed: {e}")))?;
    let wav_path = tmp.path().join("mic_chunk.wav");

    capture_to_wav(&wav_path.to_string_lossy(), seconds).await?;

    let bytes = tokio::fs::read(&wav_path)
        .await
        .map_err(|e| AppError::Voice(format!("failed reading mic capture wav: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::Voice("captured audio is empty".to_string()));
    }

    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

async fn capture_to_wav(path: &str, seconds: u64) -> AppResult<()> {
    #[cfg(target_os = "linux")]
    {
        if run_capture_cmd("arecord", &["-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-d", &seconds.to_string(), "-t", "wav", path]).await.is_ok() {
            return Ok(());
        }

        if run_capture_cmd("ffmpeg", &["-y", "-f", "pulse", "-i", "default", "-t", &seconds.to_string(), "-ac", "1", "-ar", "16000", path]).await.is_ok() {
            return Ok(());
        }

        if run_capture_cmd("ffmpeg", &["-y", "-f", "alsa", "-i", "default", "-t", &seconds.to_string(), "-ac", "1", "-ar", "16000", path]).await.is_ok() {
            return Ok(());
        }

        return Err(AppError::Voice(
            "No working mic capture backend found. Install alsa-utils (arecord) or ffmpeg.".to_string(),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        if run_capture_cmd("ffmpeg", &["-y", "-f", "avfoundation", "-i", ":0", "-t", &seconds.to_string(), "-ac", "1", "-ar", "16000", path]).await.is_ok() {
            return Ok(());
        }

        if run_capture_cmd("sox", &["-d", "-c", "1", "-r", "16000", path, "trim", "0.0", &seconds.to_string()]).await.is_ok() {
            return Ok(());
        }

        return Err(AppError::Voice(
            "No working mic capture backend found. Install ffmpeg or sox.".to_string(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        if run_capture_cmd(
            "ffmpeg",
            &["-y", "-f", "dshow", "-i", "audio=default", "-t", &seconds.to_string(), "-ac", "1", "-ar", "16000", path],
        )
        .await
        .is_ok()
        {
            return Ok(());
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
