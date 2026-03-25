use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Context;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use xmtp_daemon::socket_path;
use xmtp_ipc::{
    ActionResponse, ConversationInfoResponse, ConversationListResponse, DaemonRequest, DaemonResponse,
    DaemonResponseData, HistoryEventResponse, HistoryResponse, IpcEnvelope,
    StatusResponse,
};

use crate::event::{ActionOutcome, AppEvent, Effect};

#[derive(Debug)]
pub struct Runtime {
    data_dir: PathBuf,
    tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    watch_handle: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn new(data_dir: PathBuf, tx: tokio::sync::mpsc::UnboundedSender<AppEvent>) -> Self {
        Self {
            data_dir,
            tx,
            watch_handle: None,
        }
    }

    pub async fn ensure_ready(&self) -> anyhow::Result<()> {
        ensure_daemon(&self.data_dir).await
    }

    pub async fn apply_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::RefreshStatus => self.spawn_status(),
                Effect::RefreshConversations => self.spawn_conversations(),
                Effect::SwitchConversation { conversation_id } => {
                    self.spawn_conversation_info(conversation_id.clone());
                    self.spawn_history(conversation_id.clone());
                    self.watch_conversation(conversation_id);
                }
                Effect::OpenDm { recipient } => self.spawn_open_dm(recipient),
                Effect::CreateGroup { name, members } => self.spawn_create_group(name, members),
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

    fn spawn_status(&self) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match get_status(&data_dir).await {
                Ok(status) => {
                    let _ = tx.send(AppEvent::StatusLoaded(status));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
    }

    fn spawn_conversations(&self) {
        let tx = self.tx.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            match list_conversations(&data_dir).await {
                Ok(items) => {
                    let _ = tx.send(AppEvent::ConversationsLoaded(items));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(err.to_string()));
                }
            }
        });
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
    let socket = socket_path(data_dir);
    if socket.exists() {
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
        if socket.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("daemon socket not ready")
}

async fn send_request(data_dir: &PathBuf, request: DaemonRequest) -> anyhow::Result<DaemonResponse> {
    let socket = socket_path(data_dir);
    let mut stream = UnixStream::connect(&socket)
        .await
        .with_context(|| format!("connect daemon socket at {}", socket.display()))?;
    let envelope = IpcEnvelope {
        version: 1,
        request_id: "tui-req".to_owned(),
        payload: request,
    };
    let json = serde_json::to_string(&envelope).context("encode daemon request")?;
    stream.write_all(json.as_bytes()).await.context("write request")?;
    stream.write_all(b"\n").await.context("write newline")?;
    stream.flush().await.context("flush request")?;

    let mut reader = tokio::io::BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await.context("read daemon response")?;
    let envelope: IpcEnvelope<DaemonResponse> =
        serde_json::from_str(&line).context("decode daemon response")?;
    if envelope.payload.ok {
        Ok(envelope.payload)
    } else {
        anyhow::bail!(
            "{}",
            envelope
                .payload
                .error
                .unwrap_or_else(|| "daemon request failed".to_owned())
        )
    }
}

async fn get_status(data_dir: &PathBuf) -> anyhow::Result<StatusResponse> {
    match send_request(data_dir, DaemonRequest::GetStatus).await?.result {
        Some(DaemonResponseData::Status(status)) => Ok(status),
        _ => anyhow::bail!("unexpected daemon status response"),
    }
}

async fn list_conversations(data_dir: &PathBuf) -> anyhow::Result<Vec<xmtp_ipc::ConversationItem>> {
    match send_request(data_dir, DaemonRequest::ListConversations { kind: None })
        .await?
        .result
    {
        Some(DaemonResponseData::ConversationList(ConversationListResponse { items })) => Ok(items),
        _ => anyhow::bail!("unexpected conversation list response"),
    }
}

async fn conversation_info(data_dir: &PathBuf, conversation_id: &str) -> anyhow::Result<ConversationInfoResponse> {
    match send_request(
        data_dir,
        DaemonRequest::ConversationInfo {
            conversation_id: conversation_id.to_owned(),
        },
    )
    .await?
    .result
    {
        Some(DaemonResponseData::ConversationInfo(info)) => Ok(info),
        _ => anyhow::bail!("unexpected conversation info response"),
    }
}

async fn load_history(data_dir: &PathBuf, conversation_id: &str) -> anyhow::Result<Vec<xmtp_ipc::HistoryItem>> {
    match send_request(
        data_dir,
        DaemonRequest::History {
            conversation_id: conversation_id.to_owned(),
        },
    )
    .await?
    .result
    {
        Some(DaemonResponseData::History(HistoryResponse { items })) => Ok(items),
        _ => anyhow::bail!("unexpected history response"),
    }
}

