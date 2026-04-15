use base64::Engine;
use serde::Serialize;
use tokio::process::{Child, ChildStdout, Command};
use tokio::time::{timeout, Duration};
use std::process::Stdio;

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

pub struct PcmCaptureStream {
    pub backend: String,
    pub child: Child,
    pub stdout: ChildStdout,
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
    let clean_filter = "highpass=f=80,lowpass=f=7600";

    #[cfg(target_os = "linux")]
    {
        if let Some(source) = detect_default_pulse_source().await {
            if run_capture_cmd("ffmpeg", &["-y", "-f", "pulse", "-i", &source, "-t", &seconds_precise, "-ac", "1", "-ar", "16000", "-af", clean_filter, path]).await.is_ok() {
                return Ok(format!("ffmpeg:pulse:{source}"));
            }
        }

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

pub async fn spawn_pcm_stream() -> AppResult<PcmCaptureStream> {
    let clean_filter = "highpass=f=80,lowpass=f=7600";

    #[cfg(target_os = "linux")]
    {
        if let Some(source) = detect_default_pulse_source().await {
            if let Ok(stream) = spawn_capture_process(
                "ffmpeg",
                &[
                    "-nostdin",
                    "-loglevel",
                    "error",
                    "-fflags",
                    "nobuffer",
                    "-flags",
                    "low_delay",
                    "-f",
                    "pulse",
                    "-i",
                    &source,
                    "-ac",
                    "1",
                    "-ar",
                    "16000",
                    "-af",
                    clean_filter,
                    "-f",
                    "s16le",
                    "-acodec",
                    "pcm_s16le",
                    "pipe:1",
                ],
            )
            .await
            {
                return Ok(PcmCaptureStream {
                    backend: format!("ffmpeg:pulse:{source}"),
                    ..stream
                });
            }
        }

        if let Ok(stream) = spawn_capture_process(
            "ffmpeg",
            &[
                "-nostdin",
                "-loglevel",
                "error",
                "-fflags",
                "nobuffer",
                "-flags",
                "low_delay",
                "-f",
                "pulse",
                "-i",
                "default",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-af",
                clean_filter,
                "-f",
                "s16le",
                "-acodec",
                "pcm_s16le",
                "pipe:1",
            ],
        )
        .await
        {
            return Ok(PcmCaptureStream {
                backend: "ffmpeg:pulse:default".to_string(),
                ..stream
            });
        }

        if let Ok(stream) = spawn_capture_process(
            "ffmpeg",
            &[
                "-nostdin",
                "-loglevel",
                "error",
                "-fflags",
                "nobuffer",
                "-flags",
                "low_delay",
                "-f",
                "alsa",
                "-i",
                "default",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-af",
                clean_filter,
                "-f",
                "s16le",
                "-acodec",
                "pcm_s16le",
                "pipe:1",
            ],
        )
        .await
        {
            return Ok(PcmCaptureStream {
                backend: "ffmpeg:alsa:default".to_string(),
                ..stream
            });
        }

        if let Ok(stream) = spawn_capture_process(
            "arecord",
            &["-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "raw", "-"],
        )
        .await
        {
            return Ok(PcmCaptureStream {
                backend: "arecord:default".to_string(),
                ..stream
            });
        }

        return Err(AppError::Voice(
            "No working streaming mic backend found. Install ffmpeg or alsa-utils.".to_string(),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(stream) = spawn_capture_process(
            "ffmpeg",
            &[
                "-nostdin",
                "-loglevel",
                "error",
                "-fflags",
                "nobuffer",
                "-flags",
                "low_delay",
                "-f",
                "avfoundation",
                "-i",
                ":0",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-af",
                clean_filter,
                "-f",
                "s16le",
                "-acodec",
                "pcm_s16le",
                "pipe:1",
            ],
        )
        .await
        {
            return Ok(PcmCaptureStream {
                backend: "ffmpeg:avfoundation".to_string(),
                ..stream
            });
        }

        return Err(AppError::Voice(
            "No working streaming mic backend found. Install ffmpeg.".to_string(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(stream) = spawn_capture_process(
            "ffmpeg",
            &[
                "-nostdin",
                "-loglevel",
                "error",
                "-fflags",
                "nobuffer",
                "-flags",
                "low_delay",
                "-f",
                "dshow",
                "-i",
                "audio=default",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-af",
                clean_filter,
                "-f",
                "s16le",
                "-acodec",
                "pcm_s16le",
                "pipe:1",
            ],
        )
        .await
        {
            return Ok(PcmCaptureStream {
                backend: "ffmpeg:dshow:default".to_string(),
                ..stream
            });
        }

        return Err(AppError::Voice(
            "No working streaming mic backend found. Install ffmpeg and ensure it is on PATH.".to_string(),
        ));
    }
}

#[cfg(target_os = "linux")]
async fn detect_default_pulse_source() -> Option<String> {
    let output = timeout(Duration::from_secs(3), Command::new("pactl").arg("get-default-source").output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let source = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if source.is_empty() {
        return None;
    }
    Some(source)
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

async fn spawn_capture_process(bin: &str, args: &[&str]) -> AppResult<PcmCaptureStream> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Voice(format!("failed launching {bin}: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Voice(format!("{bin} did not expose stdout")))?;
    Ok(PcmCaptureStream {
        backend: bin.to_string(),
        child,
        stdout,
    })
}
