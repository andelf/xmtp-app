use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, anyhow};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use xmtp_ipc::{DaemonEventData, DaemonEventEnvelope, HistoryItem};

use crate::{daemon_base_url, daemon_send_conversation, http_client, http_get, wait_for_daemon_ready};

pub async fn run_acp(
    data_dir: PathBuf,
    conversation_id: String,
    command: Vec<String>,
) -> anyhow::Result<()> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow!("ACP command is required"))?;

    wait_for_daemon_ready(&data_dir, 4_000).await?;

    let status: xmtp_ipc::StatusResponse =
        http_get(&data_dir, "/v1/status").await.context("load daemon status")?;
    let self_inbox_id = status.inbox_id;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawn ACP subprocess: {}", program))?;

    let child_stdin = child
        .stdin
        .take()
        .context("take ACP subprocess stdin")?;
    let child_stdout = child
        .stdout
        .take()
        .context("take ACP subprocess stdout")?;

    let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
    let writer_task = tokio::spawn(async move {
        let mut stdin = child_stdin;
        while let Some(line) = stdin_rx.recv().await {
            stdin
                .write_all(line.as_bytes())
                .await
                .context("write ACP stdin")?;
            stdin.write_all(b"\n").await.context("write ACP newline")?;
            stdin.flush().await.context("flush ACP stdin")?;
        }
        Ok::<(), anyhow::Error>(())
    });

    let next_id = Arc::new(AtomicU64::new(1));
    let sse_task = tokio::spawn({
        let data_dir = data_dir.clone();
        let conversation_id = conversation_id.clone();
        let stdin_tx = stdin_tx.clone();
        let next_id = Arc::clone(&next_id);
        async move {
            let base_url = daemon_base_url(&data_dir)?;
            let url = format!("{base_url}/v1/conversations/{conversation_id}/events");
            let response = http_client()
                .get(url)
                .send()
                .await
                .context("open ACP history SSE stream")?
                .error_for_status()
                .context("ACP history SSE status")?;
            let mut stream = response.bytes_stream().eventsource();
            while let Some(event) = stream.next().await {
                let event = event.context("read ACP history SSE event")?;
                let envelope: DaemonEventEnvelope =
                    serde_json::from_str(&event.data).context("decode ACP SSE envelope")?;
                if let DaemonEventData::HistoryItem { item, .. } = envelope.payload {
                    if should_forward_item(&item, self_inbox_id.as_deref()) {
                        let request_id = next_id.fetch_add(1, Ordering::Relaxed);
                        let payload = json!({
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "method": "message",
                            "params": {
                                "conversation_id": conversation_id,
                                "message_id": item.message_id,
                                "sender_inbox_id": item.sender_inbox_id,
                                "content_kind": item.content_kind,
                                "content": item.content,
                            }
                        });
                        let line = serde_json::to_string(&payload).context("encode ACP request")?;
                        stdin_tx
                            .send(line)
                            .map_err(|_| anyhow!("ACP stdin task is closed"))?;
                    }
                }
            }
            Ok::<(), anyhow::Error>(())
        }
    });

    let stdout_task = tokio::spawn({
        let data_dir = data_dir.clone();
        let conversation_id = conversation_id.clone();
        async move {
            let mut lines = BufReader::new(child_stdout).lines();
            while let Some(line) = lines.next_line().await.context("read ACP stdout")? {
                if let Some(reply) = extract_reply_text(&line) {
                    let _ = daemon_send_conversation(
                        &data_dir,
                        &conversation_id,
                        &reply,
                        Some("markdown"),
                    )
                    .await
                    .with_context(|| format!("send ACP reply back to conversation {conversation_id}"))?;
                }
            }
            Ok::<(), anyhow::Error>(())
        }
    });

    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "transport": "stdio",
            "conversation_id": conversation_id,
        }
    });
    stdin_tx
        .send(serde_json::to_string(&initialize).context("encode ACP initialize")?)
        .map_err(|_| anyhow!("ACP stdin task is closed"))?;

    tokio::select! {
        result = child.wait() => {
            let status = result.context("wait for ACP subprocess")?;
            drop(stdin_tx);
            let _ = writer_task.await;
            let _ = sse_task.abort();
            let _ = stdout_task.abort();
            anyhow::bail!("ACP subprocess exited: {status}");
        }
        signal = tokio::signal::ctrl_c() => {
            signal.context("wait for ctrl-c")?;
            drop(stdin_tx);
            let _ = child.start_kill();
            let _ = child.wait().await;
            let _ = writer_task.await;
            let _ = sse_task.abort();
            let _ = stdout_task.abort();
        }
    }

    Ok(())
}

fn should_forward_item(item: &HistoryItem, self_inbox_id: Option<&str>) -> bool {
    if self_inbox_id == Some(item.sender_inbox_id.as_str()) {
        return false;
    }
    !matches!(
        item.content_kind.as_str(),
        "reaction" | "read_receipt" | "unknown"
    )
}

fn extract_reply_text(line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    extract_reply_from_value(&value).or_else(|| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn extract_reply_from_value(value: &Value) -> Option<String> {
    for candidate in [
        value.pointer("/result"),
        value.pointer("/result/content"),
        value.pointer("/result/message"),
        value.pointer("/result/text"),
        value.pointer("/params/content"),
        value.pointer("/params/message"),
        value.pointer("/params/text"),
        value.pointer("/content"),
        value.pointer("/message"),
        value.pointer("/text"),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(text) = candidate.as_str() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
    }
    None
}