async fn watch_history(
    data_dir: &PathBuf,
    conversation_id: &str,
    tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
) -> anyhow::Result<()> {
    ensure_daemon(data_dir).await?;
    let socket = socket_path(data_dir);
    let mut stream = UnixStream::connect(&socket)
        .await
        .with_context(|| format!("connect daemon socket at {}", socket.display()))?;
    let envelope = IpcEnvelope {
        version: 1,
        request_id: "tui-watch".to_owned(),
        payload: DaemonRequest::WatchHistory {
            conversation_id: conversation_id.to_owned(),
        },
    };
    let json = serde_json::to_string(&envelope).context("encode watch request")?;
    stream.write_all(json.as_bytes()).await.context("write watch request")?;
    stream.write_all(b"\n").await.context("write watch newline")?;
    stream.flush().await.context("flush watch request")?;

    let mut reader = tokio::io::BufReader::new(stream);
    let mut ack = String::new();
    reader.read_line(&mut ack).await.context("read watch ack")?;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).await.context("read watch line")?;
        if bytes == 0 {
            break;
        }
        let envelope: IpcEnvelope<DaemonResponse> =
            serde_json::from_str(&line).context("decode watch event")?;
        if !envelope.payload.ok {
            anyhow::bail!(
                "{}",
                envelope
                    .payload
                    .error
                    .unwrap_or_else(|| "watch request failed".to_owned())
            );
        }
        if let Some(DaemonResponseData::HistoryEvent(HistoryEventResponse { item })) =
            envelope.payload.result
        {
            let _ = tx.send(AppEvent::HistoryEvent {
                conversation_id: conversation_id.to_owned(),
                item,
            });
        }
    }
    Ok(())
}

async fn open_dm(data_dir: &PathBuf, recipient: &str) -> anyhow::Result<ActionResponse> {
    match send_request(
        data_dir,
        DaemonRequest::OpenDm {
            recipient: recipient.to_owned(),
        },
    )
    .await?
    .result
    {
        Some(DaemonResponseData::OpenDm(result)) => Ok(result),
        _ => anyhow::bail!("unexpected open dm response"),
    }
}

async fn send_dm(data_dir: &PathBuf, recipient: &str, message: &str) -> anyhow::Result<xmtp_ipc::SendDmResponse> {
    match send_request(
        data_dir,
        DaemonRequest::SendDm {
            recipient: recipient.to_owned(),
            message: message.to_owned(),
        },
    )
    .await?
    .result
    {
        Some(DaemonResponseData::SendDm(response)) => Ok(response),
        _ => anyhow::bail!("unexpected send dm response"),
    }
}

async fn send_group(data_dir: &PathBuf, conversation_id: &str, message: &str) -> anyhow::Result<ActionResponse> {
    match send_request(
        data_dir,
        DaemonRequest::SendGroup {
            conversation_id: conversation_id.to_owned(),
            message: message.to_owned(),
        },
    )
    .await?
    .result
    {
        Some(DaemonResponseData::SendGroup(response)) => Ok(response),
        _ => anyhow::bail!("unexpected send group response"),
    }
}

async fn create_group(data_dir: &PathBuf, name: Option<String>, members: Vec<String>) -> anyhow::Result<ActionResponse> {
    match send_request(data_dir, DaemonRequest::CreateGroup { name, members })
        .await?
        .result
    {
        Some(DaemonResponseData::CreateGroup(response)) => Ok(response),
        _ => anyhow::bail!("unexpected create group response"),
    }
}

async fn reply(data_dir: &PathBuf, message_id: &str, message: &str) -> anyhow::Result<ActionResponse> {
    match send_request(
        data_dir,
        DaemonRequest::Reply {
            message_id: message_id.to_owned(),
            message: message.to_owned(),
        },
    )
    .await?
    .result
    {
        Some(DaemonResponseData::Reply(response)) => Ok(response),
        _ => anyhow::bail!("unexpected reply response"),
    }
}

async fn react(data_dir: &PathBuf, message_id: &str, emoji: &str) -> anyhow::Result<ActionResponse> {
    match send_request(
        data_dir,
        DaemonRequest::React {
            message_id: message_id.to_owned(),
            emoji: emoji.to_owned(),
        },
    )
    .await?
    .result
    {
        Some(DaemonResponseData::React(response)) => Ok(response),
        _ => anyhow::bail!("unexpected react response"),
    }
}
