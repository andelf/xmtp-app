use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::Context;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::Response;
use tokio::task::JoinHandle;
use xmtp_daemon::addr_path;
use xmtp_ipc::{
    ActionResponse, ApiErrorBody, ConversationInfoResponse, DaemonEventData, DaemonEventEnvelope,
    EmojiRequest, GroupCreateRequest, GroupInfoResponse, GroupMembersResponse,
    GroupMembersUpdateRequest, HistoryResponse, RecipientMessageRequest, RecipientRequest,
    RenameGroupRequest, SendMessageRequest,
};

use crate::event::{ActionOutcome, AppEvent, Effect};

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(reqwest::Client::new)
}

#[derive(Debug)]
pub struct Runtime {
    data_dir: PathBuf,
    tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    app_events_handle: Option<JoinHandle<()>>,
    watch_handle: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn new(data_dir: PathBuf, tx: tokio::sync::mpsc::UnboundedSender<AppEvent>) -> Self {
        Self {
            data_dir,
            tx,
            app_events_handle: None,
            watch_handle: None,
        }
    }

    pub async fn ensure_ready(&self) -> anyhow::Result<()> {
        ensure_daemon(&self.data_dir).await
    }

    pub async fn apply_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::SubscribeAppEvents => self.subscribe_app_events(),
                Effect::SwitchConversation { conversation_id } => {
                    self.spawn_conversation_info(conversation_id.clone());
                    self.spawn_history(conversation_id.clone());
                    self.watch_conversation(conversation_id);
                }
                Effect::OpenDm { recipient } => self.spawn_open_dm(recipient),
                Effect::CreateGroup { name, members } => self.spawn_create_group(name, members),
                Effect::LoadGroupInfo { conversation_id } => self.spawn_group_info(conversation_id),
                Effect::LoadGroupMembers { conversation_id } => {
                    self.spawn_group_members(conversation_id)
                }
                Effect::AddGroupMembers {
                    conversation_id,
                    members,
                } => self.spawn_add_group_members(conversation_id, members),
                Effect::RemoveGroupMembers {
                    conversation_id,
                    members,
                } => self.spawn_remove_group_members(conversation_id, members),
                Effect::RenameGroup {
                    conversation_id,
                    name,
                } => self.spawn_rename_group(conversation_id, name),
                Effect::SendMessage {
                    conversation_id,
                    kind,
                    target,
                    text,
                } => self.spawn_send_message(conversation_id, kind, target, text),
                Effect::Reply { message_id, text } => self.spawn_reply(message_id, text),
                Effect::React { message_id, emoji } => self.spawn_react(message_id, emoji),
            }
        }
    }

    fn subscribe_app_events(&mut self) {
        if self.app_events_handle.is_some() {
            return;
        }
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        self.app_events_handle = Some(tokio::spawn(async move {
            if let Err(err) = watch_app_events(&data_dir, tx.clone()).await {
                let _ = tx.send(AppEvent::Error(err.to_string()));
            }
        }));
    }

    fn spawn_conversation_info(&self, conversation_id: String) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match conversation_info(&data_dir, &conversation_id).await {
                Ok(info) => {
                    let _ = tx.send(AppEvent::ConversationInfoLoaded(info));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_history(&self, conversation_id: String) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match load_history(&data_dir, &conversation_id).await {
                Ok(items) => {
                    let _ = tx.send(AppEvent::HistoryLoaded { conversation_id, items });
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn watch_conversation(&mut self, conversation_id: String) {
        if let Some(handle) = self.watch_handle.take() {
            handle.abort();
        }
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        self.watch_handle = Some(tokio::spawn(async move {
            if let Err(err) = watch_history(&data_dir, &conversation_id, tx.clone()).await {
                let _ = tx.send(AppEvent::Error(err.to_string()));
            }
        }));
    }

    fn spawn_open_dm(&self, recipient: String) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match open_dm(&data_dir, &recipient).await {
                Ok(result) => {
                    let _ = tx.send(AppEvent::ActionCompleted(ActionOutcome::OpenedDm(result)));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_create_group(&self, name: Option<String>, members: Vec<String>) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match create_group(&data_dir, name, members).await {
                Ok(result) => {
                    let _ = tx.send(AppEvent::ActionCompleted(ActionOutcome::CreatedGroup(result)));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_group_info(&self, conversation_id: String) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match group_info(&data_dir, &conversation_id).await {
                Ok(info) => {
                    let _ = tx.send(AppEvent::GroupInfoLoaded(info));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_group_members(&self, conversation_id: String) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match group_members(&data_dir, &conversation_id).await {
                Ok(response) => {
                    let _ = tx.send(AppEvent::GroupMembersLoaded(response.items));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_add_group_members(&self, conversation_id: String, members: Vec<String>) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match add_group_members(&data_dir, &conversation_id, members).await {
                Ok(result) => {
                    let _ = tx.send(AppEvent::ActionCompleted(ActionOutcome::GroupUpdated(
                        result.conversation_id,
                    )));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_remove_group_members(&self, conversation_id: String, members: Vec<String>) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match remove_group_members(&data_dir, &conversation_id, members).await {
                Ok(result) => {
                    let _ = tx.send(AppEvent::ActionCompleted(ActionOutcome::GroupUpdated(
                        result.conversation_id,
                    )));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_rename_group(&self, conversation_id: String, name: String) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match rename_group(&data_dir, &conversation_id, &name).await {
                Ok(result) => {
                    let _ = tx.send(AppEvent::ActionCompleted(ActionOutcome::GroupUpdated(
                        result.conversation_id,
                    )));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_send_message(
        &self,
        conversation_id: String,
        kind: String,
        target: Option<String>,
        text: String,
    ) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            let result: anyhow::Result<ActionResponse> = if kind == "group" {
                send_group(&data_dir, &conversation_id, &text).await
            } else if let Some(target) = target {
                send_dm(&data_dir, &target, &text).await.map(|response| ActionResponse {
                    conversation_id: response.conversation_id,
                    message_id: response.message_id,
                })
            } else {
                Err(anyhow::anyhow!("dm peer target unavailable"))
            };
            match result {
                Ok(_result) => {
                    let _ = tx.send(AppEvent::ActionCompleted(ActionOutcome::Sent));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_reply(&self, message_id: String, text: String) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match reply(&data_dir, &message_id, &text).await {
                Ok(_result) => {
                    let _ = tx.send(AppEvent::ActionCompleted(ActionOutcome::Sent));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_react(&self, message_id: String, emoji: String) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match react(&data_dir, &message_id, &emoji).await {
                Ok(_result) => {
                    let _ = tx.send(AppEvent::ActionCompleted(ActionOutcome::Reacted));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }
}

async fn ensure_daemon(data_dir: &PathBuf) -> anyhow::Result<()> {
    let addr = addr_path(data_dir);
    if addr.exists() {
        return Ok(());
    }
    let current_exe = std::env::current_exe().context("resolve current exe")?;
    let cli_path = current_exe.with_file_name("xmtp-cli");
    let data_dir = data_dir.clone();
    tokio::task::spawn_blocking(move || {
        let status = Command::new(cli_path)
            .arg("--data-dir")
            .arg(&data_dir)
            .arg("daemon")
            .arg("start")
            .status()
            .context("start daemon from xmtp-tui")?;
        if !status.success() {
            anyhow::bail!("daemon start failed");
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("join daemon start task")??;

    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if addr.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("daemon addr not ready")
}

async fn conversation_info(data_dir: &PathBuf, conversation_id: &str) -> anyhow::Result<ConversationInfoResponse> {
    http_get(data_dir, &format!("/v1/conversations/{conversation_id}")).await
}

async fn group_info(data_dir: &PathBuf, conversation_id: &str) -> anyhow::Result<GroupInfoResponse> {
    http_get(data_dir, &format!("/v1/groups/{conversation_id}")).await
}

async fn group_members(data_dir: &PathBuf, conversation_id: &str) -> anyhow::Result<GroupMembersResponse> {
    http_get(data_dir, &format!("/v1/groups/{conversation_id}/members")).await
}

async fn load_history(data_dir: &PathBuf, conversation_id: &str) -> anyhow::Result<Vec<xmtp_ipc::HistoryItem>> {
    let response: HistoryResponse =
        http_get(data_dir, &format!("/v1/conversations/{conversation_id}/history")).await?;
    Ok(response.items)
}

async fn watch_history(
    data_dir: &PathBuf,
    conversation_id: &str,
    tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
) -> anyhow::Result<()> {
    let mut retry_delay = Duration::from_millis(100);
    loop {
        if let Err(err) = ensure_daemon(data_dir).await {
            let _ = tx.send(AppEvent::Error(err.to_string()));
            tokio::time::sleep(retry_delay).await;
            retry_delay = next_retry_delay(retry_delay);
            continue;
        }
        let base_url = match daemon_base_url(data_dir) {
            Ok(base_url) => base_url,
            Err(err) => {
                let _ = tx.send(AppEvent::Error(err.to_string()));
                tokio::time::sleep(retry_delay).await;
                retry_delay = next_retry_delay(retry_delay);
                continue;
            }
        };
        let response = match http_client()
            .get(format!("{base_url}/v1/events/history/{conversation_id}"))
            .send()
            .await
            .context("open history sse stream")
        {
            Ok(response) => match ensure_success(response, "history sse status").await {
                Ok(response) => response,
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = next_retry_delay(retry_delay);
                    continue;
                }
            },
            Err(err) => {
                let _ = tx.send(AppEvent::Error(err.to_string()));
                tokio::time::sleep(retry_delay).await;
                retry_delay = next_retry_delay(retry_delay);
                continue;
            }
        };
        retry_delay = Duration::from_millis(100);
        let mut stream = response.bytes_stream().eventsource();
        while let Some(event) = stream.next().await {
            let event = match event.context("read history sse event") {
                Ok(event) => event,
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                    break;
                }
            };
            let envelope: DaemonEventEnvelope =
                match serde_json::from_str(&event.data).context("decode history event envelope") {
                    Ok(envelope) => envelope,
                    Err(err) => {
                        let _ = tx.send(AppEvent::Error(err.to_string()));
                        break;
                    }
                };
            match envelope.payload {
                DaemonEventData::HistoryItem { conversation_id, item } => {
                    let _ = tx.send(AppEvent::HistoryEvent {
                        conversation_id,
                        item,
                    });
                }
                DaemonEventData::DaemonError { message } => {
                    let _ = tx.send(AppEvent::Error(message));
                }
                DaemonEventData::Status(_)
                | DaemonEventData::ConversationList(_)
                | DaemonEventData::ConversationUpdated(_)
                | DaemonEventData::GroupMembersUpdated(_) => {}
            }
        }
        tokio::time::sleep(retry_delay).await;
        retry_delay = next_retry_delay(retry_delay);
    }
}

fn daemon_base_url(data_dir: &PathBuf) -> anyhow::Result<String> {
    let addr = std::fs::read_to_string(addr_path(data_dir)).context("read daemon addr file")?;
    Ok(format!("http://{}", addr.trim()))
}

async fn open_dm(data_dir: &PathBuf, recipient: &str) -> anyhow::Result<ActionResponse> {
    http_post(
        data_dir,
        "/v1/direct-message/open",
        &RecipientRequest {
            recipient: recipient.to_owned(),
        },
    )
    .await
}

async fn watch_app_events(
    data_dir: &PathBuf,
    tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
) -> anyhow::Result<()> {
    let mut retry_delay = Duration::from_millis(100);
    loop {
        if let Err(err) = ensure_daemon(data_dir).await {
            let _ = tx.send(AppEvent::Error(err.to_string()));
            tokio::time::sleep(retry_delay).await;
            retry_delay = next_retry_delay(retry_delay);
            continue;
        }
        let base_url = match daemon_base_url(data_dir) {
            Ok(base_url) => base_url,
            Err(err) => {
                let _ = tx.send(AppEvent::Error(err.to_string()));
                tokio::time::sleep(retry_delay).await;
                retry_delay = next_retry_delay(retry_delay);
                continue;
            }
        };
        let response = match http_client()
            .get(format!("{base_url}/v1/events"))
            .send()
            .await
            .context("open app event stream")
        {
            Ok(response) => match ensure_success(response, "app event stream status").await {
                Ok(response) => response,
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = next_retry_delay(retry_delay);
                    continue;
                }
            },
            Err(err) => {
                let _ = tx.send(AppEvent::Error(err.to_string()));
                tokio::time::sleep(retry_delay).await;
                retry_delay = next_retry_delay(retry_delay);
                continue;
            }
        };
        retry_delay = Duration::from_millis(100);
        let mut stream = response.bytes_stream().eventsource();
        while let Some(event) = stream.next().await {
            let event = match event.context("read app event") {
                Ok(event) => event,
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                    break;
                }
            };
            let envelope: DaemonEventEnvelope =
                match serde_json::from_str(&event.data).context("decode app event envelope") {
                    Ok(envelope) => envelope,
                    Err(err) => {
                        let _ = tx.send(AppEvent::Error(err.to_string()));
                        break;
                    }
                };
            match envelope.payload {
                DaemonEventData::Status(status) => {
                    let _ = tx.send(AppEvent::StatusLoaded(status));
                }
                DaemonEventData::ConversationList(response) => {
                    let _ = tx.send(AppEvent::ConversationsLoaded(response.items));
                }
                DaemonEventData::ConversationUpdated(update) => {
                    let _ = tx.send(AppEvent::ConversationUpdated(update));
                }
                DaemonEventData::GroupMembersUpdated(update) => {
                    let _ = tx.send(AppEvent::GroupMembersUpdated(update));
                }
                DaemonEventData::DaemonError { message } => {
                    let _ = tx.send(AppEvent::Error(message));
                }
                // Global /v1/events currently only carries app-level snapshots and errors.
                // History items come from the dedicated /v1/events/history/:id stream.
                DaemonEventData::HistoryItem { .. } => {}
            }
        }
        tokio::time::sleep(retry_delay).await;
        retry_delay = next_retry_delay(retry_delay);
    }
}

async fn send_dm(data_dir: &PathBuf, recipient: &str, message: &str) -> anyhow::Result<xmtp_ipc::SendDmResponse> {
    http_post(
        data_dir,
        "/v1/direct-message/send",
        &RecipientMessageRequest {
            recipient: recipient.to_owned(),
            message: message.to_owned(),
        },
    )
    .await
}

async fn send_group(data_dir: &PathBuf, conversation_id: &str, message: &str) -> anyhow::Result<ActionResponse> {
    http_post(
        data_dir,
        &format!("/v1/groups/{conversation_id}/send"),
        &SendMessageRequest {
            message: message.to_owned(),
        },
    )
    .await
}

async fn create_group(data_dir: &PathBuf, name: Option<String>, members: Vec<String>) -> anyhow::Result<ActionResponse> {
    http_post(data_dir, "/v1/groups", &GroupCreateRequest { name, members }).await
}

async fn add_group_members(
    data_dir: &PathBuf,
    conversation_id: &str,
    members: Vec<String>,
) -> anyhow::Result<ActionResponse> {
    http_post(
        data_dir,
        &format!("/v1/groups/{conversation_id}/members"),
        &GroupMembersUpdateRequest { members },
    )
    .await
}

async fn remove_group_members(
    data_dir: &PathBuf,
    conversation_id: &str,
    members: Vec<String>,
) -> anyhow::Result<ActionResponse> {
    http_delete(
        data_dir,
        &format!("/v1/groups/{conversation_id}/members"),
        &GroupMembersUpdateRequest { members },
    )
    .await
}

async fn rename_group(data_dir: &PathBuf, conversation_id: &str, name: &str) -> anyhow::Result<ActionResponse> {
    http_patch(
        data_dir,
        &format!("/v1/groups/{conversation_id}"),
        &RenameGroupRequest {
            name: name.to_owned(),
        },
    )
    .await
}

async fn reply(data_dir: &PathBuf, message_id: &str, message: &str) -> anyhow::Result<ActionResponse> {
    http_post(
        data_dir,
        &format!("/v1/messages/{message_id}/reply"),
        &SendMessageRequest {
            message: message.to_owned(),
        },
    )
    .await
}

async fn react(data_dir: &PathBuf, message_id: &str, emoji: &str) -> anyhow::Result<ActionResponse> {
    http_post(
        data_dir,
        &format!("/v1/messages/{message_id}/react"),
        &EmojiRequest {
            emoji: emoji.to_owned(),
        },
    )
    .await
}

async fn http_get<T>(data_dir: &PathBuf, path: &str) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    ensure_daemon(data_dir).await?;
    let base_url = daemon_base_url(data_dir)?;
    let response = http_client()
        .get(format!("{base_url}{path}"))
        .send()
        .await
        .context("send daemon http request")?;
    let response = ensure_success(response, "daemon http status").await?;
    decode_json(response, "decode daemon http response").await
}

async fn http_post<T, B>(data_dir: &PathBuf, path: &str, body: &B) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
    B: serde::Serialize + ?Sized,
{
    ensure_daemon(data_dir).await?;
    let base_url = daemon_base_url(data_dir)?;
    let response = http_client()
        .post(format!("{base_url}{path}"))
        .json(body)
        .send()
        .await
        .context("send daemon http request")?;
    let response = ensure_success(response, "daemon http status").await?;
    decode_json(response, "decode daemon http response").await
}

async fn http_patch<T, B>(data_dir: &PathBuf, path: &str, body: &B) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
    B: serde::Serialize + ?Sized,
{
    ensure_daemon(data_dir).await?;
    let base_url = daemon_base_url(data_dir)?;
    let response = http_client()
        .patch(format!("{base_url}{path}"))
        .json(body)
        .send()
        .await
        .context("send daemon http request")?;
    let response = ensure_success(response, "daemon http status").await?;
    decode_json(response, "decode daemon http response").await
}

async fn http_delete<T, B>(data_dir: &PathBuf, path: &str, body: &B) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
    B: serde::Serialize + ?Sized,
{
    ensure_daemon(data_dir).await?;
    let base_url = daemon_base_url(data_dir)?;
    let response = http_client()
        .delete(format!("{base_url}{path}"))
        .json(body)
        .send()
        .await
        .context("send daemon http request")?;
    let response = ensure_success(response, "daemon http status").await?;
    decode_json(response, "decode daemon http response").await
}

fn next_retry_delay(current: Duration) -> Duration {
    let next_ms = (current.as_millis() as u64).saturating_mul(2).min(5_000);
    Duration::from_millis(next_ms.max(100))
}

async fn decode_json<T>(response: Response, context_message: &str) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    response
        .json()
        .await
        .with_context(|| context_message.to_owned())
}

async fn ensure_success(response: Response, context_message: &str) -> anyhow::Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    if let Ok(parsed) = serde_json::from_str::<ApiErrorBody>(&body) {
        anyhow::bail!("{}: {}", parsed.error.code, parsed.error.message);
    }
    if body.trim().is_empty() {
        anyhow::bail!("{context_message}: {}", status);
    }
    anyhow::bail!("{context_message}: {}", body.trim());
}
