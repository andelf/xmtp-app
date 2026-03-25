use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::Context;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::Response;
use tempfile::TempDir;
use xmtp_ipc::{ActionResponse, ApiErrorBody, DaemonEventEnvelope, StatusResponse};

struct DaemonProcess {
    _temp: TempDir,
    data_dir: PathBuf,
    child: Child,
    base_url: String,
}

impl DaemonProcess {
    fn start() -> anyhow::Result<Self> {
        let temp = tempfile::tempdir().context("create tempdir")?;
        let data_dir = temp.path().join("data");
        run_cli(&data_dir, &["init"])?;

        let mut child = Command::new(cli_bin_path())
            .arg("--data-dir")
            .arg(&data_dir)
            .args(["daemon", "run"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn daemon child")?;

        let addr_path = data_dir.join("daemon.addr");
        let deadline = Instant::now() + Duration::from_secs(10);
        let base_url = loop {
            if let Some(status) = child.try_wait().context("poll daemon child")? {
                anyhow::bail!("daemon exited early with status {status}");
            }
            if let Ok(addr) = fs::read_to_string(&addr_path) {
                break format!("http://{}", addr.trim());
            }
            if Instant::now() >= deadline {
                anyhow::bail!("timed out waiting for daemon addr file");
            }
            std::thread::sleep(Duration::from_millis(100));
        };

        Ok(Self {
            _temp: temp,
            data_dir,
            child,
            base_url,
        })
    }

    async fn login_dev(&self) -> anyhow::Result<StatusResponse> {
        let client = reqwest::Client::new();
        post_json_with_retry(
            &client,
            format!("{}/v1/login", self.base_url),
            &serde_json::json!({
                "env": "dev",
                "api_url": null
            }),
            "login status",
        )
        .await
    }
}

async fn post_json_with_retry<T, B>(
    client: &reqwest::Client,
    url: String,
    body: &B,
    context_message: &str,
) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
    B: serde::Serialize + ?Sized,
{
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut last_error = None;
    while Instant::now() < deadline {
        match client.post(&url).json(body).send().await {
            Ok(response) => match ensure_success(response, context_message).await {
                Ok(response) => {
                    return response
                        .json()
                        .await
                        .with_context(|| format!("decode {}", context_message));
                }
                Err(err) => last_error = Some(err),
            },
            Err(err) => last_error = Some(anyhow::Error::new(err).context(context_message.to_owned())),
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("{context_message} did not succeed before timeout")))
}

async fn ensure_success(response: Response, context_message: &str) -> anyhow::Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    if let Ok(parsed) = serde_json::from_str::<ApiErrorBody>(&body) {
        anyhow::bail!("{}: {}", parsed.error.code, parsed.error.message);
    }
    anyhow::bail!("{context_message}: {}", body);
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _ = Command::new(cli_bin_path())
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(["daemon", "stop"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn cli_bin_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    workspace_root
        .join("target")
        .join("debug")
        .join(format!("xmtp-cli{}", std::env::consts::EXE_SUFFIX))
}

fn run_cli(data_dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(cli_bin_path())
        .arg("--data-dir")
        .arg(data_dir)
        .args(args)
        .status()
        .with_context(|| format!("run xmtp-cli {}", args.join(" ")))?;
    if !status.success() {
        anyhow::bail!("xmtp-cli {} failed with status {status}", args.join(" "));
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_endpoints_smoke_return_expected_json_shapes() -> anyhow::Result<()> {
    let daemon = DaemonProcess::start()?;
    let client = reqwest::Client::new();

    let status_json: serde_json::Value = client
        .get(format!("{}/v1/status", daemon.base_url))
        .send()
        .await
        .context("send status request")?
        .error_for_status()
        .context("status response status")?
        .json()
        .await
        .context("decode status json")?;
    assert!(status_json.get("daemon_state").is_some());

    daemon.login_dev().await?;

    let conversations_json: serde_json::Value = client
        .get(format!("{}/v1/conversations", daemon.base_url))
        .send()
        .await
        .context("send conversations request")?
        .error_for_status()
        .context("conversations response status")?
        .json()
        .await
        .context("decode conversations json")?;
    assert!(conversations_json.get("items").is_some());
    assert!(conversations_json["items"].is_array());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_events_sse_stream_yields_deserializable_envelope() -> anyhow::Result<()> {
    let daemon = DaemonProcess::start()?;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/v1/events", daemon.base_url))
        .send()
        .await
        .context("open app events stream")?
        .error_for_status()
        .context("app events status")?;
    let mut stream = response.bytes_stream().eventsource();

    let envelope = tokio::time::timeout(Duration::from_secs(10), async move {
        while let Some(event) = stream.next().await {
            let event = event.context("read sse event")?;
            let envelope: DaemonEventEnvelope =
                serde_json::from_str(&event.data).context("decode daemon event envelope")?;
            return Ok::<DaemonEventEnvelope, anyhow::Error>(envelope);
        }
        anyhow::bail!("app events stream ended before first event")
    })
    .await
    .context("wait for first app event")??;

    assert!(!envelope.event_id.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_group_and_send_message_over_http() -> anyhow::Result<()> {
    let daemon = DaemonProcess::start()?;
    let login = match daemon.login_dev().await {
        Ok(login) => login,
        Err(err) if is_rate_limited(&err) => {
            eprintln!("skipping create_group_and_send_message_over_http due to XMTP rate limit: {err:#}");
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    let client = reqwest::Client::new();
    let member = login
        .inbox_id
        .clone()
        .context("login response missing inbox_id")?;

    let created: ActionResponse = match post_json_with_retry(
        &client,
        format!("{}/v1/groups", daemon.base_url),
        &serde_json::json!({
            "name": "ci-http-group",
            "members": [member]
        }),
        "create group status",
    )
    .await {
        Ok(created) => created,
        Err(err) if is_rate_limited(&err) => {
            eprintln!("skipping create_group_and_send_message_over_http due to XMTP rate limit: {err:#}");
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    assert!(!created.conversation_id.is_empty());

    let sent: ActionResponse = match post_json_with_retry(
        &client,
        format!(
            "{}/v1/groups/{}/send",
            daemon.base_url, created.conversation_id
        ),
        &serde_json::json!({
            "message": "ci-http-group-send"
        }),
        "group send status",
    )
    .await {
        Ok(sent) => sent,
        Err(err) if is_rate_limited(&err) => {
            eprintln!("skipping create_group_and_send_message_over_http due to XMTP rate limit: {err:#}");
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    assert_eq!(sent.conversation_id, created.conversation_id);
    assert!(!sent.message_id.is_empty());

    Ok(())
}

fn is_rate_limited(err: &anyhow::Error) -> bool {
    err.to_string().to_ascii_lowercase().contains("rate_limit")
        || err
            .to_string()
            .to_ascii_lowercase()
            .contains("rate limit")
        || format!("{err:#}")
            .to_ascii_lowercase()
            .contains("resource has been exhausted")
}
