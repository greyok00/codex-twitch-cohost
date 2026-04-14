use std::{
    collections::BTreeMap,
    env,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use base64::Engine;
use chrono::Utc;
use parking_lot::RwLock;
use serde::Serialize;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
#[cfg(windows)]
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::{
    config::AppConfig,
    error::{AppError, AppResult},
    llm::provider::LlmService,
    memory::store::{MemoryRecord, MemoryStore},
    personality::engine::PersonalityEngine,
    security::secret_store::SecretStore,
    state::{ChatMessage, EventMessage},
    tts,
    voice::stt,
};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[38;5;45m";
const GOLD: &str = "\x1b[38;5;220m";
const GREEN: &str = "\x1b[38;5;83m";
const RED: &str = "\x1b[38;5;203m";
const SLATE: &str = "\x1b[38;5;110m";

#[derive(Debug, Serialize, Deserialize)]
struct WorkerEnvelope {
    ok: bool,
    module: String,
    output: Option<String>,
    error: Option<String>,
    started_at: String,
    finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthLight {
    Red,
    Yellow,
    Green,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleHealth {
    pub light: HealthLight,
    pub message: String,
    pub restarts: u32,
    pub last_started_at: Option<String>,
    pub last_finished_at: Option<String>,
    pub last_duration_ms: Option<u128>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlRequest {
    pub command: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlResponse {
    pub ok: bool,
    pub result: Option<String>,
    pub error: Option<String>,
    pub status: Option<HeadlessStatus>,
    pub modules: BTreeMap<String, ModuleHealth>,
}

#[derive(Clone)]
struct ControlPlane {
    runtime: HeadlessRuntime,
    modules: Arc<RwLock<BTreeMap<String, ModuleHealth>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadlessStatus {
    pub config_path: String,
    pub model: String,
    pub voice_enabled: bool,
    pub stt_backend: String,
    pub tts_backend: String,
    pub memory_log: String,
}

#[derive(Clone)]
pub struct HeadlessRuntime {
    pub config: AppConfig,
    pub memory: MemoryStore,
    pub llm: LlmService,
    pub secrets: SecretStore,
}

impl HeadlessRuntime {
    pub fn new() -> AppResult<Self> {
        let mut config = AppConfig::load()?;
        let secrets = SecretStore::new();

        if config.providers.primary.api_key.is_none() {
            config.providers.primary.api_key = secrets.get_provider_key(&config.providers.primary.name)?;
        }
        for provider in &mut config.providers.fallbacks {
            if provider.api_key.is_none() {
                provider.api_key = secrets.get_provider_key(&provider.name)?;
            }
        }

        let memory_dir = resolve_data_dir().join("memory_db");
        std::fs::create_dir_all(&memory_dir)
            .map_err(|e| AppError::Storage(format!("failed creating {}: {e}", memory_dir.display())))?;
        let memory = MemoryStore::new(memory_dir)?;

        Ok(Self {
            config,
            memory,
            llm: LlmService::new(),
            secrets,
        })
    }

    pub fn status(&self) -> HeadlessStatus {
        let stt_backend = match self.config.voice.stt_binary_path.as_deref().map(str::trim) {
            Some("vosk") | Some("vosk-python") => "Local Vosk".to_string(),
            Some(bin) if !bin.is_empty() => format!("Local {bin}"),
            _ => "Browser/UI primary, local fallback headless".to_string(),
        };
        let tts_backend = if command_in_path("edge-tts")
            || PathBuf::from("./.venv-edge-tts/bin/edge-tts").exists()
            || PathBuf::from("../.venv-edge-tts/bin/edge-tts").exists()
        {
            "edge-tts + espeak-ng fallback".to_string()
        } else {
            "espeak-ng fallback".to_string()
        };
        HeadlessStatus {
            config_path: AppConfig::load_path_for_display(),
            model: self.config.providers.primary.model.clone(),
            voice_enabled: self.config.voice.enabled,
            stt_backend,
            tts_backend,
            memory_log: self.memory.log_path(),
        }
    }

    pub async fn prompt(&self, text: &str, caller: Option<&str>) -> AppResult<String> {
        let user = caller.unwrap_or("owner");
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(AppError::Internal("prompt text is required".to_string()));
        }

        self.memory.append_structured(
            "headless_user",
            Some(user),
            trimmed,
            Some("conversation".to_string()),
            20,
            vec!["headless".to_string(), "user".to_string()],
            None,
        )?;

        let recent_chat = self.build_recent_chat(18)?;
        let relevant_memory = self.build_memory_context(20)?;
        let system_prompt = PersonalityEngine::build_prompt(
            &self.config.personality,
            &recent_chat,
            &Vec::<EventMessage>::new(),
            &relevant_memory,
            false,
            self.config.voice.enabled,
        );
        let user_prompt = format!(
            "Latest line from {user}: {trimmed}\nRespond naturally, stay on the current subject, and do not ask filler questions unless necessary."
        );

        let reply = self
            .llm
            .generate(
                &self.config.providers.primary,
                &self.config.providers.fallbacks,
                &system_prompt,
                &user_prompt,
            )
            .await?;

        self.memory.append_structured(
            "headless_bot",
            Some(&self.config.personality.name),
            reply.trim(),
            Some("conversation".to_string()),
            16,
            vec!["headless".to_string(), "assistant".to_string()],
            None,
        )?;

        Ok(reply.trim().to_string())
    }

    pub async fn synthesize_tts(&self, text: &str, voice: Option<&str>) -> AppResult<String> {
        let selected = voice
            .filter(|v| !v.trim().is_empty())
            .or(self.config.voice.voice_name.as_deref())
            .unwrap_or("en-US-JennyNeural");
        tts::synthesize_tts_with_voice(text.trim(), selected)
            .await
            .map_err(AppError::Voice)
    }

    pub async fn transcribe_file(&self, path: &Path) -> AppResult<String> {
        stt::transcribe_file(&self.config.voice, &path.to_path_buf()).await
    }

    pub async fn voice_smoke(&self, phrase: &str) -> AppResult<String> {
        let data_url = self
            .synthesize_tts(phrase, self.config.voice.voice_name.as_deref())
            .await?;
        let (mime, bytes) = decode_data_url(&data_url)?;
        let tmp = tempfile::tempdir().map_err(|e| AppError::Voice(format!("tempdir failed: {e}")))?;
        let input_path = if mime == "audio/wav" {
            let wav = tmp.path().join("voice.wav");
            tokio::fs::write(&wav, bytes)
                .await
                .map_err(|e| AppError::Voice(format!("failed writing wav: {e}")))?;
            wav
        } else {
            let mp3 = tmp.path().join("voice.mp3");
            let wav = tmp.path().join("voice.wav");
            tokio::fs::write(&mp3, bytes)
                .await
                .map_err(|e| AppError::Voice(format!("failed writing mp3: {e}")))?;
            convert_to_wav(&mp3, &wav).await?;
            wav
        };
        let transcript = self.transcribe_file(&input_path).await?;
        Ok(transcript)
    }

    pub fn list_pinned(&self) -> AppResult<Vec<(String, String)>> {
        Ok(self
            .memory
            .list_pinned()?
            .into_iter()
            .map(|item| (item.label, item.content))
            .collect())
    }

    pub fn pin(&self, label: &str, content: &str) -> AppResult<()> {
        self.memory.upsert_pinned(label, content)?;
        Ok(())
    }

    fn build_recent_chat(&self, max: usize) -> AppResult<Vec<ChatMessage>> {
        let mut out = Vec::new();
        for rec in self.memory.tail(max * 4)? {
            let is_bot = matches!(rec.kind.as_str(), "headless_bot" | "assistant" | "bot_reply");
            let include = is_bot
                || matches!(
                    rec.kind.as_str(),
                    "headless_user" | "chat" | "local_chat" | "voice_frame" | "viewer_fact"
                );
            if !include {
                continue;
            }
            out.push(ChatMessage {
                id: rec.id,
                user: rec.user.unwrap_or_else(|| if is_bot {
                    self.config.personality.name.clone()
                } else {
                    "user".to_string()
                }),
                content: rec.content,
                timestamp: rec.timestamp,
                is_bot,
            });
            if out.len() >= max {
                break;
            }
        }
        out.reverse();
        Ok(out)
    }

    fn build_memory_context(&self, max: usize) -> AppResult<Vec<String>> {
        let mut lines = Vec::new();
        for pinned in self.memory.list_pinned()? {
            lines.push(format!("pinned {}: {}", pinned.label, pinned.content));
        }
        for rec in self.memory.tail(max * 4)? {
            if matches!(rec.kind.as_str(), "headless_user" | "headless_bot") {
                continue;
            }
            lines.push(render_memory_record(&rec));
            if lines.len() >= max {
                break;
            }
        }
        Ok(lines)
    }
}

impl ControlPlane {
    fn new() -> AppResult<Self> {
        let runtime = HeadlessRuntime::new()?;
        let mut modules = BTreeMap::new();
        modules.insert(
            "llm".to_string(),
            ModuleHealth {
                light: HealthLight::Green,
                message: format!("ready: {}", runtime.config.providers.primary.model),
                restarts: 0,
                last_started_at: None,
                last_finished_at: None,
                last_duration_ms: None,
            },
        );
        modules.insert(
            "tts".to_string(),
            ModuleHealth {
                light: HealthLight::Green,
                message: runtime.status().tts_backend,
                restarts: 0,
                last_started_at: None,
                last_finished_at: None,
                last_duration_ms: None,
            },
        );
        modules.insert(
            "stt".to_string(),
            ModuleHealth {
                light: HealthLight::Yellow,
                message: runtime.status().stt_backend,
                restarts: 0,
                last_started_at: None,
                last_finished_at: None,
                last_duration_ms: None,
            },
        );
        modules.insert(
            "twitch".to_string(),
            ModuleHealth {
                light: HealthLight::Yellow,
                message: "not attached to detached runtime yet".to_string(),
                restarts: 0,
                last_started_at: None,
                last_finished_at: None,
                last_duration_ms: None,
            },
        );
        Ok(Self {
            runtime,
            modules: Arc::new(RwLock::new(modules)),
        })
    }

    fn snapshot(&self) -> BTreeMap<String, ModuleHealth> {
        self.modules.read().clone()
    }

    async fn supervise_worker(&self, module: &str, args: &[String]) -> AppResult<String> {
        let started_at = Utc::now().to_rfc3339();
        {
            let mut modules = self.modules.write();
            let entry = modules.entry(module.to_string()).or_insert(ModuleHealth {
                light: HealthLight::Yellow,
                message: String::new(),
                restarts: 0,
                last_started_at: None,
                last_finished_at: None,
                last_duration_ms: None,
            });
            entry.light = HealthLight::Yellow;
            entry.message = "running".to_string();
            entry.last_started_at = Some(started_at.clone());
        }

        let timeout_secs = match module {
            "llm" => 45,
            "tts" => 20,
            "stt" => 25,
            "voice" => 45,
            _ => 30,
        };
        let exe = std::env::current_exe()
            .map_err(|e| AppError::Internal(format!("current_exe failed: {e}")))?;
        let started = std::time::Instant::now();
        let mut child = Command::new(&exe);
        child.arg("worker");
        for arg in args {
            child.arg(arg);
        }

        let output = timeout(Duration::from_secs(timeout_secs), child.output())
            .await
            .map_err(|_| AppError::Internal(format!("worker {module} timed out after {timeout_secs}s")))?;

        let finished_at = Utc::now().to_rfc3339();
        let duration_ms = started.elapsed().as_millis();
        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let parsed = serde_json::from_str::<WorkerEnvelope>(&stdout).ok();
                if output.status.success() {
                    let result = parsed.and_then(|p| p.output).unwrap_or(stdout.clone());
                    let mut modules = self.modules.write();
                    if let Some(entry) = modules.get_mut(module) {
                        entry.light = HealthLight::Green;
                        entry.message = "ok".to_string();
                        entry.last_finished_at = Some(finished_at);
                        entry.last_duration_ms = Some(duration_ms);
                    }
                    Ok(result)
                } else {
                    let err = parsed
                        .and_then(|p| p.error)
                        .unwrap_or_else(|| String::from_utf8_lossy(&output.stderr).trim().to_string());
                    let mut modules = self.modules.write();
                    if let Some(entry) = modules.get_mut(module) {
                        entry.light = HealthLight::Red;
                        entry.message = err.clone();
                        entry.last_finished_at = Some(finished_at);
                        entry.last_duration_ms = Some(duration_ms);
                        entry.restarts += 1;
                    }
                    Err(AppError::Internal(err))
                }
            }
            Err(err) => {
                let mut modules = self.modules.write();
                if let Some(entry) = modules.get_mut(module) {
                    entry.light = HealthLight::Red;
                    entry.message = err.to_string();
                    entry.last_finished_at = Some(finished_at);
                    entry.last_duration_ms = Some(duration_ms);
                    entry.restarts += 1;
                }
                Err(AppError::Internal(err.to_string()))
            }
        }
    }
}

pub async fn run_cli() -> AppResult<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    run_cli_args(args).await
}

pub async fn run_cli_args(args: Vec<String>) -> AppResult<()> {
    if args.is_empty() {
        let runtime = HeadlessRuntime::new()?;
        print_banner(&runtime.status());
        return run_repl(runtime).await;
    }

    match args[0].as_str() {
        "daemon" => run_daemon().await,
        "call" => run_call(args[1..].to_vec()).await,
        "shell" => {
            let runtime = HeadlessRuntime::new()?;
            print_banner(&runtime.status());
            run_repl(runtime).await
        }
        "status" => {
            let runtime = HeadlessRuntime::new()?;
            let status = runtime.status();
            print_banner(&status);
            Ok(())
        }
        "prompt" => {
            let runtime = HeadlessRuntime::new()?;
            let text = args[1..].join(" ");
            let reply = runtime.prompt(&text, Some("owner")).await?;
            print_panel("LLM", &reply, GREEN);
            Ok(())
        }
        "tts" => {
            let runtime = HeadlessRuntime::new()?;
            let text = args[1..].join(" ");
            let data = runtime.synthesize_tts(&text, None).await?;
            let (mime, bytes) = decode_data_url(&data)?;
            let ext = if mime == "audio/wav" { "wav" } else { "mp3" };
            let out = std::env::temp_dir().join(format!("cohostd-tts-{}.{}", Utc::now().timestamp_millis(), ext));
            tokio::fs::write(&out, bytes)
                .await
                .map_err(|e| AppError::Voice(format!("failed writing tts output: {e}")))?;
            print_panel("TTS", &format!("wrote {}", out.display()), GREEN);
            Ok(())
        }
        "stt-file" => {
            let runtime = HeadlessRuntime::new()?;
            let Some(path) = args.get(1) else {
                return Err(AppError::Internal("stt-file requires a path".to_string()));
            };
            let transcript = runtime.transcribe_file(Path::new(path)).await?;
            print_panel("STT", &transcript, GREEN);
            Ok(())
        }
        "voice-smoke" => {
            let runtime = HeadlessRuntime::new()?;
            let phrase = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                "hello this is a voice test please transcribe this sentence clearly".to_string()
            };
            let transcript = runtime.voice_smoke(&phrase).await?;
            print_panel("VOICE SMOKE", &format!("expected: {phrase}\nactual:   {transcript}"), GREEN);
            Ok(())
        }
        "worker" => run_worker(args[1..].to_vec()).await,
        "supervise" => run_supervisor(args[1..].to_vec()).await,
        other => Err(AppError::Internal(format!("unknown command: {other}"))),
    }
}

async fn run_worker(args: Vec<String>) -> AppResult<()> {
    let Some(module) = args.first().cloned() else {
        return Err(AppError::Internal("worker requires a module".to_string()));
    };
    let started_at = Utc::now().to_rfc3339();
    let runtime = HeadlessRuntime::new()?;
    let result = match module.as_str() {
        "llm" => {
            let text = args[1..].join(" ");
            runtime.prompt(&text, Some("owner")).await
        }
        "tts" => {
            let text = args[1..].join(" ");
            let data = runtime.synthesize_tts(&text, None).await?;
            Ok(format!("data-url-bytes={}", data.len()))
        }
        "stt-file" => {
            let Some(path) = args.get(1) else {
                return Err(AppError::Internal("worker stt-file requires a path".to_string()));
            };
            runtime.transcribe_file(Path::new(path)).await
        }
        "voice-smoke" => {
            let phrase = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                "hello this is a voice test please transcribe this sentence clearly".to_string()
            };
            runtime.voice_smoke(&phrase).await
        }
        other => Err(AppError::Internal(format!("unknown worker module: {other}"))),
    };

    let envelope = match result {
        Ok(output) => WorkerEnvelope {
            ok: true,
            module,
            output: Some(output),
            error: None,
            started_at,
            finished_at: Utc::now().to_rfc3339(),
        },
        Err(err) => WorkerEnvelope {
            ok: false,
            module,
            output: None,
            error: Some(err.to_string()),
            started_at,
            finished_at: Utc::now().to_rfc3339(),
        },
    };
    println!(
        "{}",
        serde_json::to_string(&envelope)
            .map_err(|e| AppError::Internal(format!("worker json encode failed: {e}")))?
    );
    if envelope.ok {
        Ok(())
    } else {
        Err(AppError::Internal(
            envelope.error.unwrap_or_else(|| "worker failed".to_string()),
        ))
    }
}

async fn run_supervisor(args: Vec<String>) -> AppResult<()> {
    let Some(module) = args.first().cloned() else {
        return Err(AppError::Internal("supervise requires a module".to_string()));
    };
    let child_args = args;
    let timeout_secs = match module.as_str() {
        "llm" => 45,
        "tts" => 20,
        "stt-file" => 25,
        "voice-smoke" => 45,
        _ => 30,
    };
    let max_restarts = 2;
    let exe = std::env::current_exe()
        .map_err(|e| AppError::Internal(format!("current_exe failed: {e}")))?;

    for attempt in 1..=max_restarts + 1 {
        print_panel(
            "SUPERVISOR",
            &format!("module: {module}\nattempt: {attempt}/{}", max_restarts + 1),
            SLATE,
        );
        let mut child = Command::new(&exe);
        child.arg("worker");
        for arg in &child_args {
            child.arg(arg);
        }
        let wait = timeout(Duration::from_secs(timeout_secs), child.output()).await;
        match wait {
            Ok(Ok(output)) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                print_panel("CHILD OK", &stdout, GREEN);
                return Ok(());
            }
            Ok(Ok(output)) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let detail = if !stdout.is_empty() { stdout } else { stderr };
                if attempt > max_restarts {
                    return Err(AppError::Internal(format!(
                        "supervised module {module} failed after {} attempts: {detail}",
                        max_restarts + 1
                    )));
                }
                print_panel("RESTART", &detail, RED);
            }
            Ok(Err(err)) => {
                if attempt > max_restarts {
                    return Err(AppError::Internal(format!(
                        "failed launching supervised module {module}: {err}"
                    )));
                }
                print_panel("RESTART", &err.to_string(), RED);
            }
            Err(_) => {
                if attempt > max_restarts {
                    return Err(AppError::Internal(format!(
                        "supervised module {module} timed out after {timeout_secs}s"
                    )));
                }
                print_panel(
                    "RESTART",
                    &format!("module {module} timed out after {timeout_secs}s; relaunching"),
                    RED,
                );
            }
        }
    }
    Err(AppError::Internal("supervisor exhausted retries".to_string()))
}

