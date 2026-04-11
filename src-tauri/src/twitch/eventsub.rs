use std::sync::Arc;

use futures_util::StreamExt;
use parking_lot::Mutex;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::{sync::oneshot, task::JoinHandle};
use tokio_tungstenite::connect_async;
use tracing::{error, info, warn};

use crate::{
    error::{AppError, AppResult},
    state::{EventMessage, PipelineInput},
};

#[derive(Default)]
struct EventSubHandles {
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

#[derive(Clone, Default)]
pub struct EventSubService {
    handles: Arc<Mutex<EventSubHandles>>,
}

#[derive(Clone)]
pub struct EventSubStartConfig {
    pub token: String,
    pub client_id: String,
    pub broadcaster_login: String,
    pub bot_login: String,
}

impl EventSubService {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(
        &self,
        app: AppHandle,
        cfg: EventSubStartConfig,
        queue_tx: tokio::sync::mpsc::Sender<PipelineInput>,
    ) -> AppResult<()> {
        if self.handles.lock().task.is_some() {
            return Ok(());
        }

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(run_eventsub_loop(app, cfg, queue_tx, shutdown_rx));

        let mut handles = self.handles.lock();
        handles.shutdown_tx = Some(shutdown_tx);
        handles.task = Some(task);
        Ok(())
    }

    pub async fn stop(&self) {
        let mut handles = self.handles.lock();
        if let Some(shutdown) = handles.shutdown_tx.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = handles.task.take() {
            task.abort();
        }
    }

    pub fn is_running(&self) -> bool {
        self.handles.lock().task.is_some()
    }
}

pub async fn smoke_test_streamer_api(
    client_id: &str,
    token: &str,
    broadcaster_login: &str,
) -> AppResult<String> {
    let client = reqwest::Client::new();
    let users_resp = client
        .get("https://api.twitch.tv/helix/users")
        .header("Client-Id", client_id)
        .bearer_auth(token)
        .query(&[("login", broadcaster_login)])
        .send()
        .await?;

    if !users_resp.status().is_success() {
        let status = users_resp.status();
        let body = users_resp.text().await.unwrap_or_else(|_| "<empty>".to_string());
        return Err(AppError::Twitch(format!(
            "streamer API check failed resolving broadcaster login '{broadcaster_login}' ({status}): {body}"
        )));
    }

    let users_json: Value = users_resp.json().await?;
    let broadcaster_id = users_json
        .pointer("/data/0/id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Twitch(format!(
                "streamer API check could not resolve broadcaster id for '{broadcaster_login}'"
            ))
        })?
        .to_string();

    let subs_resp = client
        .get("https://api.twitch.tv/helix/eventsub/subscriptions")
        .header("Client-Id", client_id)
        .bearer_auth(token)
        .send()
        .await?;

    if !subs_resp.status().is_success() {
        let status = subs_resp.status();
        let body = subs_resp.text().await.unwrap_or_else(|_| "<empty>".to_string());
        return Err(AppError::Twitch(format!(
            "streamer API check failed reading eventsub subscriptions ({status}): {body}"
        )));
    }

    let subs_json: Value = subs_resp.json().await?;
    let total = subs_json
        .get("total")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Ok(format!(
        "Streamer API check OK: broadcaster={broadcaster_login} id={broadcaster_id}, eventsub_subscriptions={total}"
    ))
}

async fn run_eventsub_loop(
    app: AppHandle,
    cfg: EventSubStartConfig,
    queue_tx: tokio::sync::mpsc::Sender<PipelineInput>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let ws = connect_async("wss://eventsub.wss.twitch.tv/ws").await;
    let (stream, _) = match ws {
        Ok(v) => v,
        Err(err) => {
            error!("eventsub connect failed: {}", err);
            return;
        }
    };

    let (broadcaster_id, bot_id) = match fetch_user_ids(&cfg).await {
        Ok(ids) => ids,
        Err(err) => {
            error!("eventsub user lookup failed: {}", err);
            return;
        }
    };

    let mut reader = stream;
    let mut subscribed = false;

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                info!("eventsub shutdown requested");
                return;
            }
            frame = reader.next() => {
                match frame {
                    Some(Ok(msg)) => {
                        if !msg.is_text() {
                            continue;
                        }
                        let text = match msg.into_text() {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let payload: Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(err) => {
                                warn!("eventsub payload parse failed: {}", err);
                                continue;
                            }
                        };

                        if let Some(kind) = payload.pointer("/metadata/message_type").and_then(|v| v.as_str()) {
                            match kind {
                                "session_welcome" => {
                                    if !subscribed {
                                        if let Some(session_id) = payload.pointer("/payload/session/id").and_then(|v| v.as_str()) {
                                            match subscribe_default_topics(&cfg, session_id, &broadcaster_id, &bot_id).await {
                                                Ok(()) => {
                                                    subscribed = true;
                                                    info!("eventsub subscriptions registered");
                                                }
                                                Err(err) => {
                                                    error!("eventsub subscription failed: {}", err);
                                                }
                                            }
                                        }
                                    }
                                }
                                "notification" => {
                                    if let Some(event) = normalize_notification(&payload) {
                                        let _ = app.emit("timeline_event", &event);
                                        let _ = queue_tx.send(PipelineInput::Event(event)).await;
                                    }
                                }
                                "session_keepalive" => {}
                                "session_reconnect" => {
                                    warn!("eventsub requested reconnect");
                                    return;
                                }
                                other => {
                                    warn!("unhandled eventsub message type: {}", other);
                                }
                            }
                        }
                    }
                    Some(Err(err)) => {
                        error!("eventsub read error: {}", err);
                        return;
                    }
                    None => return,
                }
            }
        }
    }
}

