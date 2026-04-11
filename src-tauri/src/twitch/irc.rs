use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::{sync::{mpsc, oneshot}, task::JoinHandle, time::{sleep, Duration, Instant}};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info, warn};

use crate::{error::{AppError, AppResult}, state::{ChatMessage, EventMessage, PipelineInput}};

#[derive(Default)]
struct RuntimeHandles {
    writer_tx: Option<mpsc::Sender<String>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

#[derive(Clone, Default)]
pub struct TwitchIrcService {
    handles: Arc<Mutex<RuntimeHandles>>,
}

impl TwitchIrcService {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn connect(
        &self,
        app: AppHandle,
        token: String,
        nick: String,
        channel: String,
        queue_tx: mpsc::Sender<PipelineInput>,
    ) -> AppResult<()> {
        // Always replace the live IRC task so switching from bot-self channel
        // to streamer channel actually takes effect.
        let previous_task = {
            let mut handles = self.handles.lock();
            if let Some(stop) = handles.shutdown_tx.take() {
                let _ = stop.send(());
            }
            handles.writer_tx = None;
            handles.task.take()
        };
        if let Some(task) = previous_task {
            task.abort();
        }

        let (writer_tx, writer_rx) = mpsc::channel::<String>(256);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(run_irc_loop(
            app,
            token,
            nick,
            channel,
            queue_tx,
            writer_rx,
            shutdown_rx,
        ));

        let mut handles = self.handles.lock();
        handles.writer_tx = Some(writer_tx);
        handles.shutdown_tx = Some(shutdown_tx);
        handles.task = Some(task);
        Ok(())
    }

    pub async fn disconnect(&self) {
        let mut handles = self.handles.lock();
        if let Some(stop) = handles.shutdown_tx.take() {
            let _ = stop.send(());
        }
        handles.writer_tx = None;
        if let Some(task) = handles.task.take() {
            task.abort();
        }
    }

    pub async fn send_message(&self, content: String) -> AppResult<()> {
        let tx = self.handles.lock().writer_tx.clone();
        if let Some(tx) = tx {
            tx.send(content)
                .await
                .map_err(|e| AppError::Twitch(format!("failed queueing message: {e}")))
        } else {
            Err(AppError::Twitch("Twitch chat not connected".to_string()))
        }
    }

    pub fn is_connected(&self) -> bool {
        let handles = self.handles.lock();
        handles.writer_tx.is_some() && handles.task.is_some()
    }
}

async fn run_irc_loop(
    app: AppHandle,
    token: String,
    nick: String,
    channel: String,
    queue_tx: mpsc::Sender<PipelineInput>,
    mut writer_rx: mpsc::Receiver<String>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut retry = 0_u64;
    let channel_name = channel.to_lowercase();
    let nick_name = nick.to_lowercase();
    loop {
        if retry > 0 {
            let backoff = (retry * 2).min(20);
            sleep(Duration::from_secs(backoff)).await;
        }
        retry = retry.saturating_add(1);

        let connect = connect_async("wss://irc-ws.chat.twitch.tv:443").await;
        let (ws_stream, _) = match connect {
            Ok(v) => v,
            Err(err) => {
                warn!("irc connect failed: {}", err);
                continue;
            }
        };

        info!("connected to Twitch IRC websocket");
        let mut auth_logged = false;
        let mut join_logged = false;
        let join_attempt_started = Instant::now();
        let mut join_timeout_logged = false;
        let _ = app.emit("timeline_event", serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "kind": "irc",
            "content": format!("Connecting IRC as {} to #{}", nick, channel),
            "timestamp": chrono::Utc::now().to_rfc3339()
        }));
        let (mut writer, mut reader) = ws_stream.split();