async fn run_daemon() -> AppResult<()> {
    let plane = ControlPlane::new()?;
    let addr = control_plane_addr();
    #[cfg(unix)]
    if std::fs::metadata(&addr).is_ok() {
        let _ = std::fs::remove_file(&addr);
    }
    let listener = bind_control_listener(&addr)
        .await
        .map_err(|e| AppError::Internal(format!("failed binding {addr}: {e}")))?;
    print_banner(&plane.runtime.status());
    print_panel("CONTROL", &format!("listening on {addr}"), CYAN);
    loop {
        let stream = accept_control_stream(&listener)
            .await
            .map_err(|e| AppError::Internal(format!("accept failed: {e}")))?;
        let plane = plane.clone();
        tokio::spawn(async move {
            let _ = handle_client(plane, stream).await;
        });
    }
}

async fn handle_client(plane: ControlPlane, stream: ControlStream) -> AppResult<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| AppError::Internal(format!("socket read failed: {e}")))? {
        if line.trim().is_empty() {
            continue;
        }
        let request: ControlRequest = serde_json::from_str(&line)
            .map_err(|e| AppError::Internal(format!("invalid control request: {e}")))?;
        let response = handle_request(&plane, request).await;
        let rendered = serde_json::to_string(&response)
            .map_err(|e| AppError::Internal(format!("control response encode failed: {e}")))?;
        writer
            .write_all(rendered.as_bytes())
            .await
            .map_err(|e| AppError::Internal(format!("socket write failed: {e}")))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|e| AppError::Internal(format!("socket write failed: {e}")))?;
    }
    Ok(())
}