fn normalize_notification(payload: &Value) -> Option<EventMessage> {
    let sub_type = payload.pointer("/payload/subscription/type")?.as_str()?;
    let event = payload.pointer("/payload/event")?;

    let (kind, content) = match sub_type {
        "channel.follow" => {
            let user = event.get("user_name").and_then(|v| v.as_str()).unwrap_or("Someone");
            ("follow", format!("{user} just followed"))
        }
        "channel.subscribe" => {
            let user = event.get("user_name").and_then(|v| v.as_str()).unwrap_or("Someone");
            let tier = event.get("tier").and_then(|v| v.as_str()).unwrap_or("unknown tier");
            ("subscription", format!("{user} subscribed ({tier})"))
        }
        "channel.subscription.gift" => {
            let user = event.get("user_name").and_then(|v| v.as_str()).unwrap_or("Someone");
            let total = event.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
            ("gift_sub", format!("{user} gifted {total} subs"))
        }
        "channel.raid" => {
            let from = event.get("from_broadcaster_user_name").and_then(|v| v.as_str()).unwrap_or("Another channel");
            let viewers = event.get("viewers").and_then(|v| v.as_u64()).unwrap_or(0);
            ("raid", format!("Raid from {from} with {viewers} viewers"))
        }
        "channel.channel_points_custom_reward_redemption.add" => {
            let user = event.get("user_name").and_then(|v| v.as_str()).unwrap_or("A viewer");
            let reward = event
                .pointer("/reward/title")
                .and_then(|v| v.as_str())
                .unwrap_or("a reward");
            ("channel_points", format!("{user} redeemed {reward}"))
        }
        "stream.online" => ("stream_online", "Stream is now online".to_string()),
        "stream.offline" => ("stream_offline", "Stream is now offline".to_string()),
        _ => {
            let raw = serde_json::to_string(event).ok()?;
            ("event", format!("{sub_type}: {raw}"))
        }
    };

    Some(EventMessage {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        content,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn fetch_user_ids(cfg: &EventSubStartConfig) -> AppResult<(String, String)> {
    let client = reqwest::Client::new();
    let url = "https://api.twitch.tv/helix/users";

    let broadcaster = client
        .get(url)
        .header("Client-Id", &cfg.client_id)
        .bearer_auth(&cfg.token)
        .query(&[("login", cfg.broadcaster_login.as_str())])
        .send()
        .await?;

    let broadcaster_json: Value = broadcaster.json().await?;
    let broadcaster_id = broadcaster_json
        .pointer("/data/0/id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Twitch("failed resolving broadcaster ID for EventSub".to_string()))?
        .to_string();

    let bot = client
        .get(url)
        .header("Client-Id", &cfg.client_id)
        .bearer_auth(&cfg.token)
        .query(&[("login", cfg.bot_login.as_str())])
        .send()
        .await?;

    let bot_json: Value = bot.json().await?;
    let bot_id = bot_json
        .pointer("/data/0/id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Twitch("failed resolving bot ID for EventSub".to_string()))?
        .to_string();

    Ok((broadcaster_id, bot_id))
}

async fn subscribe_default_topics(
    cfg: &EventSubStartConfig,
    session_id: &str,
    broadcaster_id: &str,
    bot_id: &str,
) -> AppResult<()> {
    let client = reqwest::Client::new();
    let endpoint = "https://api.twitch.tv/helix/eventsub/subscriptions";

    let requests = vec![
        serde_json::json!({
            "type": "channel.follow",
            "version": "2",
            "condition": {
                "broadcaster_user_id": broadcaster_id,
                "moderator_user_id": bot_id
            },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }),
        serde_json::json!({
            "type": "channel.subscribe",
            "version": "1",
            "condition": { "broadcaster_user_id": broadcaster_id },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }),
        serde_json::json!({
            "type": "channel.subscription.gift",
            "version": "1",
            "condition": { "broadcaster_user_id": broadcaster_id },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }),
        serde_json::json!({
            "type": "channel.raid",
            "version": "1",
            "condition": { "to_broadcaster_user_id": broadcaster_id },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }),
        serde_json::json!({
            "type": "channel.channel_points_custom_reward_redemption.add",
            "version": "1",
            "condition": { "broadcaster_user_id": broadcaster_id },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }),
        serde_json::json!({
            "type": "stream.online",
            "version": "1",
            "condition": { "broadcaster_user_id": broadcaster_id },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }),
        serde_json::json!({
            "type": "stream.offline",
            "version": "1",
            "condition": { "broadcaster_user_id": broadcaster_id },
            "transport": {
                "method": "websocket",
                "session_id": session_id
            }
        }),
    ];

    for body in requests {
        let resp = client
            .post(endpoint)
            .header("Client-Id", &cfg.client_id)
            .bearer_auth(&cfg.token)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_else(|_| "<empty>".to_string());
            warn!("eventsub subscribe failed ({status}): {text}");
        }
    }

    Ok(())
}
