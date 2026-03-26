use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use agent_client_protocol::{self as acp, Agent as _};
use anyhow::{Context, anyhow};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use tokio::process::Command;
use tokio::task::LocalSet;
use tokio::time::{Duration, sleep};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use xmtp_ipc::{
    ApiErrorBody, DaemonEventData, DaemonEventEnvelope, HistoryItem, SendMessageRequest,
    StatusResponse,
};

use crate::{daemon_base_url, daemon_send_conversation, http_client, http_get, wait_for_daemon_ready};

pub async fn run_acp(
    data_dir: PathBuf,
    conversation_id: String,
    command: Vec<String>,
) -> anyhow::Result<()> {
    let local = LocalSet::new();
    local
        .run_until(run_acp_inner(data_dir, conversation_id, command))
        .await
}

async fn run_acp_inner(
    data_dir: PathBuf,
    conversation_id: String,
    command: Vec<String>,
) -> anyhow::Result<()> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow!("ACP command is required"))?;

    wait_for_daemon_ready(&data_dir, 4_000).await?;
    let status: StatusResponse = http_get(&data_dir, "/v1/status")
        .await
        .context("load daemon status")?;
    ensure_acp_send_endpoint(&data_dir)
        .await
        .context("verify ACP daemon endpoints")?;
    let self_inbox_id = status.inbox_id;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawn ACP subprocess: {}", program))?;

    let child_stdin = child.stdin.take().context("take ACP subprocess stdin")?;
    let child_stdout = child.stdout.take().context("take ACP subprocess stdout")?;

    let chunks = Arc::new(Mutex::new(HashMap::<String, Vec<String>>::new()));
    let client = BridgeClient {
        chunks: Arc::clone(&chunks),
    };
    let (conn, io_task) = acp::ClientSideConnection::new(
        client,
        child_stdin.compat_write(),
        child_stdout.compat(),
        |fut| {
            tokio::task::spawn_local(fut);
        },
    );

    let io_handle = tokio::task::spawn_local(async move {
        if let Err(err) = io_task.await {
            eprintln!("ACP protocol task failed: {err:#}");
        }
    });

    conn.initialize(
        acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(
            acp::Implementation::new("xmtp-cli-acp", env!("CARGO_PKG_VERSION"))
                .title("XMTP ACP Bridge"),
        ),
    )
    .await
    .context("ACP initialize")?;

    let cwd = std::env::current_dir().context("read current working directory")?;
    let session_id = ensure_acp_session(&data_dir, &conversation_id, &conn, &cwd).await?;
    eprintln!("ACP session ready: {}", session_id_str(&session_id));

    let bridge_result =
        bridge_history_to_acp(&data_dir, &conversation_id, self_inbox_id.as_deref(), &conn, &session_id, &chunks).await;

    if let Err(err) = &bridge_result {
        eprintln!("ACP bridge error: {err:#}");
    }

    let _ = child.start_kill();
    let _ = child.wait().await;
    io_handle.abort();
    bridge_result
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct PersistedAcpSessions {
    sessions: HashMap<String, String>,
}

fn acp_sessions_path(data_dir: &Path) -> PathBuf {
    data_dir.join("acp_sessions.json")
}

