use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use agent_client_protocol::{self as acp, Agent as _};
use anyhow::{Context, anyhow};
use chrono::Utc;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use tokio::process::Command;
use tokio::task::LocalSet;
use tokio::time::{Duration, sleep};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use xmtp_daemon::resolve_conversation_id;
use xmtp_ipc::{
    ApiErrorBody, ConversationInfoResponse, DaemonEventData, DaemonEventEnvelope, EmojiRequest,
    HistoryItem, SendMessageRequest, StatusResponse,
};

use crate::{
    daemon_base_url, daemon_send_conversation, http_client, http_get, wait_for_daemon_ready,
};

pub async fn run_acp(
    data_dir: PathBuf,
    conversation_id: String,
    context_prefix: bool,
    enable_reaction: bool,
    command: Vec<String>,
) -> anyhow::Result<()> {
    let local = LocalSet::new();
    local
        .run_until(run_acp_inner(
            data_dir,
            conversation_id,
            context_prefix,
            enable_reaction,
            command,
        ))
        .await
}

async fn run_acp_inner(
    data_dir: PathBuf,
    conversation_id: String,
    context_prefix: bool,
    enable_reaction: bool,
    command: Vec<String>,
) -> anyhow::Result<()> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow!("ACP command is required"))?;

    wait_for_daemon_ready(&data_dir, 4_000).await?;
    let status: StatusResponse = http_get(&data_dir, "/v1/status")
        .await
        .context("load daemon status")?;
    let conversation_id = resolve_conversation_id(&data_dir, &conversation_id, None)
        .context("resolve conversation ID")?;
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
    let session_id = ensure_acp_session(
        &data_dir,
        &conversation_id,
        self_inbox_id.as_deref(),
        context_prefix,
        &conn,
        &cwd,
        &chunks,
    )
    .await?;
    eprintln!("ACP session ready: {}", session_id_str(&session_id));

    let bridge_result = bridge_history_to_acp(
        &data_dir,
        &conversation_id,
        self_inbox_id.as_deref(),
        context_prefix,
        enable_reaction,
        &conn,
        &session_id,
        &chunks,
    )
    .await;

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

fn acp_log_path(data_dir: &Path, conversation_id: &str) -> PathBuf {
    data_dir
        .join("logs")
        .join("acp")
        .join(format!("{conversation_id}.jsonl"))
}