async fn handle_request(plane: &ControlPlane, request: ControlRequest) -> ControlResponse {
    let result = match request.command.as_str() {
        "status" => Ok(Some("ok".to_string())),
        "prompt" => {
            let text = request.text.unwrap_or_default();
            plane
                .supervise_worker("llm", &[String::from("llm"), text])
                .await
                .map(Some)
        }
        "tts" => {
            let text = request.text.unwrap_or_default();
            plane
                .supervise_worker("tts", &[String::from("tts"), text])
                .await
                .map(Some)
        }
        "stt-file" => {
            let path = request.path.unwrap_or_default();
            plane
                .supervise_worker("stt", &[String::from("stt-file"), path])
                .await
                .map(Some)
        }
        "voice-smoke" => {
            let text = request
                .text
                .unwrap_or_else(|| "hello this is a voice test please transcribe this sentence clearly".to_string());
            plane
                .supervise_worker("voice", &[String::from("voice-smoke"), text])
                .await
                .map(Some)
        }
        "pin" => {
            let label = request.label.unwrap_or_default();
            let content = request.content.unwrap_or_default();
            match plane.runtime.pin(&label, &content) {
                Ok(()) => Ok(Some(format!("saved pinned memory '{label}'"))),
                Err(e) => Err(e),
            }
        }
        "pins" => plane
            .runtime
            .list_pinned()
            .map(|pins| {
                Some(
                    if pins.is_empty() {
                        "no pinned memory".to_string()
                    } else {
                        pins.into_iter()
                            .map(|(label, content)| format!("{label}: {content}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    },
                )
            }),
        _ => Err(AppError::Internal(format!("unknown command: {}", request.command))),
    };

    match result {
        Ok(result) => ControlResponse {
            ok: true,
            result,
            error: None,
            status: Some(plane.runtime.status()),
            modules: plane.snapshot(),
        },
        Err(err) => ControlResponse {
            ok: false,
            result: None,
            error: Some(err.to_string()),
            status: Some(plane.runtime.status()),
            modules: plane.snapshot(),
        },
    }
}

async fn run_call(args: Vec<String>) -> AppResult<()> {
    let Some(command) = args.first().cloned() else {
        return Err(AppError::Internal("call requires a command".to_string()));
    };
    let request = match command.as_str() {
        "status" => ControlRequest {
            command,
            text: None,
            path: None,
            label: None,
            content: None,
        },
        "prompt" | "tts" | "voice-smoke" => ControlRequest {
            command,
            text: Some(args[1..].join(" ")),
            path: None,
            label: None,
            content: None,
        },
        "stt-file" => ControlRequest {
            command,
            text: None,
            path: args.get(1).cloned(),
            label: None,
            content: None,
        },
        "pin" => {
            let joined = args[1..].join(" ");
            let Some((label, content)) = joined.split_once("::") else {
                return Err(AppError::Internal("use: call pin label::content".to_string()));
            };
            ControlRequest {
                command,
                text: None,
                path: None,
                label: Some(label.trim().to_string()),
                content: Some(content.trim().to_string()),
            }
        }
        "pins" => ControlRequest {
            command,
            text: None,
            path: None,
            label: None,
            content: None,
        },
        _ => return Err(AppError::Internal(format!("unknown call command: {command}"))),
    };

    let addr = control_plane_addr();
    let mut stream = connect_control_stream(&addr)
        .await
        .map_err(|e| AppError::Internal(format!("failed connecting to {addr}: {e}")))?;
    let payload = serde_json::to_string(&request)
        .map_err(|e| AppError::Internal(format!("failed encoding control request: {e}")))?;
    stream
        .write_all(payload.as_bytes())
        .await
        .map_err(|e| AppError::Internal(format!("socket write failed: {e}")))?;
    stream
        .write_all(b"\n")
        .await
        .map_err(|e| AppError::Internal(format!("socket write failed: {e}")))?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| AppError::Internal(format!("socket read failed: {e}")))?;
    let response: ControlResponse = serde_json::from_str(line.trim())
        .map_err(|e| AppError::Internal(format!("invalid control response: {e}")))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&response)
            .map_err(|e| AppError::Internal(format!("pretty json encode failed: {e}")))?
    );
    if response.ok {
        Ok(())
    } else {
        Err(AppError::Internal(
            response.error.unwrap_or_else(|| "backend call failed".to_string()),
        ))
    }
}