fn load_acp_sessions(data_dir: &Path) -> anyhow::Result<PersistedAcpSessions> {
    let path = acp_sessions_path(data_dir);
    if !path.exists() {
        return Ok(PersistedAcpSessions::default());
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn save_acp_sessions(data_dir: &Path, sessions: &PersistedAcpSessions) -> anyhow::Result<()> {
    let path = acp_sessions_path(data_dir);
    let payload = serde_json::to_vec_pretty(sessions).context("serialize ACP sessions")?;
    fs::write(&path, payload).with_context(|| format!("write {}", path.display()))
}

fn persisted_session_id(data_dir: &Path, conversation_id: &str) -> anyhow::Result<Option<String>> {
    Ok(load_acp_sessions(data_dir)?.sessions.get(conversation_id).cloned())
}

fn store_session_id(
    data_dir: &Path,
    conversation_id: &str,
    session_id: &acp::SessionId,
) -> anyhow::Result<()> {
    let mut sessions = load_acp_sessions(data_dir)?;
    sessions
        .sessions
        .insert(conversation_id.to_owned(), session_id_owned(session_id));
    save_acp_sessions(data_dir, &sessions)
}

async fn ensure_acp_session(
    data_dir: &Path,
    conversation_id: &str,
    conn: &acp::ClientSideConnection,
    cwd: &Path,
) -> anyhow::Result<acp::SessionId> {
    if let Some(saved_session_id) = persisted_session_id(data_dir, conversation_id)? {
        let session_id = acp::SessionId::new(saved_session_id);
        match conn
            .load_session(acp::LoadSessionRequest::new(session_id.clone(), cwd.to_path_buf()))
            .await
        {
            Ok(_) => {
                eprintln!("ACP session restored: {}", session_id_str(&session_id));
                store_session_id(data_dir, conversation_id, &session_id)?;
                return Ok(session_id);
            }
            Err(err) => {
                eprintln!(
                    "ACP session restore failed for conversation {}: {err:#}",
                    conversation_id
                );
            }
        }
    }

    let session = conn
        .new_session(acp::NewSessionRequest::new(cwd.to_path_buf()))
        .await
        .context("ACP new_session")?;
    store_session_id(data_dir, conversation_id, &session.session_id)?;
    Ok(session.session_id)
}

async fn ensure_acp_send_endpoint(data_dir: &PathBuf) -> anyhow::Result<()> {
    let base_url = daemon_base_url(data_dir)?;
    let response = http_client()
        .post(format!("{base_url}/v1/conversations/__probe__/send"))
        .json(&SendMessageRequest {
            message: String::new(),
            conversation_id: None,
            content_type: None,
        })
        .send()
        .await
        .context("probe conversation send endpoint")?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let json_error = serde_json::from_str::<ApiErrorBody>(&body).ok();

    if status == reqwest::StatusCode::NOT_FOUND && json_error.is_none() {
        anyhow::bail!("Daemon is outdated, please restart: xmtp-cli shutdown");
    }

    Ok(())
}

async fn bridge_history_to_acp(
    data_dir: &PathBuf,
    conversation_id: &str,
    self_inbox_id: Option<&str>,
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
    chunks: &Arc<Mutex<HashMap<String, Vec<String>>>>,
) -> anyhow::Result<()> {
    let base_url = daemon_base_url(data_dir)?;
    let url = format!("{base_url}/v1/conversations/{conversation_id}/events");
    let response = http_client()
        .get(url)
        .send()
        .await
        .context("open ACP history SSE stream")?
        .error_for_status()
        .context("ACP history SSE status")?;
    let mut stream = response.bytes_stream().eventsource();

    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.context("wait for ctrl-c")?;
                break;
            }
            event = stream.next() => {
                let Some(event) = event else {
                    anyhow::bail!("ACP history SSE stream ended");
                };
                let event = event.context("read ACP history SSE event")?;
                let envelope: DaemonEventEnvelope =
                    serde_json::from_str(&event.data).context("decode ACP SSE envelope")?;
                if let DaemonEventData::HistoryItem { item, .. } = envelope.payload {
                    if should_forward_item(&item, self_inbox_id) {
                        let reply = prompt_agent(conn, session_id, chunks, item).await?;
                        if !reply.trim().is_empty() {
                            daemon_send_conversation(
                                data_dir,
                                conversation_id,
                                &reply,
                                Some("markdown"),
                            )
                            .await
                            .with_context(|| {
                                format!(
                                    "send ACP reply back to conversation {conversation_id}; if this looks like a stale daemon, restart it with `xmtp-cli shutdown`"
                                )
                            })?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn prompt_agent(
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
    chunks: &Arc<Mutex<HashMap<String, Vec<String>>>>,
    item: HistoryItem,
) -> anyhow::Result<String> {
    clear_session_chunks(chunks, session_id_str(session_id));
    let content = item.content;
    conn.prompt(acp::PromptRequest::new(
        session_id.clone(),
        vec![content.into()],
    ))
    .await
    .context("ACP prompt")?;

    // Allow any final session notifications to land before we read the buffered chunks.
    sleep(Duration::from_millis(100)).await;

    Ok(take_session_chunks(chunks, session_id_str(session_id)).join(""))
}

fn should_forward_item(item: &HistoryItem, self_inbox_id: Option<&str>) -> bool {
    if self_inbox_id == Some(item.sender_inbox_id.as_str()) {
        return false;
    }
    matches!(item.content_kind.as_str(), "text" | "markdown" | "reply")
}

fn clear_session_chunks(chunks: &Arc<Mutex<HashMap<String, Vec<String>>>>, session_id: &str) {
    if let Ok(mut chunks) = chunks.lock() {
        chunks.insert(session_id.to_owned(), Vec::new());
    }
}

fn take_session_chunks(
    chunks: &Arc<Mutex<HashMap<String, Vec<String>>>>,
    session_id: &str,
) -> Vec<String> {
    chunks
        .lock()
        .ok()
        .and_then(|mut chunks| chunks.remove(session_id))
        .unwrap_or_default()
}

fn session_id_str(session_id: &acp::SessionId) -> &str {
    session_id.0.as_ref()
}

fn session_id_owned(session_id: &acp::SessionId) -> String {
    session_id_str(session_id).to_owned()
}

struct BridgeClient {
    chunks: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

#[async_trait::async_trait(?Send)]
impl acp::Client for BridgeClient {
    async fn request_permission(
        &self,
        _args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn session_notification(
        &self,
        args: acp::SessionNotification,
    ) -> acp::Result<(), acp::Error> {
        if let acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk { content, .. }) = args.update {
            let text = match content {
                acp::ContentBlock::Text(text) => text.text,
                acp::ContentBlock::Image(_) => "<image>".to_owned(),
                acp::ContentBlock::Audio(_) => "<audio>".to_owned(),
                acp::ContentBlock::ResourceLink(link) => link.uri,
                acp::ContentBlock::Resource(_) => "<resource>".to_owned(),
                _ => "<unsupported>".to_owned(),
            };
            if let Ok(mut chunks) = self.chunks.lock() {
                chunks
                    .entry(session_id_owned(&args.session_id))
                    .or_default()
                    .push(text);
            }
        }
        Ok(())
    }

    async fn write_text_file(
        &self,
        _args: acp::WriteTextFileRequest,
    ) -> acp::Result<acp::WriteTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn read_text_file(
        &self,
        _args: acp::ReadTextFileRequest,
    ) -> acp::Result<acp::ReadTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn create_terminal(
        &self,
        _args: acp::CreateTerminalRequest,
    ) -> acp::Result<acp::CreateTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> acp::Result<acp::TerminalOutputResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn kill_terminal(
        &self,
        _args: acp::KillTerminalRequest,
    ) -> acp::Result<acp::KillTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_method(&self, _args: acp::ExtRequest) -> acp::Result<acp::ExtResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(&self, _args: acp::ExtNotification) -> acp::Result<()> {
        Err(acp::Error::method_not_found())
    }
}