        for line in [
            "CAP REQ :twitch.tv/tags twitch.tv/commands twitch.tv/membership".to_string(),
            format!("PASS oauth:{}", token),
            format!("NICK {}", nick),
            format!("JOIN #{}", channel),
        ] {
            if writer.send(Message::Text(line.into())).await.is_err() {
                break;
            }
        }

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    let _ = writer.send(Message::Close(None)).await;
                    info!("twitch irc shutdown requested");
                    return;
                }
                outbound = writer_rx.recv() => {
                    match outbound {
                        Some(content) => {
                            let escaped = sanitize_for_twitch(&content);
                            let payload = format!("PRIVMSG #{} :{}", channel, escaped);
                            if let Err(err) = writer.send(Message::Text(payload.into())).await {
                                warn!("failed writing chat message: {}", err);
                                break;
                            }
                        }
                        None => return,
                    }
                }
                inbound = reader.next() => {
                    match inbound {
                        Some(Ok(Message::Text(text))) => {
                            for line in text.lines() {
                                let lower = line.to_lowercase();
                                if line.starts_with("PING") {
                                    let pong = line.replacen("PING", "PONG", 1);
                                    let _ = writer.send(Message::Text(pong.into())).await;
                                    continue;
                                }
                                if lower.contains("login authentication failed")
                                    || lower.contains("invalid nick")
                                    || lower.contains("improperly formatted auth")
                                {
                                    let msg = format!("Twitch IRC auth failed for {}: {}", nick, line);
                                    let _ = app.emit("error_banner", msg.clone());
                                    let _ = app.emit("timeline_event", serde_json::json!({
                                        "id": uuid::Uuid::new_v4().to_string(),
                                        "kind": "irc_error",
                                        "content": msg,
                                        "timestamp": chrono::Utc::now().to_rfc3339()
                                    }));
                                    break;
                                }
                                if !auth_logged && lower.contains(&format!(" 001 {} ", nick_name)) {
                                    auth_logged = true;
                                    let _ = app.emit("timeline_event", serde_json::json!({
                                        "id": uuid::Uuid::new_v4().to_string(),
                                        "kind": "irc",
                                        "content": format!("IRC authenticated as {}", nick),
                                        "timestamp": chrono::Utc::now().to_rfc3339()
                                    }));
                                }

                                let is_join_line = lower.contains(&format!(":{}!", nick_name))
                                    && lower.contains(" join #")
                                    && lower.contains(&format!("#{}", channel_name));
                                let is_roomstate = lower.contains(" roomstate #")
                                    && lower.contains(&format!("#{}", channel_name));
                                let is_names_end = lower.contains(&format!(" 366 {} #{} ", nick_name, channel_name));
                                if !join_logged && (is_join_line || is_roomstate || is_names_end) {
                                    join_logged = true;
                                    let _ = app.emit("timeline_event", serde_json::json!({
                                        "id": uuid::Uuid::new_v4().to_string(),
                                        "kind": "irc",
                                        "content": format!("Joined chatroom #{} as {}", channel, nick),
                                        "timestamp": chrono::Utc::now().to_rfc3339()
                                    }));
                                    let status_line = build_join_status_report(&channel);
                                    let payload = format!("PRIVMSG #{} :{}", channel, sanitize_for_twitch(&status_line));
                                    match writer.send(Message::Text(payload.into())).await {
                                        Ok(()) => {
                                            let _ = app.emit("timeline_event", serde_json::json!({
                                                "id": uuid::Uuid::new_v4().to_string(),
                                                "kind": "irc",
                                                "content": "Posted automatic connection status report to chat",
                                                "timestamp": chrono::Utc::now().to_rfc3339()
                                            }));
                                        }
                                        Err(err) => {
                                            let msg = format!("Failed sending automatic connection status report: {}", err);
                                            let _ = app.emit("error_banner", msg.clone());
                                            let _ = app.emit("timeline_event", serde_json::json!({
                                                "id": uuid::Uuid::new_v4().to_string(),
                                                "kind": "irc_error",
                                                "content": msg,
                                                "timestamp": chrono::Utc::now().to_rfc3339()
                                            }));
                                        }
                                    }
                                }

                                if lower.contains(" notice #")
                                    && lower.contains(&format!("#{}", channel_name))
                                    && (lower.contains("msg_channel_suspended")
                                        || lower.contains("msg_channel_not_found")
                                        || lower.contains("msg_requires_verified_phone_number"))
                                {
                                    let msg = format!("Channel join rejected: {}", line);
                                    let _ = app.emit("error_banner", msg.clone());
                                    let _ = app.emit("timeline_event", serde_json::json!({
                                        "id": uuid::Uuid::new_v4().to_string(),
                                        "kind": "irc_error",
                                        "content": msg,
                                        "timestamp": chrono::Utc::now().to_rfc3339()
                                    }));
                                }
                                if lower.contains(" notice #")
                                    && lower.contains(&format!("#{}", channel_name))
                                    && (lower.contains("msg_ratelimit")
                                        || lower.contains("msg_duplicate")
                                        || lower.contains("msg_timedout")
                                        || lower.contains("msg_banned")
                                        || lower.contains("msg_requires_verified_phone_number")
                                        || lower.contains("msg_followersonly")
                                        || lower.contains("msg_emoteonly"))
                                {
                                    let msg = format!("Chat send rejected by Twitch: {}", line);
                                    let _ = app.emit("error_banner", msg.clone());
                                    let _ = app.emit("timeline_event", serde_json::json!({
                                        "id": uuid::Uuid::new_v4().to_string(),
                                        "kind": "irc_error",
                                        "content": msg,
                                        "timestamp": chrono::Utc::now().to_rfc3339()
                                    }));
                                }

                                if auth_logged
                                    && !join_logged
                                    && !join_timeout_logged
                                    && join_attempt_started.elapsed() > Duration::from_secs(12)
                                {
                                    join_timeout_logged = true;
                                    let _ = app.emit("timeline_event", serde_json::json!({
                                        "id": uuid::Uuid::new_v4().to_string(),
                                        "kind": "irc_error",
                                        "content": format!("No JOIN confirmation from Twitch for #{} yet. Verify target channel exists and bot can access chat.", channel),
                                        "timestamp": chrono::Utc::now().to_rfc3339()
                                    }));
                                }
                                if let Some(chat) = parse_privmsg_line(line) {
                                    let _ = app.emit("chat_message", &chat);
                                    let _ = queue_tx.send(PipelineInput::Chat(chat)).await;
                                }
                                if let Some(event) = parse_usernotice_line(line) {
                                    let _ = app.emit("timeline_event", &event);
                                    let _ = queue_tx.send(PipelineInput::Event(event)).await;
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            warn!("irc websocket closed by server");
                            break;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(err)) => {
                            warn!("irc read error: {}", err);
                            break;
                        }
                        None => break,
                    }
                }
            }
        }

        error!("irc loop restarting");
    }
}