async fn run_repl(runtime: HeadlessRuntime) -> AppResult<()> {
    loop {
        print!("{}cohostd>{} ", CYAN, RESET);
        io::stdout()
            .flush()
            .map_err(|e| AppError::Internal(format!("stdout flush failed: {e}")))?;
        let mut line = String::new();
        let read = io::stdin()
            .read_line(&mut line)
            .map_err(|e| AppError::Internal(format!("stdin read failed: {e}")))?;
        if read == 0 {
            break;
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if matches!(input, "/exit" | "/quit") {
            break;
        }
        match handle_cli_command(&runtime, input).await {
            Ok(Some(msg)) => print_panel("RESULT", &msg, GREEN),
            Ok(None) => {}
            Err(err) => print_panel("ERROR", &err.to_string(), RED),
        }
    }
    Ok(())
}

async fn handle_cli_command(runtime: &HeadlessRuntime, input: &str) -> AppResult<Option<String>> {
    if input == "/help" {
        return Ok(Some(
            "/status | /voice-smoke [phrase] | /tts <text> | /stt-file <path> | /pin <label>::<content> | /pins | /exit"
                .to_string(),
        ));
    }
    if input == "/status" {
        let status = runtime.status();
        return Ok(Some(format!(
            "config: {}\nmodel: {}\nvoice enabled: {}\nstt: {}\ntts: {}\nmemory log: {}",
            status.config_path,
            status.model,
            status.voice_enabled,
            status.stt_backend,
            status.tts_backend,
            status.memory_log
        )));
    }
    if let Some(rest) = input.strip_prefix("/tts ") {
        let data = runtime.synthesize_tts(rest, None).await?;
        let (_, bytes) = decode_data_url(&data)?;
        let out = std::env::temp_dir().join(format!("cohostd-tts-{}.wav", Utc::now().timestamp_millis()));
        tokio::fs::write(&out, bytes)
            .await
            .map_err(|e| AppError::Voice(format!("failed writing tts output: {e}")))?;
        return Ok(Some(format!("wrote {}", out.display())));
    }
    if let Some(rest) = input.strip_prefix("/stt-file ") {
        let transcript = runtime.transcribe_file(Path::new(rest.trim())).await?;
        return Ok(Some(transcript));
    }
    if let Some(rest) = input.strip_prefix("/voice-smoke") {
        let phrase = rest.trim();
        let phrase = if phrase.is_empty() {
            "hello this is a voice test please transcribe this sentence clearly"
        } else {
            phrase
        };
        let transcript = runtime.voice_smoke(phrase).await?;
        return Ok(Some(format!("expected: {phrase}\nactual:   {transcript}")));
    }
    if input == "/pins" {
        let pins = runtime.list_pinned()?;
        let text = if pins.is_empty() {
            "no pinned memory".to_string()
        } else {
            pins.into_iter()
                .map(|(label, content)| format!("{label}: {content}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        return Ok(Some(text));
    }
    if let Some(rest) = input.strip_prefix("/pin ") {
        let Some((label, content)) = rest.split_once("::") else {
            return Err(AppError::Internal("use /pin label::content".to_string()));
        };
        runtime.pin(label.trim(), content.trim())?;
        return Ok(Some(format!("saved pinned memory '{}'", label.trim())));
    }

    let reply = runtime.prompt(input, Some("owner")).await?;
    Ok(Some(reply))
}

fn render_memory_record(rec: &MemoryRecord) -> String {
    let user = rec.user.clone().unwrap_or_else(|| "system".to_string());
    match rec.kind.as_str() {
        "voice_frame" => {
            if let Some(meta) = &rec.metadata {
                let heard = meta.get("heard").and_then(|v| v.as_str()).unwrap_or(&rec.content);
                let normalized = meta.get("normalized").and_then(|v| v.as_str()).unwrap_or("");
                let command = meta.get("command").and_then(|v| v.as_str()).unwrap_or("");
                if !normalized.is_empty() || !command.is_empty() {
                    return format!(
                        "voice_frame user={user} heard=\"{heard}\" normalized=\"{normalized}\" command={command}"
                    );
                }
            }
            format!("voice_frame user={user} heard=\"{}\"", rec.content)
        }
        _ => format!("{} {}: {}", rec.kind, user, rec.content),
    }
}

async fn convert_to_wav(input: &Path, wav: &Path) -> AppResult<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg(wav);
    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::Voice(format!("ffmpeg launch failed: {e}")))?;
    if !output.status.success() {
        return Err(AppError::Voice(format!(
            "ffmpeg conversion failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn decode_data_url(value: &str) -> AppResult<(String, Vec<u8>)> {
    let Some((prefix, payload)) = value.split_once(',') else {
        return Err(AppError::Voice("invalid data url".to_string()));
    };
    let mime = prefix
        .strip_prefix("data:")
        .and_then(|s| s.strip_suffix(";base64"))
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| AppError::Voice(format!("invalid base64 tts payload: {e}")))?;
    Ok((mime, bytes))
}

fn resolve_data_dir() -> PathBuf {
    let mut candidates = Vec::new();
    if let Some(explicit) = env::var_os("TWITCH_COHOST_DATA_DIR") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Some(xdg) = env::var_os("XDG_DATA_HOME") {
        candidates.push(PathBuf::from(xdg).join("twitch-cohost-bot"));
    }
    if let Some(home) = env::var_os("HOME") {
        candidates.push(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("twitch-cohost-bot"),
        );
    }
    candidates.push(PathBuf::from("./data"));
    candidates.push(std::env::temp_dir().join("twitch-cohost-bot"));

    for candidate in candidates {
        if ensure_dir_writable(&candidate) {
            return candidate;
        }
    }

    PathBuf::from("./data")
}

fn command_in_path(name: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|dir| dir.join(name).exists())
}

fn ensure_dir_writable(path: &Path) -> bool {
    if std::fs::create_dir_all(path).is_err() {
        return false;
    }
    let probe = path.join(format!(".probe-{}", std::process::id()));
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn print_banner(status: &HeadlessStatus) {
    println!(
        "{BOLD}{GOLD}COHOSTD // headless runtime{RESET}\n{DIM}native backend-first shell; UI is optional{RESET}"
    );
    print_panel(
        "STATUS",
        &format!(
            "model: {}\nstt: {}\ntts: {}\nmemory: {}",
            status.model, status.stt_backend, status.tts_backend, status.memory_log
        ),
        SLATE,
    );
    println!("{DIM}Type /help for commands. Plain text sends a direct prompt through the shared backend.{RESET}");
    println!("{DIM}Supervisor mode: cohostd supervise llm|tts|stt-file|voice-smoke ...{RESET}");
    println!("{DIM}Daemon mode: cohostd daemon  |  Client mode: cohostd call status{RESET}");
}

fn print_panel(title: &str, body: &str, color: &str) {
    println!("{color}{BOLD}╭─ {title}{RESET}");
    for line in body.lines() {
        println!("{color}│{RESET} {line}");
    }
    println!("{color}╰────────────────────────────────────────{RESET}");
}

#[cfg(unix)]
type ControlListener = UnixListener;
#[cfg(unix)]
type ControlStream = UnixStream;
#[cfg(windows)]
type ControlListener = TcpListener;
#[cfg(windows)]
type ControlStream = TcpStream;

#[cfg(unix)]
async fn bind_control_listener(addr: &str) -> std::io::Result<ControlListener> {
    UnixListener::bind(addr)
}

#[cfg(windows)]
async fn bind_control_listener(addr: &str) -> std::io::Result<ControlListener> {
    TcpListener::bind(addr).await
}

#[cfg(unix)]
async fn accept_control_stream(listener: &ControlListener) -> std::io::Result<ControlStream> {
    let (stream, _) = listener.accept().await?;
    Ok(stream)
}

#[cfg(windows)]
async fn accept_control_stream(listener: &ControlListener) -> std::io::Result<ControlStream> {
    let (stream, _) = listener.accept().await?;
    Ok(stream)
}

#[cfg(unix)]
async fn connect_control_stream(addr: &str) -> std::io::Result<ControlStream> {
    UnixStream::connect(addr).await
}

#[cfg(windows)]
async fn connect_control_stream(addr: &str) -> std::io::Result<ControlStream> {
    TcpStream::connect(addr).await
}

fn control_plane_addr() -> String {
    if let Ok(explicit) = env::var("COHOSTD_ADDR") {
        return explicit;
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
