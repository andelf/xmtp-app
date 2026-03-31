use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};

use agent_client_protocol::{self as acp, Agent as _};
use anyhow::{Context, anyhow};
use chrono::Utc;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use tokio::process::Command;
use tokio::task::LocalSet;
use tokio::time::{Duration, sleep};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info, warn};
use xmtp_core::ConnectionState;
use xmtp_daemon::resolve_conversation_id;
use xmtp_ipc::{
    ApiErrorBody, ConversationInfoResponse, DaemonEventData, DaemonEventEnvelope, EmojiRequest,
    HistoryItem, SendMessageRequest, StatusResponse,
};

use crate::{
    daemon_base_url, daemon_send_conversation, http_client, http_get, wait_for_daemon_ready,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ReplyMode {
    Single,
    Stream,
}

static TRACING_INIT: OnceLock<()> = OnceLock::new();

fn init_acp_tracing() {
    TRACING_INIT.get_or_init(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_target(false)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_ansi(true)
            .with_writer(std::io::stderr)
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

pub async fn run_acp(
    data_dir: PathBuf,
    conversation_id: String,
    context_prefix: bool,
    enable_reaction: bool,
    reply_mode: ReplyMode,
    resume: Option<String>,
    command: Vec<String>,
) -> anyhow::Result<()> {
    init_acp_tracing();
    let local = LocalSet::new();
    local
        .run_until(run_acp_inner(
            data_dir,
            conversation_id,
            context_prefix,
            enable_reaction,
            reply_mode,
            resume,
            command,
        ))
        .await
}

async fn run_acp_inner(
    data_dir: PathBuf,
    conversation_id: String,
    context_prefix: bool,
    enable_reaction: bool,
    reply_mode: ReplyMode,
    resume: Option<String>,
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
    let self_inbox_id = status.inbox_id.clone();

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

    let state = Arc::new(Mutex::new(BridgeState::default()));
    let base_url = daemon_base_url(&data_dir)?;
    let client = BridgeClient {
        base_url: base_url.clone(),
        data_dir: data_dir.clone(),
        conversation_id: conversation_id.clone(),
        enable_reaction,
        reply_mode,
        state: Arc::clone(&state),
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
            error!("ACP protocol task failed: {err:#}");
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
        resume.as_deref(),
        &conn,
        &cwd,
        &state,
    )
    .await?;
    info!(session_id = %session_id_str(&session_id), "ACP session ready");

    let bridge_result = bridge_history_to_acp(
        &data_dir,
        &conversation_id,
        self_inbox_id.as_deref(),
        context_prefix,
        enable_reaction,
        reply_mode,
        status,
        &conn,
        &session_id,
        &state,
    )
    .await;

    if let Err(err) = &bridge_result {
        error!("ACP bridge error: {err:#}");
    }

    let _ = child.start_kill();
    let _ = child.wait().await;
    io_handle.abort();
    bridge_result
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct PersistedAcpSessions {
    sessions: HashMap<String, PersistedSessionEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum PersistedSessionEntry {
    Legacy(String),
    Recent(Vec<String>),
}

impl PersistedSessionEntry {
    fn into_recent(self) -> Vec<String> {
        match self {
            Self::Legacy(session_id) => vec![session_id],
            Self::Recent(session_ids) => session_ids,
        }
    }
}

#[derive(Debug, Default)]
struct BridgeState {
    sessions: HashMap<String, SessionRuntime>,
    seen_message_ids: HashSet<String>,
}

#[derive(Debug, Default)]
struct SessionRuntime {
    chunks: Vec<String>,
    stream_buffer: String,
    full_text: String,
    tool_calls: HashMap<String, ToolCallSnapshot>,
    active_turn: Option<ActiveTurn>,
}

#[derive(Debug, Default)]
struct ActiveTurn {
    source_message_id: String,
    started_tool_calls: HashSet<String>,
}

#[derive(Debug, Default)]
struct PromptReply {
    full_text: String,
    remaining_text: String,
}

#[derive(Debug, Clone, Default)]
struct ToolCallSnapshot {
    title: Option<String>,
    raw_input: Option<serde_json::Value>,
    raw_output: Option<serde_json::Value>,
    meta: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReactionEmoji {
    Eyes,
    Tool,
    Subagent,
    Warning,
    Done,
}

impl ReactionEmoji {
    fn as_str(self) -> &'static str {
        match self {
            Self::Eyes => "👀",
            Self::Tool => "🛠️",
            Self::Subagent => "🤖",
            Self::Warning => "⚠️",
            Self::Done => "✅",
        }
    }
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
        warn!("ACP log write failed: {err:#}");
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

fn persisted_session_ids(data_dir: &Path, conversation_id: &str) -> anyhow::Result<Vec<String>> {
    Ok(load_acp_sessions(data_dir)?
        .sessions
        .remove(conversation_id)
        .map(PersistedSessionEntry::into_recent)
        .unwrap_or_default())
}

fn resolve_resume_session_id(
    data_dir: &Path,
    conversation_id: &str,
    selector: &str,
) -> anyhow::Result<Option<String>> {
    let session_ids = persisted_session_ids(data_dir, conversation_id)?;
    resolve_resume_session_selector(&session_ids, selector)
}

fn resolve_resume_session_selector(
    session_ids: &[String],
    selector: &str,
) -> anyhow::Result<Option<String>> {
    if session_ids.is_empty() {
        return Ok(None);
    }

    if selector == "latest" {
        return Ok(session_ids.first().cloned());
    }

    if let Ok(index) = selector.parse::<usize>() {
        if index == 0 {
            anyhow::bail!("ACP resume index must start at 1");
        }
        return session_ids
            .get(index - 1)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                anyhow!(
                    "ACP resume index {} is out of range; only {} recent sessions are available",
                    index,
                    session_ids.len()
                )
            });
    }

    if session_ids.iter().any(|session_id| session_id == selector) {
        return Ok(Some(selector.to_owned()));
    }

    anyhow::bail!(
        "ACP session `{}` was not found in the recent session list for this conversation",
        selector
    );
}

fn store_session_id(
    data_dir: &Path,
    conversation_id: &str,
    session_id: &acp::SessionId,
) -> anyhow::Result<()> {
    let mut sessions = load_acp_sessions(data_dir)?;
    let session_id = session_id_owned(session_id);
    let mut recent = sessions
        .sessions
        .remove(conversation_id)
        .map(PersistedSessionEntry::into_recent)
        .unwrap_or_default();
    recent.retain(|existing| existing != &session_id);
    recent.insert(0, session_id);
    recent.truncate(10);
    sessions.sessions.insert(
        conversation_id.to_owned(),
        PersistedSessionEntry::Recent(recent),
    );
    save_acp_sessions(data_dir, &sessions)
}

async fn ensure_acp_session(
    data_dir: &Path,
    conversation_id: &str,
    self_inbox_id: Option<&str>,
    context_prefix: bool,
    resume: Option<&str>,
    conn: &acp::ClientSideConnection,
    cwd: &Path,
    state: &Arc<Mutex<BridgeState>>,
) -> anyhow::Result<acp::SessionId> {
    if let Some(selector) = resume
        && let Some(saved_session_id) =
            resolve_resume_session_id(data_dir, conversation_id, selector)?
    {
        let session_id = acp::SessionId::new(saved_session_id);
        match conn
            .load_session(acp::LoadSessionRequest::new(
                session_id.clone(),
                cwd.to_path_buf(),
            ))
            .await
        {
            Ok(_) => {
                info!(session_id = %session_id_str(&session_id), "ACP session restored");
                log_acp_event(
                    data_dir,
                    conversation_id,
                    serde_json::json!({
                        "event": "session_event",
                        "action": "restored",
                        "session_id": session_id_str(&session_id),
                        "resume_selector": selector,
                    }),
                );
                store_session_id(data_dir, conversation_id, &session_id)?;
                return Ok(session_id);
            }
            Err(err) => {
                warn!(
                    conversation_id = conversation_id,
                    "ACP session restore failed: {err:#}"
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
    info!(session_id = %session_id_str(&session.session_id), "ACP session created");
    log_acp_event(
        data_dir,
        conversation_id,
        serde_json::json!({
            "event": "session_event",
            "action": "created",
            "session_id": session_id_str(&session.session_id),
            "resume_selector": resume,
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
            state,
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
    reply_mode: ReplyMode,
    initial_status: StatusResponse,
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
    state: &Arc<Mutex<BridgeState>>,
) -> anyhow::Result<()> {
    let mut retry_delay = Duration::from_millis(100);
    let mut reconnect_attempt: u64 = 0;
    let mut last_connection_state = initial_status.connection_state;

    seed_seen_history_items(data_dir, conversation_id, self_inbox_id, state)
        .await
        .context("seed existing conversation history for ACP bridge")?;

    loop {
        let base_url = match daemon_base_url(data_dir) {
            Ok(base_url) => base_url,
            Err(err) => {
                error!("ACP history base URL error: {err:#}");
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
                    error!("ACP history SSE status error: {err:#}");
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
                error!("ACP history SSE connect error: {err:#}");
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
        let mut status_tick = tokio::time::interval(Duration::from_secs(2));
        status_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    signal.context("wait for ctrl-c")?;
                    return Ok(());
                }
                _ = status_tick.tick() => {
                    let status: StatusResponse = match http_get(data_dir, "/v1/status").await {
                        Ok(status) => status,
                        Err(err) => {
                            warn!("ACP daemon status poll failed: {err:#}");
                            log_acp_event(
                                data_dir,
                                conversation_id,
                                serde_json::json!({
                                    "event": "error",
                                    "message": format!("ACP daemon status poll failed: {err:#}"),
                                }),
                            );
                            continue;
                        }
                    };

                    if status.connection_state != last_connection_state {
                        info!(
                            daemon_state = %status.daemon_state,
                            connection_state = %status.connection_state,
                            "daemon status changed"
                        );
                        log_acp_event(
                            data_dir,
                            conversation_id,
                            serde_json::json!({
                                "event": "daemon_status",
                                "daemon_state": status.daemon_state,
                                "connection_state": status.connection_state,
                            }),
                        );

                        let should_force_reconnect = last_connection_state == ConnectionState::Disconnected
                            && status.connection_state == ConnectionState::Connected;
                        last_connection_state = status.connection_state;

                        if should_force_reconnect {
                            reconnect_attempt = reconnect_attempt.saturating_add(1);
                            log_acp_event(
                                data_dir,
                                conversation_id,
                                serde_json::json!({
                                    "event": "sse_reconnect",
                                    "attempt": reconnect_attempt,
                                    "reason": "connection_restored",
                                    "delay_ms": 0,
                                }),
                            );
                            break;
                        }
                    }
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
                            warn!("ACP history SSE read error: {err:#}");
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
                                warn!("ACP history SSE decode error: {err:#}");
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
                        && remember_processed_message(state, &item.message_id)
                    {
                        let source_message_id = item.message_id.clone();
                        info!(
                            sender = %sender_short_id(&item.sender_inbox_id),
                            message = %truncate_display(&item.content, 80),
                            "received conversation message"
                        );
                        if enable_reaction {
                            send_reaction(
                                &base_url,
                                data_dir,
                                conversation_id,
                                &source_message_id,
                                ReactionEmoji::Eyes,
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
                        let reply = match prompt_agent(
                            data_dir,
                            conversation_id,
                            conn,
                            session_id,
                            state,
                            reply_mode,
                            item,
                            context_prefix,
                        )
                        .await {
                            Ok(reply) => reply,
                            Err(err) => {
                                error!("ACP prompt failed: {err:#}");
                                if enable_reaction {
                                    send_reaction(
                                        &base_url,
                                        data_dir,
                                        conversation_id,
                                        &source_message_id,
                                        ReactionEmoji::Warning,
                                    );
                                }
                                log_acp_event(
                                    data_dir,
                                    conversation_id,
                                    serde_json::json!({
                                        "event": "error",
                                        "message": format!("ACP prompt failed: {err:#}"),
                                    }),
                                );
                                send_bridge_error_message(
                                    data_dir,
                                    conversation_id,
                                    format_agent_error_message(&err),
                                )
                                .await;
                                continue;
                            }
                        };
                        if !reply.full_text.trim().is_empty() {
                            let reply_parts = match reply_mode {
                                ReplyMode::Single => {
                                    if reply.remaining_text.trim().is_empty() {
                                        vec![]
                                    } else {
                                        vec![split_markdown_reply(&reply.remaining_text)
                                            .join("\n\n")]
                                    }
                                }
                                ReplyMode::Stream => {
                                    split_markdown_reply(&reply.remaining_text)
                                }
                            };
                            info!(
                                parts = reply_parts.len(),
                                bytes = reply.full_text.len(),
                                "sending reply"
                            );
                            let mut send_failed = false;
                            for (index, reply_part) in reply_parts.iter().enumerate() {
                                let message_id = match send_reply_part(
                                    data_dir,
                                    conversation_id,
                                    reply_part,
                                    "xmtp_sent",
                                )
                                .await {
                                    Ok(message_id) => message_id,
                                    Err(err) => {
                                        error!("ACP reply send failed: {err:#}");
                                        log_acp_event(
                                            data_dir,
                                            conversation_id,
                                            serde_json::json!({
                                                "event": "error",
                                                "message": format!("ACP reply send failed: {err:#}"),
                                            }),
                                        );
                                        send_failed = true;
                                        break;
                                    }
                                };
                                info!(
                                    message_index = index + 1,
                                    message_count = reply_parts.len(),
                                    message_id = %sender_short_id(&message_id),
                                    "reply part sent"
                                );
                                log_acp_event(
                                    data_dir,
                                    conversation_id,
                                    serde_json::json!({
                                        "event": "xmtp_sent_meta",
                                        "conversation_id": conversation_id,
                                        "message_id": message_id,
                                        "message_index": index + 1,
                                        "message_count": reply_parts.len(),
                                    }),
                                );
                            }
                            if send_failed {
                                continue;
                            }
                            if enable_reaction && reply_mode == ReplyMode::Stream {
                                send_reaction(
                                    &base_url,
                                    data_dir,
                                    conversation_id,
                                    &source_message_id,
                                    ReactionEmoji::Done,
                                );
                            }
                        } else {
                            let message = "ACP agent could not produce a reply for this message. Check the agent/session logs for details.".to_owned();
                            log_acp_event(
                                data_dir,
                                conversation_id,
                                serde_json::json!({
                                    "event": "warning",
                                    "message": message,
                                }),
                            );
                            if enable_reaction {
                                send_reaction(
                                    &base_url,
                                    data_dir,
                                    conversation_id,
                                    &source_message_id,
                                    ReactionEmoji::Warning,
                                );
                            }
                            send_bridge_error_message(
                                data_dir,
                                conversation_id,
                                "ACP agent could not produce a reply for this message.".to_owned(),
                            )
                            .await;
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
    state: &Arc<Mutex<BridgeState>>,
    reply_mode: ReplyMode,
    item: HistoryItem,
    context_prefix: bool,
) -> anyhow::Result<PromptReply> {
    begin_session_turn(state, session_id_str(session_id), &item.message_id);
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
    info!("prompting agent");
    let prompt_result = conn
        .prompt(acp::PromptRequest::new(
            session_id.clone(),
            vec![content.clone().into()],
        ))
        .await;
    if let Err(err) = prompt_result {
        end_session_turn(state, session_id_str(session_id));
        return Err(err).context("ACP prompt");
    }
    info!("agent responded");

    // Allow any final session notifications to land before we read the buffered chunks.
    sleep(Duration::from_millis(100)).await;

    let remaining_chunks = take_session_chunks(state, session_id_str(session_id)).join("");
    let full_text = take_session_full_text(state, session_id_str(session_id));
    let remaining_stream = take_session_stream_buffer(state, session_id_str(session_id));
    let remaining_text = match reply_mode {
        ReplyMode::Single => remaining_chunks,
        ReplyMode::Stream => remaining_stream,
    };
    end_session_turn(state, session_id_str(session_id));
    log_acp_event(
        data_dir,
        conversation_id,
        serde_json::json!({
            "event": "agent_reply",
            "session_id": session_id_str(session_id),
            "content": full_text,
            "mode": match reply_mode {
                ReplyMode::Single => "single",
                ReplyMode::Stream => "stream",
            },
        }),
    );
    Ok(PromptReply {
        full_text,
        remaining_text,
    })
}

async fn send_bootstrap_prompt(
    data_dir: &Path,
    conversation_id: &str,
    self_inbox_id: Option<&str>,
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
    state: &Arc<Mutex<BridgeState>>,
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
    reset_session_runtime(state, session_id_str(session_id));
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
    let discarded_reply = take_session_chunks(state, session_id_str(session_id)).join("");
    reset_session_runtime(state, session_id_str(session_id));
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

async fn seed_seen_history_items(
    data_dir: &Path,
    conversation_id: &str,
    self_inbox_id: Option<&str>,
    state: &Arc<Mutex<BridgeState>>,
) -> anyhow::Result<()> {
    let mut before_ns = None;

    for _ in 0..100 {
        let path = match before_ns {
            Some(before_ns) => {
                format!(
                    "/v1/conversations/{conversation_id}/history?limit=200&before_ns={before_ns}"
                )
            }
            None => format!("/v1/conversations/{conversation_id}/history?limit=200"),
        };
        let response: xmtp_ipc::HistoryResponse = http_get(data_dir, &path)
            .await
            .with_context(|| format!("load conversation history page for {conversation_id}"))?;
        if response.items.is_empty() {
            break;
        }

        if let Ok(mut bridge_state) = state.lock() {
            for item in &response.items {
                if should_forward_item(item, self_inbox_id) {
                    bridge_state
                        .seen_message_ids
                        .insert(item.message_id.to_lowercase());
                }
            }
        }

        if response.items.len() < 200 {
            break;
        }

        let next_before_ns = response.items.first().map(|item| item.sent_at_ns);
        if next_before_ns.is_none() || next_before_ns == before_ns {
            break;
        }
        before_ns = next_before_ns;
    }

    Ok(())
}

fn remember_processed_message(state: &Arc<Mutex<BridgeState>>, message_id: &str) -> bool {
    if let Ok(mut bridge_state) = state.lock() {
        return bridge_state
            .seen_message_ids
            .insert(message_id.to_lowercase());
    }
    true
}

fn reset_session_runtime(state: &Arc<Mutex<BridgeState>>, session_id: &str) {
    if let Ok(mut state) = state.lock() {
        state
            .sessions
            .insert(session_id.to_owned(), SessionRuntime::default());
    }
}

fn begin_session_turn(state: &Arc<Mutex<BridgeState>>, session_id: &str, source_message_id: &str) {
    if let Ok(mut state) = state.lock() {
        let runtime = state.sessions.entry(session_id.to_owned()).or_default();
        runtime.chunks.clear();
        runtime.stream_buffer.clear();
        runtime.full_text.clear();
        runtime.tool_calls.clear();
        runtime.active_turn = Some(ActiveTurn {
            source_message_id: source_message_id.to_owned(),
            started_tool_calls: HashSet::new(),
        });
    }
}

fn end_session_turn(state: &Arc<Mutex<BridgeState>>, session_id: &str) {
    if let Ok(mut state) = state.lock()
        && let Some(runtime) = state.sessions.get_mut(session_id)
    {
        runtime.active_turn = None;
        runtime.tool_calls.clear();
    }
}

fn take_session_chunks(state: &Arc<Mutex<BridgeState>>, session_id: &str) -> Vec<String> {
    if let Ok(mut state) = state.lock()
        && let Some(runtime) = state.sessions.get_mut(session_id)
    {
        return std::mem::take(&mut runtime.chunks);
    }
    Vec::new()
}

fn take_session_stream_buffer(state: &Arc<Mutex<BridgeState>>, session_id: &str) -> String {
    if let Ok(mut state) = state.lock()
        && let Some(runtime) = state.sessions.get_mut(session_id)
    {
        return std::mem::take(&mut runtime.stream_buffer);
    }
    String::new()
}

fn take_session_full_text(state: &Arc<Mutex<BridgeState>>, session_id: &str) -> String {
    if let Ok(mut state) = state.lock()
        && let Some(runtime) = state.sessions.get_mut(session_id)
    {
        return std::mem::take(&mut runtime.full_text);
    }
    String::new()
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
    if single_line.chars().count() <= max {
        single_line
    } else {
        format!("{}...", single_line.chars().take(max).collect::<String>())
    }
}

fn split_markdown_reply(reply: &str) -> Vec<String> {
    const MAX_PART_CHARS: usize = 1800;
    let blocks = split_markdown_blocks(reply);

    if blocks.is_empty() {
        return vec![reply.trim().to_owned()];
    }

    let mut parts = Vec::new();
    let mut index = 0;
    while index < blocks.len() {
        let mut block = blocks[index].clone();
        if block.starts_with('#')
            && let Some(next_block) = blocks.get(index + 1)
        {
            let combined = format!("{block}\n\n{next_block}");
            if combined.chars().count() <= MAX_PART_CHARS {
                block = combined;
                index += 1;
            }
        }

        if block.chars().count() > MAX_PART_CHARS {
            parts.extend(split_long_markdown_block(&block, MAX_PART_CHARS));
        } else {
            parts.push(block);
        }
        index += 1;
    }

    parts
}

fn split_long_markdown_block(block: &str, max_chars: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();

    for line in block.lines() {
        let separator = if current.is_empty() { "" } else { "\n" };
        let next_len = current.chars().count() + separator.chars().count() + line.chars().count();

        if !current.is_empty() && next_len > max_chars {
            parts.push(std::mem::take(&mut current));
        }

        if line.chars().count() > max_chars {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            let mut chunk = String::new();
            for ch in line.chars() {
                if chunk.chars().count() >= max_chars {
                    parts.push(std::mem::take(&mut chunk));
                }
                chunk.push(ch);
            }
            if !chunk.is_empty() {
                current = chunk;
            }
            continue;
        }

        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}


fn extract_streamable_markdown_parts(buffer: &str) -> (Vec<String>, String) {
    let blocks = split_markdown_blocks(buffer);
    if blocks.is_empty() {
        return (Vec::new(), buffer.to_owned());
    }

    let mut completed = blocks;
    let trailing = completed.pop().unwrap_or_default();
    let mut parts = Vec::new();
    let mut index = 0;
    while index < completed.len() {
        let mut block = completed[index].clone();
        if block.starts_with('#')
            && let Some(next_block) = completed.get(index + 1)
        {
            block = format!("{block}\n\n{next_block}");
            index += 1;
        }
        parts.push(block);
        index += 1;
    }

    (parts, trailing)
}

fn split_markdown_blocks(reply: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_code_fence = false;

    for line in reply.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
        }

        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);

        if !in_code_fence && trimmed.is_empty() {
            if !current.trim().is_empty() {
                blocks.push(current.trim().to_owned());
            }
            current.clear();
        }
    }

    if !current.trim().is_empty() {
        blocks.push(current.trim().to_owned());
    }

    blocks
}

fn format_agent_error_message(err: &anyhow::Error) -> String {
    let message = truncate_display(&err.to_string(), 240);
    format!("ACP agent error: {message}")
}

async fn send_reply_part(
    data_dir: &Path,
    conversation_id: &str,
    reply_part: &str,
    event_name: &str,
) -> anyhow::Result<String> {
    let sent = daemon_send_conversation(data_dir, conversation_id, reply_part, Some("markdown"))
        .await
        .with_context(|| {
            format!(
                "send ACP reply back to conversation {conversation_id}; if this looks like a stale daemon, restart it with `xmtp-cli shutdown`"
            )
        })?;
    log_acp_event(
        data_dir,
        conversation_id,
        serde_json::json!({
            "event": event_name,
            "conversation_id": conversation_id,
            "message_id": sent.message_id,
        }),
    );
    Ok(sent.message_id)
}

async fn send_bridge_error_message(data_dir: &Path, conversation_id: &str, message: String) {
    match daemon_send_conversation(data_dir, conversation_id, &message, Some("text")).await {
        Ok(sent) => {
            warn!(
                message_id = %sender_short_id(&sent.message_id),
                "sent ACP error message"
            );
            log_acp_event(
                data_dir,
                conversation_id,
                serde_json::json!({
                    "event": "xmtp_error_sent",
                    "conversation_id": conversation_id,
                    "message_id": sent.message_id,
                    "content": message,
                }),
            );
        }
        Err(err) => {
            error!("ACP error message send failed: {err:#}");
            log_acp_event(
                data_dir,
                conversation_id,
                serde_json::json!({
                    "event": "error",
                    "message": format!("ACP error message send failed: {err:#}"),
                    "content": message,
                }),
            );
        }
    }
}

fn send_reaction(
    base_url: &str,
    data_dir: &Path,
    conversation_id: &str,
    message_id: &str,
    emoji: ReactionEmoji,
) {
    let base_url = base_url.to_owned();
    let data_dir = data_dir.to_path_buf();
    let conversation_id = conversation_id.to_owned();
    let message_id = message_id.to_owned();
    tokio::spawn(async move {
        let result = http_client()
            .post(format!("{base_url}/v1/messages/{message_id}/react"))
            .json(&EmojiRequest {
                emoji: emoji.as_str().to_owned(),
                action: Some("add".to_owned()),
                conversation_id: Some(conversation_id.clone()),
            })
            .send()
            .await;

        match result {
            Ok(response) => {
                if let Err(err) = response.error_for_status_ref() {
                    warn!("ACP reaction send failed for message {message_id}: {err:#}");
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
                            "emoji": emoji.as_str(),
                        }),
                    );
                }
            }
            Err(err) => {
                warn!("ACP reaction send failed for message {message_id}: {err:#}");
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
    base_url: String,
    data_dir: PathBuf,
    conversation_id: String,
    enable_reaction: bool,
    reply_mode: ReplyMode,
    state: Arc<Mutex<BridgeState>>,
}

impl BridgeClient {
    fn handle_tool_call(&self, session_id: &str, tool_call: acp::ToolCall) {
        let tool_call_id = tool_call.tool_call_id.0.to_string();
        let snapshot = ToolCallSnapshot::from_tool_call(&tool_call);
        let reaction = reaction_for_tool_start(&snapshot);
        let mut source_message_id = None;

        if let Ok(mut state) = self.state.lock() {
            let runtime = state.sessions.entry(session_id.to_owned()).or_default();
            runtime
                .tool_calls
                .insert(tool_call_id.clone(), snapshot.clone());
            if let Some(active_turn) = runtime.active_turn.as_mut() {
                active_turn.started_tool_calls.insert(tool_call_id);
                source_message_id = Some(active_turn.source_message_id.clone());
            }
        }

        log_acp_event(
            &self.data_dir,
            &self.conversation_id,
            serde_json::json!({
                "event": "acp_tool_call",
                "session_id": session_id,
                "title": snapshot.title,
                "reaction": reaction.map(ReactionEmoji::as_str),
            }),
        );

        if let (true, Some(message_id), Some(emoji)) =
            (self.enable_reaction, source_message_id, reaction)
        {
            send_reaction(
                &self.base_url,
                &self.data_dir,
                &self.conversation_id,
                &message_id,
                emoji,
            );
        }
    }

    fn handle_tool_call_update(&self, session_id: &str, update: acp::ToolCallUpdate) {
        let tool_call_id = update.tool_call_id.0.to_string();
        let (snapshot, source_message_id, should_emit_start) =
            if let Ok(mut state) = self.state.lock() {
                let runtime = state.sessions.entry(session_id.to_owned()).or_default();
                let snapshot = runtime
                    .tool_calls
                    .entry(tool_call_id.clone())
                    .or_insert_with(ToolCallSnapshot::default);
                snapshot.apply_update(&update);

                let mut source_message_id = None;
                let mut should_emit_start = false;
                if let Some(active_turn) = runtime.active_turn.as_mut() {
                    source_message_id = Some(active_turn.source_message_id.clone());
                    if matches!(
                        update.fields.status,
                        Some(acp::ToolCallStatus::Pending | acp::ToolCallStatus::InProgress)
                    ) && active_turn.started_tool_calls.insert(tool_call_id)
                    {
                        should_emit_start = true;
                    }
                }

                (snapshot.clone(), source_message_id, should_emit_start)
            } else {
                (ToolCallSnapshot::default(), None, false)
            };

        let failure = matches!(update.fields.status, Some(acp::ToolCallStatus::Failed));
        let start_reaction = should_emit_start
            .then(|| reaction_for_tool_start(&snapshot))
            .flatten();

        log_acp_event(
            &self.data_dir,
            &self.conversation_id,
            serde_json::json!({
                "event": "acp_tool_call_update",
                "session_id": session_id,
                "title": snapshot.title,
                "status": update.fields.status.map(tool_status_label),
                "start_reaction": start_reaction.map(ReactionEmoji::as_str),
                "failure": failure,
            }),
        );

        if !self.enable_reaction {
            return;
        }

        if let Some(message_id) = source_message_id {
            if let Some(emoji) = start_reaction {
                send_reaction(
                    &self.base_url,
                    &self.data_dir,
                    &self.conversation_id,
                    &message_id,
                    emoji,
                );
            }
            if failure {
                send_reaction(
                    &self.base_url,
                    &self.data_dir,
                    &self.conversation_id,
                    &message_id,
                    ReactionEmoji::Warning,
                );
            }
        }
    }
}

impl ToolCallSnapshot {
    fn from_tool_call(tool_call: &acp::ToolCall) -> Self {
        Self {
            title: Some(tool_call.title.clone()),
            raw_input: tool_call.raw_input.clone(),
            raw_output: tool_call.raw_output.clone(),
            meta: tool_call.meta.clone(),
        }
    }

    fn apply_update(&mut self, update: &acp::ToolCallUpdate) {
        if let Some(title) = &update.fields.title {
            self.title = Some(title.clone());
        }
        if let Some(raw_input) = &update.fields.raw_input {
            self.raw_input = Some(raw_input.clone());
        }
        if let Some(raw_output) = &update.fields.raw_output {
            self.raw_output = Some(raw_output.clone());
        }
        if let Some(meta) = &update.meta {
            self.meta = Some(meta.clone());
        }
    }
}

fn reaction_for_tool_start(snapshot: &ToolCallSnapshot) -> Option<ReactionEmoji> {
    if infer_subagent_tool(snapshot) {
        Some(ReactionEmoji::Subagent)
    } else if snapshot.title.is_some() || snapshot.raw_input.is_some() || snapshot.meta.is_some() {
        Some(ReactionEmoji::Tool)
    } else {
        None
    }
}

fn infer_subagent_tool(snapshot: &ToolCallSnapshot) -> bool {
    if snapshot.title.as_deref().is_some_and(matches_subagent_name) {
        return true;
    }
    if snapshot
        .meta
        .as_ref()
        .is_some_and(meta_contains_subagent_tool_name)
    {
        return true;
    }
    if snapshot
        .raw_input
        .as_ref()
        .is_some_and(value_contains_subagent_tool_name)
    {
        return true;
    }
    false
}

fn meta_contains_subagent_tool_name(meta: &serde_json::Map<String, serde_json::Value>) -> bool {
    if let Some(tool_name) = meta
        .get("claudeCode")
        .and_then(|value| value.get("toolName"))
        .and_then(serde_json::Value::as_str)
    {
        return matches_subagent_name(tool_name);
    }
    meta.values().any(value_contains_subagent_tool_name)
}

fn value_contains_subagent_tool_name(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => matches_subagent_name(s),
        serde_json::Value::Array(values) => values.iter().any(value_contains_subagent_tool_name),
        serde_json::Value::Object(map) => {
            if let Some(tool_name) = map.get("toolName").and_then(serde_json::Value::as_str)
                && matches_subagent_name(tool_name)
            {
                return true;
            }
            map.values().any(value_contains_subagent_tool_name)
        }
        _ => false,
    }
}

fn matches_subagent_name(value: &str) -> bool {
    matches!(value.trim(), "Agent" | "Task")
}

fn tool_status_label(status: acp::ToolCallStatus) -> &'static str {
    match status {
        acp::ToolCallStatus::Pending => "pending",
        acp::ToolCallStatus::InProgress => "in_progress",
        acp::ToolCallStatus::Completed => "completed",
        acp::ToolCallStatus::Failed => "failed",
        _ => "other",
    }
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
        let session_id = session_id_owned(&args.session_id);
        match args.update {
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk { content, .. }) => {
                let text = match content {
                    acp::ContentBlock::Text(text) => text.text,
                    acp::ContentBlock::Image(_) => "<image>".to_owned(),
                    acp::ContentBlock::Audio(_) => "<audio>".to_owned(),
                    acp::ContentBlock::ResourceLink(link) => link.uri,
                    acp::ContentBlock::Resource(_) => "<resource>".to_owned(),
                    _ => "<unsupported>".to_owned(),
                };
                let mut stream_parts = Vec::new();
                if let Ok(mut state) = self.state.lock() {
                    let runtime = state.sessions.entry(session_id).or_default();
                    if runtime.active_turn.is_none() {
                        return Ok(());
                    }
                    runtime.chunks.push(text.clone());
                    runtime.full_text.push_str(&text);

                    if self.reply_mode == ReplyMode::Stream {
                        runtime.stream_buffer.push_str(&text);
                        let (parts, remainder) =
                            extract_streamable_markdown_parts(&runtime.stream_buffer);
                        runtime.stream_buffer = remainder;
                        stream_parts = parts;
                    }
                }

                if self.reply_mode == ReplyMode::Stream {
                    for part in stream_parts {
                        if let Err(err) = send_reply_part(
                            &self.data_dir,
                            &self.conversation_id,
                            &part,
                            "xmtp_stream_sent",
                        )
                        .await
                        {
                            error!("ACP streamed reply send failed: {err:#}");
                            log_acp_event(
                                &self.data_dir,
                                &self.conversation_id,
                                serde_json::json!({
                                    "event": "error",
                                    "message": format!("ACP streamed reply send failed: {err:#}"),
                                }),
                            );
                            break;
                        }
                    }
                }
            }
            acp::SessionUpdate::ToolCall(tool_call) => {
                let flushed_text = if self.reply_mode == ReplyMode::Single {
                    if let Ok(mut state) = self.state.lock() {
                        let runtime = state.sessions.entry(session_id.clone()).or_default();
                        if runtime.active_turn.is_some() {
                            Some(std::mem::take(&mut runtime.chunks).join(""))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.handle_tool_call(&session_id, tool_call);
                if let Some(text) = flushed_text {
                    if !text.trim().is_empty() {
                        if let Err(err) = send_reply_part(
                            &self.data_dir,
                            &self.conversation_id,
                            &text,
                            "xmtp_sent",
                        )
                        .await
                        {
                            error!("ACP single flush send failed: {err:#}");
                        }
                    }
                }
            }
            acp::SessionUpdate::ToolCallUpdate(update) => {
                self.handle_tool_call_update(&session_id, update);
            }
            _ => {}
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

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    use super::{
        BridgeState, PersistedSessionEntry, ReactionEmoji, SessionRuntime, ToolCallSnapshot,
        begin_session_turn, format_agent_error_message, infer_subagent_tool,
        reaction_for_tool_start, remember_processed_message, resolve_resume_session_selector,
        split_markdown_reply, truncate_display,
    };

    #[test]
    fn truncate_display_handles_multibyte_text_without_panicking() {
        let input = "那么BDD看来是没有指导如何编排agents的，对吧，针对BDD这种需求，我们编排agents时候有什么优化点或者注意事项么";

        let truncated = truncate_display(input, 20);

        assert!(truncated.ends_with("..."));
        assert_eq!(truncated.chars().count(), 23);
    }

    #[test]
    fn truncate_display_replaces_newlines_before_truncating() {
        let truncated = truncate_display("hello\nworld", 7);

        assert_eq!(truncated, "hello w...");
    }

    #[test]
    fn format_agent_error_message_prefixes_and_truncates() {
        let err =
            anyhow!("Unknown skill: ce:review\nArgs from unknown skill: 当前xmtp-mobile项目的结构");

        let message = format_agent_error_message(&err);

        assert!(message.starts_with("ACP agent error: "));
        assert!(message.contains("Unknown skill: ce:review"));
        assert!(!message.contains('\n'));
    }

    #[test]
    fn reaction_mapping_marks_regular_tool_as_wrench() {
        let snapshot = ToolCallSnapshot {
            title: Some("Read src/main.rs".to_owned()),
            ..ToolCallSnapshot::default()
        };

        assert_eq!(
            reaction_for_tool_start(&snapshot),
            Some(ReactionEmoji::Tool)
        );
    }

    #[test]
    fn reaction_mapping_marks_agent_meta_as_robot() {
        let snapshot = ToolCallSnapshot {
            meta: Some(
                json!({
                    "claudeCode": {
                        "toolName": "Agent"
                    }
                })
                .as_object()
                .cloned()
                .expect("object"),
            ),
            ..ToolCallSnapshot::default()
        };

        assert!(infer_subagent_tool(&snapshot));
        assert_eq!(
            reaction_for_tool_start(&snapshot),
            Some(ReactionEmoji::Subagent)
        );
    }

    #[test]
    fn reaction_mapping_marks_task_raw_input_as_robot() {
        let snapshot = ToolCallSnapshot {
            raw_input: Some(json!({
                "toolName": "Task"
            })),
            ..ToolCallSnapshot::default()
        };

        assert!(infer_subagent_tool(&snapshot));
        assert_eq!(
            reaction_for_tool_start(&snapshot),
            Some(ReactionEmoji::Subagent)
        );
    }

    #[test]
    fn split_markdown_reply_keeps_code_fence_together() {
        let reply = "# Title\n\n```rust\nfn main() {}\n```\n\nAfter";

        let parts = split_markdown_reply(reply);

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "# Title\n\n```rust\nfn main() {}\n```");
        assert_eq!(parts[1], "After");
    }

    #[test]
    fn split_markdown_reply_splits_long_markdown_into_multiple_messages() {
        let reply = format!("{}\n\n{}", "A".repeat(1200), "B".repeat(1200),);

        let parts = split_markdown_reply(&reply);

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "A".repeat(1200));
        assert_eq!(parts[1], "B".repeat(1200));
    }

    #[test]
    fn remember_processed_message_skips_duplicates_case_insensitively() {
        let state = Arc::new(Mutex::new(BridgeState::default()));

        assert!(remember_processed_message(&state, "Msg-1"));
        assert!(!remember_processed_message(&state, "msg-1"));
        assert!(remember_processed_message(&state, "msg-2"));
    }

    #[test]
    fn persisted_session_entry_legacy_upgrades_to_recent_list() {
        let entry = PersistedSessionEntry::Legacy("session-1".to_owned());

        assert_eq!(entry.into_recent(), vec!["session-1"]);
    }

    #[test]
    fn resolve_resume_session_selector_supports_latest_index_and_exact_id() {
        let sessions = vec![
            "session-latest".to_owned(),
            "session-prev".to_owned(),
            "session-old".to_owned(),
        ];

        assert_eq!(
            resolve_resume_session_selector(&sessions, "latest").unwrap(),
            Some("session-latest".to_owned())
        );
        assert_eq!(
            resolve_resume_session_selector(&sessions, "2").unwrap(),
            Some("session-prev".to_owned())
        );
        assert_eq!(
            resolve_resume_session_selector(&sessions, "session-old").unwrap(),
            Some("session-old".to_owned())
        );
    }

    #[test]
    fn resolve_resume_session_selector_rejects_invalid_index() {
        let sessions = vec!["session-latest".to_owned()];

        let err = resolve_resume_session_selector(&sessions, "0").unwrap_err();

        assert!(err.to_string().contains("must start at 1"));
    }

    #[test]
    fn begin_session_turn_clears_stream_buffer() {
        let state = Arc::new(Mutex::new(BridgeState::default()));
        if let Ok(mut bridge_state) = state.lock() {
            bridge_state.sessions.insert(
                "session-1".to_owned(),
                SessionRuntime {
                    stream_buffer: "stale".to_owned(),
                    ..SessionRuntime::default()
                },
            );
        }

        begin_session_turn(&state, "session-1", "message-1");

        let stream_buffer = state
            .lock()
            .ok()
            .and_then(|bridge_state| {
                bridge_state
                    .sessions
                    .get("session-1")
                    .map(|runtime| runtime.stream_buffer.clone())
            })
            .unwrap_or_default();

        assert!(stream_buffer.is_empty());
    }
}