fn parse_privmsg_line(line: &str) -> Option<ChatMessage> {
    if !line.contains(" PRIVMSG ") {
        return None;
    }

    let mut user = "unknown".to_string();
    if let Some(tags_and_prefix) = line.strip_prefix('@') {
        if let Some((tags, _rest)) = tags_and_prefix.split_once(' ') {
            if let Some(display_name) = tags
                .split(';')
                .find_map(|entry| entry.strip_prefix("display-name="))
                .filter(|v| !v.trim().is_empty())
            {
                user = display_name.to_string();
            }
        }
    }
    if user == "unknown" {
        user = line
            .split('!')
            .next()
            .map(|p| p.trim_start_matches(':').trim_start_matches('@').to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
    }

    let content = line
        .split(" PRIVMSG ")
        .nth(1)
        .and_then(|s| s.split(" :").nth(1))?
        .trim()
        .to_string();

    Some(ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        user,
        content,
        timestamp: chrono::Utc::now().to_rfc3339(),
        is_bot: false,
    })
}

fn parse_usernotice_line(line: &str) -> Option<EventMessage> {
    if !line.contains(" USERNOTICE ") {
        return None;
    }

    let kind = if line.contains("msg-id=sub") {
        "subscription"
    } else if line.contains("msg-id=subgift") {
        "gift_sub"
    } else if line.contains("msg-id=raid") {
        "raid"
    } else if line.contains("msg-id=rewardgift") {
        "channel_points"
    } else {
        "event"
    };

    let content = line
        .split(" :")
        .nth(1)
        .unwrap_or("Twitch event triggered")
        .to_string();

    Some(EventMessage {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        content,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

fn sanitize_for_twitch(content: &str) -> String {
    content
        .replace('\n', " ")
        .replace('\r', " ")
        .chars()
        .take(480)
        .collect::<String>()
}

fn build_join_status_report(channel: &str) -> String {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
    format!(
        "Status report: connected to #{} at {}. Joke: I bill by the hour, but this bot works pro-bono for good vibes.",
        channel, now
    )
}