fn append_acp_log(
    data_dir: &Path,
    conversation_id: &str,
    entry: &serde_json::Value,
) -> anyhow::Result<()> {
    let path = acp_log_path(data_dir, conversation_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    serde_json::to_writer(&mut file, entry).context("encode ACP log entry")?;
    writeln!(&mut file).context("append ACP log newline")?;
    Ok(())
}

fn log_acp_event(data_dir: &Path, conversation_id: &str, mut entry: serde_json::Value) {
    if let Some(map) = entry.as_object_mut() {
        map.insert(
            "ts".to_owned(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
    }
    if let Err(err) = append_acp_log(data_dir, conversation_id, &entry) {
        eprintln!("ACP log write failed: {err:#}");
    }
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
    Ok(load_acp_sessions(data_dir)?
        .sessions
        .get(conversation_id)
        .cloned())
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
    self_inbox_id: Option<&str>,
    context_prefix: bool,
    conn: &acp::ClientSideConnection,
    cwd: &Path,
    chunks: &Arc<Mutex<HashMap<String, Vec<String>>>>,
) -> anyhow::Result<acp::SessionId> {
    if let Some(saved_session_id) = persisted_session_id(data_dir, conversation_id)? {
        let session_id = acp::SessionId::new(saved_session_id);
        match conn
            .load_session(acp::LoadSessionRequest::new(
                session_id.clone(),
                cwd.to_path_buf(),
            ))
            .await
        {
            Ok(_) => {
                eprintln!("ACP session restored: {}", session_id_str(&session_id));
                log_acp_event(
                    data_dir,
                    conversation_id,
                    serde_json::json!({
                        "event": "session_event",
                        "action": "restored",
                        "session_id": session_id_str(&session_id),
                    }),
                );
                store_session_id(data_dir, conversation_id, &session_id)?;
                return Ok(session_id);
            }
            Err(err) => {
                eprintln!(
                    "ACP session restore failed for conversation {}: {err:#}",
                    conversation_id
                );
                log_acp_event(
                    data_dir,
                    conversation_id,
                    serde_json::json!({
                        "event": "error",
                        "message": format!("ACP session restore failed: {err:#}"),
                    }),
                );
            }
        }
    }

    let session = conn
        .new_session(acp::NewSessionRequest::new(cwd.to_path_buf()))
        .await
        .context("ACP new_session")?;
    log_acp_event(
        data_dir,
        conversation_id,
        serde_json::json!({
            "event": "session_event",
            "action": "created",
            "session_id": session_id_str(&session.session_id),
        }),
    );
    store_session_id(data_dir, conversation_id, &session.session_id)?;
    if context_prefix {
        send_bootstrap_prompt(
            data_dir,
            conversation_id,
            self_inbox_id,
            conn,
            &session.session_id,
            chunks,
        )
        .await?;
    }
    Ok(session.session_id)
}

async fn ensure_acp_send_endpoint(data_dir: &Path) -> anyhow::Result<()> {
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
    data_dir: &Path,
    conversation_id: &str,
    self_inbox_id: Option<&str>,
    context_prefix: bool,
    enable_reaction: bool,
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
    chunks: &Arc<Mutex<HashMap<String, Vec<String>>>>,
) -> anyhow::Result<()> {
    let mut retry_delay = Duration::from_millis(100);
    let mut reconnect_attempt: u64 = 0;

    loop {
        let base_url = match daemon_base_url(data_dir) {
            Ok(base_url) => base_url,
            Err(err) => {
                eprintln!("ACP history base URL error: {err:#}");
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                log_acp_event(
                    data_dir,
                    conversation_id,
                    serde_json::json!({
                        "event": "error",
                        "message": format!("ACP history base URL error: {err:#}"),
                    }),
                );
                log_acp_event(
                    data_dir,
                    conversation_id,
                    serde_json::json!({
                        "event": "sse_reconnect",
                        "attempt": reconnect_attempt,
                        "delay_ms": retry_delay.as_millis() as u64,
                    }),
                );
                tokio::select! {
                    signal = tokio::signal::ctrl_c() => {
                        signal.context("wait for ctrl-c")?;
                        break;
                    }
                    _ = sleep(retry_delay) => {}
                }
                retry_delay = next_retry_delay(retry_delay);
                continue;
            }
        };
        let url = format!("{base_url}/v1/conversations/{conversation_id}/events");
        let response = match http_client().get(url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => response,
                Err(err) => {
                    eprintln!("ACP history SSE status error: {err:#}");
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                    log_acp_event(
                        data_dir,
                        conversation_id,
                        serde_json::json!({
                            "event": "error",
                            "message": format!("ACP history SSE status error: {err:#}"),
                        }),
                    );
                    log_acp_event(
                        data_dir,
                        conversation_id,
                        serde_json::json!({
                            "event": "sse_reconnect",
                            "attempt": reconnect_attempt,
                            "delay_ms": retry_delay.as_millis() as u64,
                        }),
                    );
                    tokio::select! {
                        signal = tokio::signal::ctrl_c() => {
                            signal.context("wait for ctrl-c")?;
                            break;
                        }
                        _ = sleep(retry_delay) => {}
                    }
                    retry_delay = next_retry_delay(retry_delay);
                    continue;
                }
            },
            Err(err) => {
                eprintln!("ACP history SSE connect error: {err:#}");
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                log_acp_event(
                    data_dir,
                    conversation_id,
                    serde_json::json!({
                        "event": "error",
                        "message": format!("ACP history SSE connect error: {err:#}"),
                    }),
                );
                log_acp_event(
                    data_dir,
                    conversation_id,
                    serde_json::json!({
                        "event": "sse_reconnect",
                        "attempt": reconnect_attempt,
                        "delay_ms": retry_delay.as_millis() as u64,
                    }),
                );
                tokio::select! {
                    signal = tokio::signal::ctrl_c() => {
                        signal.context("wait for ctrl-c")?;
                        break;
                    }
                    _ = sleep(retry_delay) => {}
                }
                retry_delay = next_retry_delay(retry_delay);
                continue;
            }
        };

        retry_delay = Duration::from_millis(100);
        reconnect_attempt = 0;
        let mut stream = response.bytes_stream().eventsource();

        loop {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    signal.context("wait for ctrl-c")?;
                    return Ok(());
                }
                event = stream.next() => {
                    let Some(event) = event else {
                        reconnect_attempt = reconnect_attempt.saturating_add(1);
                        log_acp_event(
                            data_dir,
                            conversation_id,
                            serde_json::json!({
                                "event": "sse_reconnect",
                                "attempt": reconnect_attempt,
                                "delay_ms": retry_delay.as_millis() as u64,
                            }),
                        );
                        break;
                    };
                    let event = match event.context("read ACP history SSE event") {
                        Ok(event) => event,
                        Err(err) => {
                            eprintln!("ACP history SSE read error: {err:#}");
                            reconnect_attempt = reconnect_attempt.saturating_add(1);
                            log_acp_event(
                                data_dir,
                                conversation_id,
                                serde_json::json!({
                                    "event": "error",
                                    "message": format!("ACP history SSE read error: {err:#}"),
                                }),
                            );
                            log_acp_event(
                                data_dir,
                                conversation_id,
                                serde_json::json!({
                                    "event": "sse_reconnect",
                                    "attempt": reconnect_attempt,
                                    "delay_ms": retry_delay.as_millis() as u64,
                                }),
                            );
                            break;
                        }
                    };

                    retry_delay = Duration::from_millis(100);
                    reconnect_attempt = 0;
                    let envelope: DaemonEventEnvelope =
                        match serde_json::from_str(&event.data).context("decode ACP SSE envelope") {
                            Ok(envelope) => envelope,
                            Err(err) => {
                                eprintln!("ACP history SSE decode error: {err:#}");
                                reconnect_attempt = reconnect_attempt.saturating_add(1);
                                log_acp_event(
                                    data_dir,
                                    conversation_id,
                                    serde_json::json!({
                                        "event": "error",
                                        "message": format!("ACP history SSE decode error: {err:#}"),
                                    }),
                                );
                                log_acp_event(
                                    data_dir,
                                    conversation_id,
                                    serde_json::json!({
                                        "event": "sse_reconnect",
                                        "attempt": reconnect_attempt,
                                        "delay_ms": retry_delay.as_millis() as u64,
                                    }),
                                );
                                break;
                            }
                        };
                    if let DaemonEventData::HistoryItem { item, .. } = envelope.payload
                        && should_forward_item(&item, self_inbox_id)
                    {
                        eprintln!(
                            ">> received [{}] {}",
                            sender_short_id(&item.sender_inbox_id),
                            truncate_display(&item.content, 80),
                        );
                        if enable_reaction {
                            send_processing_reaction(
                                data_dir,
                                conversation_id,
                                &base_url,
                                &item.message_id,
                            );
                        }
                        log_acp_event(
                            data_dir,
                            conversation_id,
                            serde_json::json!({
                                "event": "user_message",
                                "conversation_id": conversation_id,
                                "sender_inbox_id": item.sender_inbox_id.clone(),
                                "content": item.content.clone(),
                            }),
                        );
                        let reply = prompt_agent(
                            data_dir,
                            conversation_id,
                            conn,
                            session_id,
                            chunks,
                            item,
                            context_prefix,
                        )
                        .await?;
                        if !reply.trim().is_empty() {
                            eprintln!(
                                "<< sending reply ({}B)",
                                reply.len(),
                            );
                            let sent = daemon_send_conversation(
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
                            eprintln!("<< reply sent (message_id={})", sender_short_id(&sent.message_id));
                            log_acp_event(
                                data_dir,
                                conversation_id,
                                serde_json::json!({
                                    "event": "xmtp_sent",
                                    "conversation_id": conversation_id,
                                    "message_id": sent.message_id,
                                }),
                            );
                        }
                    }
                }
            }
        }

        sleep(retry_delay).await;
        retry_delay = next_retry_delay(retry_delay);
    }

    Ok(())
}

async fn prompt_agent(
    data_dir: &Path,
    conversation_id: &str,
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
    chunks: &Arc<Mutex<HashMap<String, Vec<String>>>>,
    item: HistoryItem,
    context_prefix: bool,
) -> anyhow::Result<String> {
    clear_session_chunks(chunks, session_id_str(session_id));
    let content = if context_prefix {
        format!(
            "[{}] {}",
            sender_short_id(&item.sender_inbox_id),
            item.content
        )
    } else {
        item.content
    };
    log_acp_event(
        data_dir,
        conversation_id,
        serde_json::json!({
            "event": "agent_prompt",
            "session_id": session_id_str(session_id),
            "content": content,
        }),
    );
    eprintln!("<< prompting agent...");
    conn.prompt(acp::PromptRequest::new(
        session_id.clone(),
        vec![content.clone().into()],
    ))
    .await
    .context("ACP prompt")?;
    eprintln!("<< agent responded");

    // Allow any final session notifications to land before we read the buffered chunks.
    sleep(Duration::from_millis(100)).await;

    let reply = take_session_chunks(chunks, session_id_str(session_id)).join("");
    log_acp_event(
        data_dir,
        conversation_id,
        serde_json::json!({
            "event": "agent_reply",
            "session_id": session_id_str(session_id),
            "content": reply,
        }),
    );
    Ok(reply)
}

async fn send_bootstrap_prompt(
    data_dir: &Path,
    conversation_id: &str,
    self_inbox_id: Option<&str>,
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
    chunks: &Arc<Mutex<HashMap<String, Vec<String>>>>,
) -> anyhow::Result<()> {
    let info: ConversationInfoResponse =
        http_get(data_dir, &format!("/v1/conversations/{conversation_id}"))
            .await
            .context("load conversation info for ACP bootstrap")?;
    let bootstrap = format!(
        "You are an AI agent connected to an XMTP messaging conversation.\n\nContext:\n- Type: {} ({} members)\n- Conversation ID: {}\n- Your identity (inbox ID): {}\n\nMessage format:\nEach user message is prefixed with the sender short ID in brackets:\n  [0x1a2b3c4d] Hello everyone\n\nRules:\n- Respond with plain text only, do NOT include any prefix in your replies.\n- In group chats, pay attention to who said what.\n- If the conversation type is dm, there is only one other participant.",
        info.conversation_type,
        info.member_count,
        conversation_id,
        self_inbox_id.unwrap_or("unknown"),
    );
    clear_session_chunks(chunks, session_id_str(session_id));
    log_acp_event(
        data_dir,
        conversation_id,
        serde_json::json!({
            "event": "agent_bootstrap_prompt",
            "session_id": session_id_str(session_id),
            "content": bootstrap,
        }),
    );
    conn.prompt(acp::PromptRequest::new(
        session_id.clone(),
        vec![bootstrap.into()],
    ))
    .await
    .context("ACP bootstrap prompt")?;

    sleep(Duration::from_millis(100)).await;
    let discarded_reply = take_session_chunks(chunks, session_id_str(session_id)).join("");
    if !discarded_reply.trim().is_empty() {
        log_acp_event(
            data_dir,
            conversation_id,
            serde_json::json!({
                "event": "agent_bootstrap_reply_discarded",
                "session_id": session_id_str(session_id),
                "content": discarded_reply,
            }),
        );
    }
    Ok(())
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

fn next_retry_delay(current: Duration) -> Duration {
    let next_ms = (current.as_millis() as u64).saturating_mul(2).min(5_000);
    Duration::from_millis(next_ms.max(100))
}

fn sender_short_id(sender_inbox_id: &str) -> String {
    sender_inbox_id.chars().take(8).collect()
}

fn truncate_display(s: &str, max: usize) -> String {
    let single_line: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if single_line.len() <= max {
        single_line
    } else {
        format!("{}...", &single_line[..max])
    }
}

fn send_processing_reaction(
    data_dir: &Path,
    conversation_id: &str,
    base_url: &str,
    message_id: &str,
) {
    let data_dir = data_dir.to_path_buf();
    let conversation_id = conversation_id.to_owned();
    let base_url = base_url.to_owned();
    let message_id = message_id.to_owned();
    tokio::spawn(async move {
        let result = http_client()
            .post(format!("{base_url}/v1/messages/{message_id}/react"))
            .json(&EmojiRequest {
                emoji: "👀".to_owned(),
                action: Some("add".to_owned()),
                conversation_id: Some(conversation_id.clone()),
            })
            .send()
            .await;

        match result {
            Ok(response) => {
                if let Err(err) = response.error_for_status_ref() {
                    eprintln!("ACP reaction send failed for message {message_id}: {err:#}");
                    log_acp_event(
                        &data_dir,
                        &conversation_id,
                        serde_json::json!({
                            "event": "warning",
                            "message": format!("ACP reaction send failed for message {message_id}: {err:#}"),
                        }),
                    );
                } else {
                    log_acp_event(
                        &data_dir,
                        &conversation_id,
                        serde_json::json!({
                            "event": "reaction_sent",
                            "message_id": message_id,
                            "emoji": "👀",
                        }),
                    );
                }
            }
            Err(err) => {
                eprintln!("ACP reaction send failed for message {message_id}: {err:#}");
                log_acp_event(
                    &data_dir,
                    &conversation_id,
                    serde_json::json!({
                        "event": "warning",
                        "message": format!("ACP reaction send failed for message {message_id}: {err:#}"),
                    }),
                );
            }
        }
    });
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
        if let acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk { content, .. }) =
            args.update
        {
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
