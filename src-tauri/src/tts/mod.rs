use base64::Engine;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

pub struct ResolvedTtsVoiceProfile {
    pub engine_voice: String,
    pub rate_pct: i32,
    pub pitch_hz: i32,
}

pub fn edge_tts_candidates() -> Vec<String> {
    let mut bins = Vec::new();
    if let Ok(bin) = std::env::var("COHOST_EDGE_TTS_BIN") {
        if !bin.trim().is_empty() {
            bins.push(bin);
        }
    }
    bins.push("../.venv-edge-tts/bin/edge-tts".to_string());
    bins.push("./.venv-edge-tts/bin/edge-tts".to_string());
    bins.push("edge-tts".to_string());
    bins
}

pub fn resolve_tts_voice_profile(selected_voice: &str, clean: &str) -> ResolvedTtsVoiceProfile {
    let chosen = selected_voice.trim();
    let mut engine_voice = chosen.to_string();
    if engine_voice.trim().is_empty() || engine_voice.eq_ignore_ascii_case("auto") {
        engine_voice = "en-US-JennyNeural".to_string();
    }

    let mut rate_pct: i32 = 8;
    let mut pitch_hz: i32 = 10;
    let lowered = clean.to_lowercase();
    let exclamations = clean.matches('!').count() as i32;
    let questions = clean.matches('?').count() as i32;
    let ellipses = clean.matches("...").count() as i32;

    let excited_hits = [
        "oh my god", "omg", "wow", "holy", "yes", "yesss", "let's go", "lets go",
        "baby", "daddy", "please", "come on", "right now", "good girl", "good boy",
        "love that", "need that", "so good", "perfect",
    ]
    .iter()
    .filter(|needle| lowered.contains(**needle))
    .count() as i32;

    let soft_hits = [
        "sleepy", "soft", "slow", "calm", "easy", "gentle", "quiet", "hush", "whisper",
        "sweet", "tender", "closer", "come here", "relax", "breathe", "mm", "mmm",
    ]
    .iter()
    .filter(|needle| lowered.contains(**needle))
    .count() as i32;

    rate_pct += (exclamations * 3).min(9);
    pitch_hz += (exclamations * 4).min(14);
    pitch_hz += (questions * 3).min(9);

    if excited_hits > 0 {
        rate_pct += (excited_hits * 2).min(8);
        pitch_hz += (excited_hits * 3).min(12);
    }

    if ellipses > 0 || soft_hits > 0 {
        rate_pct -= (ellipses * 4).min(8) + (soft_hits * 2).min(10);
        pitch_hz -= (soft_hits * 2).min(8);
    }

    if lowered.contains("serious") || lowered.contains("calm") {
        rate_pct -= 4;
        pitch_hz -= 1;
    }

    rate_pct = rate_pct.clamp(-18, 24);
    pitch_hz = pitch_hz.clamp(-8, 28);

    ResolvedTtsVoiceProfile {
        engine_voice,
        rate_pct,
        pitch_hz,
    }
}

pub async fn synthesize_tts_with_voice(clean: &str, voice: &str) -> Result<String, String> {
    let tmp = tempfile::tempdir().map_err(|e| format!("tempdir failed: {e}"))?;
    let audio_path = tmp.path().join("edge_tts.mp3");
    let mut last_err: Option<String> = None;
    let profile = resolve_tts_voice_profile(voice, clean);

    let rate_arg = format!("{:+}%", profile.rate_pct);
    let pitch_arg = format!("{:+}Hz", profile.pitch_hz);

    for bin in edge_tts_candidates() {
        let mut cmd = Command::new(&bin);
        cmd.arg("--voice")
            .arg(&profile.engine_voice)
            .arg("--rate")
            .arg(&rate_arg)
            .arg("--pitch")
            .arg(&pitch_arg)
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
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                last_err = Some(format!("edge-tts failed for {bin}: {stderr}"));
            }
            Ok(Err(e)) => {
                last_err = Some(format!("edge-tts launch failed for {bin}: {e}"));
            }
            Err(_) => {
                last_err = Some(format!("edge-tts timed out for {bin}"));
            }
        }
    }

    match synthesize_tts_local_fallback(clean).await {
        Ok(data_url) => Ok(data_url),
        Err(local_err) => Err(format!(
            "{}; local fallback failed: {}",
            last_err.unwrap_or_else(|| {
                "edge-tts not available; install it and set COHOST_EDGE_TTS_BIN if needed"
                    .to_string()
            }),
            local_err
        )),
    }
}

pub async fn synthesize_tts_local_fallback(clean: &str) -> Result<String, String> {
    let tmp = tempfile::tempdir().map_err(|e| format!("tempdir failed: {e}"))?;
    let wav_path = tmp.path().join("fallback_tts.wav");

    let mut cmd = Command::new("espeak-ng");
    cmd.arg("-v")
        .arg("en-us")
        .arg("-s")
        .arg("185")
        .arg("-w")
        .arg(&wav_path)
        .arg(clean);

    let run = timeout(Duration::from_secs(10), cmd.output()).await;
    match run {
        Ok(Ok(output)) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return Err(if stderr.is_empty() {
                    format!("espeak-ng exited with {}", output.status)
                } else {
                    format!("espeak-ng failed: {stderr}")
                });
            }
        }
        Ok(Err(e)) => return Err(format!("espeak-ng launch failed: {e}")),
        Err(_) => return Err("espeak-ng timed out".to_string()),
    }

    if !wav_path.exists() {
        return Err("espeak-ng produced no wav output".to_string());
    }

    let bytes = tokio::fs::read(&wav_path)
        .await
        .map_err(|e| format!("failed reading fallback synthesized audio: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:audio/wav;base64,{b64}"))
}
