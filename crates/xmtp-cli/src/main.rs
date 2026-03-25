use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::os::unix::process::CommandExt;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use xmtp_config::{AppConfig, save_config};
use xmtp_daemon::{
    HistoryEntry, history_with_kind as load_history_direct, pid_path, serve, socket_path,
    watch_history_with_kind as watch_history_stream,
};
use xmtp_core::{ConnectionState, DaemonState, StateSnapshot, SyncPhase, SyncState};
use xmtp_ipc::{
    ActionResponse, ConversationItem, ConversationListResponse, DaemonRequest, DaemonResponse,
    DaemonResponseData, ConversationInfoResponse, GroupInfoResponse, GroupMemberItem,
    GroupMembersResponse, IpcEnvelope, MessageInfoResponse, SendDmResponse,
    StatusResponse,
};
use xmtp_logging::{
    daemon_events_log_path, daemon_stderr_log_path, daemon_stdout_log_path, ensure_logs_dir,
};
use xmtp_store::{load_state, save_state};

#[derive(Debug, Parser)]
#[command(name = "xmtp-cli")]
struct Cli {
    #[arg(long, global = true, default_value = "./data")]
    data_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init,
    Login {
        #[arg(long, default_value = "dev")]
        env: String,
        #[arg(long)]
        api_url: Option<String>,
    },
    Doctor,
    Status,
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    Logs {
        #[arg(long, default_value = "events")]
        kind: String,
        #[arg(long)]
        follow: bool,
    },
    ListConversations {
        #[arg(long, conflicts_with = "dm")]
        group: bool,
        #[arg(long, conflicts_with = "group")]
        dm: bool,
    },
    #[command(name = "direct-message", visible_alias = "dm")]
    DirectMessage {
        recipient: String,
        message: String,
    },
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    Reply {
        message_id: String,
        message: String,
    },
    React {
        message_id: String,
        emoji: String,
    },
    Unreact {
        message_id: String,
        emoji: String,
    },
    Leave {
        conversation_id: String,
    },
    Info {
        #[command(subcommand)]
        command: InfoCommand,
    },
    History {
        conversation_id: String,
        #[arg(long)]
        watch: bool,
        #[arg(long, conflicts_with = "dm")]
        group: bool,
        #[arg(long, conflicts_with = "group")]
        dm: bool,
    },
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    Start,
    Restart,
    Status,
    Stop,
    Run,
}

#[derive(Debug, Subcommand)]
enum GroupCommand {
    List,
    History {
        conversation_id: String,
        #[arg(long)]
        watch: bool,
    },
    Create {
        #[arg(long)]
        name: Option<String>,
        #[arg(long = "member", required = true)]
        members: Vec<String>,
    },
    Send {
        conversation_id: String,
        message: String,
    },
    Rename {
        conversation_id: String,
        name: String,
    },
    Add {
        conversation_id: String,
        #[arg(long = "member", required = true)]
        members: Vec<String>,
    },
    Remove {
        conversation_id: String,
        #[arg(long = "member", required = true)]
        members: Vec<String>,
    },
    Members {
        conversation_id: String,
    },
    Info {
        conversation_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum InfoCommand {
    Conversation {
        conversation_id: String,
    },
    Message {
        message_id: String,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let data_dir = cli.data_dir;

    match cli.command {
        Command::Init => init(data_dir),
        Command::Login { env, api_url } => login(data_dir, &env, api_url.as_deref()).await,
        Command::Doctor => doctor(data_dir).await,
        Command::Status => status(data_dir),
        Command::Daemon { command } => daemon(data_dir, command).await,
        Command::Logs { kind, follow } => logs(data_dir, &kind, follow).await,
        Command::ListConversations { group, dm } => {
            let kind = if group {
                Some("group")
            } else if dm {
                Some("dm")
            } else {
                None
            };
            list(data_dir, kind).await
        }
        Command::DirectMessage { recipient, message } => dm(data_dir, &recipient, &message).await,
        Command::Group { command } => group(data_dir, command).await,
        Command::Reply { message_id, message } => reply(data_dir, &message_id, &message).await,
        Command::React { message_id, emoji } => react(data_dir, &message_id, &emoji).await,
        Command::Unreact { message_id, emoji } => unreact(data_dir, &message_id, &emoji).await,
        Command::Leave { conversation_id } => leave(data_dir, &conversation_id).await,
        Command::Info { command } => info(data_dir, command).await,
        Command::History {
            conversation_id,
            watch,
            group,
            dm,
        } => {
            let kind = if group {
                Some("group")
            } else if dm {
                Some("dm")
            } else {
                None
            };
            history(data_dir, &conversation_id, watch, kind).await
        }
    }
}

fn init(data_dir: PathBuf) -> anyhow::Result<()> {
    std::fs::create_dir_all(&data_dir).context("create data dir")?;

    let config = AppConfig::for_data_dir(&data_dir);
    save_config(&data_dir.join("config.json"), &config)?;

    let state = StateSnapshot {
        schema_version: 1,
        daemon_state: DaemonState::Stopped,
        started_at_unix_ms: None,
        current_profile: Some("default".to_owned()),
        inbox_id: None,
        installation_id: None,
        connection_state: ConnectionState::Disconnected,
        sync_state: SyncState {
            phase: SyncPhase::Idle,
            last_cursor: None,
            last_successful_sync_unix_ms: None,
            pending_actions: 0,
        },
        recent_error: None,
    };
    save_state(&data_dir.join("state.json"), &state)?;

    Ok(())
}

fn status(data_dir: PathBuf) -> anyhow::Result<()> {
    let state = load_state(&data_dir.join("state.json"))?;
    println!("{}", render_status_row("daemon_state", &state.daemon_state.to_string()));
    println!(
        "{}",
        render_status_row("connection_state", &state.connection_state.to_string())
    );
    if let Some(inbox_id) = state.inbox_id {
        println!("{}", render_status_row("inbox_id", &short_id(&inbox_id)));
    }
    if let Some(installation_id) = state.installation_id {
        println!(
            "{}",
            render_status_row("installation_id", &short_id(&installation_id))
        );
    }
    Ok(())
}

async fn doctor(data_dir: PathBuf) -> anyhow::Result<()> {
    let config_path = data_dir.join("config.json");
    let state_path = data_dir.join("state.json");
    let signer_path = data_dir.join("signer.key");
    let socket = socket_path(&data_dir);
    let pid = pid_path(&data_dir);
    let events_log = daemon_events_log_path(&data_dir);
    let stdout_log = daemon_stdout_log_path(&data_dir);
    let stderr_log = daemon_stderr_log_path(&data_dir);

    println!("{}", render_status_row("data_dir", &data_dir.display().to_string()));
    println!(
        "{}",
        render_status_row("config_json", bool_label(config_path.exists()))
    );
    println!(
        "{}",
        render_status_row("state_json", bool_label(state_path.exists()))
    );
    println!(
        "{}",
        render_status_row("signer_key", bool_label(signer_path.exists()))
    );
    println!(
        "{}",
        render_status_row("daemon_socket", bool_label(socket.exists()))
    );
    println!(
        "{}",
        render_status_row("daemon_pid", bool_label(pid.exists()))
    );
    println!(
        "{}",
        render_status_row("events_log", bool_label(events_log.exists()))
    );
    println!(
        "{}",
        render_status_row("stdout_log", bool_label(stdout_log.exists()))
    );
    println!(
        "{}",
        render_status_row("stderr_log", bool_label(stderr_log.exists()))
    );

    match send_daemon_request_without_autostart(&data_dir, DaemonRequest::GetStatus).await {
        Ok(response) => {
            let status = expect_status(response)?;
            println!("{}", render_status_row("daemon_reachable", "yes"));
            println!(
                "{}",
                render_status_row("daemon_state", &status.daemon_state.to_string())
            );
            println!(
                "{}",
                render_status_row("connection_state", &status.connection_state.to_string())
            );
            if let Some(inbox_id) = status.inbox_id {
                println!("{}", render_status_row("inbox_id", &short_id(&inbox_id)));
            }
        }
        Err(_) => {
            println!("{}", render_status_row("daemon_reachable", "no"));
        }
    }

    Ok(())
}

async fn login(data_dir: PathBuf, env: &str, api_url: Option<&str>) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::Login {
            env: env.to_owned(),
            api_url: api_url.map(str::to_owned),
        },
    )
    .await?;
    print_status_response(expect_status(response)?)?;
    Ok(())
}

async fn list(data_dir: PathBuf, kind: Option<&str>) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::ListConversations {
            kind: kind.map(str::to_owned),
        },
    )
    .await?;
    let list = expect_conversation_list(response)?;
    println!("{}", conversation_list_header());
    for conversation in list.items {
        println!("{}", render_conversation_row(&conversation));
    }
    Ok(())
}

async fn dm(data_dir: PathBuf, recipient: &str, message: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::SendDm {
            recipient: recipient.to_owned(),
            message: message.to_owned(),
        },
    )
    .await?;
    let result = expect_send_dm(response)?;
    println!("{}", action_result_header());
    println!(
        "{}",
        render_action_result(
            "direct-message",
            &ActionResponse {
                conversation_id: result.conversation_id,
                message_id: result.message_id,
            }
        )
    );
    Ok(())
}

async fn group(data_dir: PathBuf, command: GroupCommand) -> anyhow::Result<()> {
    match command {
        GroupCommand::List => list(data_dir, Some("group")).await,
        GroupCommand::History {
            conversation_id,
            watch,
        } => history(data_dir, &conversation_id, watch, Some("group")).await,
        GroupCommand::Create { name, members } => group_create(data_dir, name, members).await,
        GroupCommand::Send {
            conversation_id,
            message,
        } => group_send(data_dir, &conversation_id, &message).await,
        GroupCommand::Rename {
            conversation_id,
            name,
        } => group_rename(data_dir, &conversation_id, &name).await,
        GroupCommand::Add {
            conversation_id,
            members,
        } => group_add(data_dir, &conversation_id, members).await,
        GroupCommand::Remove {
            conversation_id,
            members,
        } => group_remove(data_dir, &conversation_id, members).await,
        GroupCommand::Members { conversation_id } => group_members(data_dir, &conversation_id).await,
        GroupCommand::Info { conversation_id } => group_info(data_dir, &conversation_id).await,
    }
}

async fn group_create(data_dir: PathBuf, name: Option<String>, members: Vec<String>) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::CreateGroup { name, members },
    )
    .await?;
    let result = expect_action(response, "create_group")?;
    print_action_result("group-create", &result);
    Ok(())
}

async fn group_send(data_dir: PathBuf, conversation_id: &str, message: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::SendGroup {
            conversation_id: conversation_id.to_owned(),
            message: message.to_owned(),
        },
    )
    .await?;
    let result = expect_action(response, "send_group")?;
    print_action_result("group-send", &result);
    Ok(())
}

async fn group_members(data_dir: PathBuf, conversation_id: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::GroupMembers {
            conversation_id: conversation_id.to_owned(),
        },
    )
    .await?;
    let members = expect_group_members(response)?;
    println!("{}", group_members_header());
    for member in members.items {
        println!("{}", render_group_member_row(&member));
    }
    Ok(())
}

async fn group_rename(data_dir: PathBuf, conversation_id: &str, name: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::RenameGroup {
            conversation_id: conversation_id.to_owned(),
            name: name.to_owned(),
        },
    )
    .await?;
    let result = expect_action(response, "rename_group")?;
    print_action_result("group-rename", &result);
    Ok(())
}

async fn group_add(data_dir: PathBuf, conversation_id: &str, members: Vec<String>) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::AddGroupMembers {
            conversation_id: conversation_id.to_owned(),
            members,
        },
    )
    .await?;
    let result = expect_action(response, "add_group_members")?;
    print_action_result("group-add", &result);
    Ok(())
}

async fn group_remove(
    data_dir: PathBuf,
    conversation_id: &str,
    members: Vec<String>,
) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::RemoveGroupMembers {
            conversation_id: conversation_id.to_owned(),
            members,
        },
    )
    .await?;
    let result = expect_action(response, "remove_group_members")?;
    print_action_result("group-remove", &result);
    Ok(())
}

async fn group_info(data_dir: PathBuf, conversation_id: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::GroupInfo {
            conversation_id: conversation_id.to_owned(),
        },
    )
    .await?;
    let info = expect_group_info(response)?;
    println!("{}", group_info_header());
    println!("{}", render_group_info_row(&info));
    Ok(())
}

async fn history(
    data_dir: PathBuf,
    conversation_id: &str,
    watch: bool,
    kind: Option<&str>,
) -> anyhow::Result<()> {
    if watch {
        print_history_direct(&data_dir, conversation_id, kind)?;
        watch_history(data_dir, conversation_id, kind)?;
        return Ok(());
    }
    println!("{}", history_header());
    for entry in load_history_direct(&data_dir, conversation_id, kind)? {
        println!("{}", render_history_entry(&entry));
    }
    Ok(())
}

fn print_history_direct(
    data_dir: &PathBuf,
    conversation_id: &str,
    kind: Option<&str>,
) -> anyhow::Result<()> {
    let entries = load_history_direct(data_dir, conversation_id, kind)?;
    println!("{}", history_header());
    for entry in entries {
        println!("{}", render_history_entry(&entry));
    }
    Ok(())
}

fn watch_history(data_dir: PathBuf, conversation_id: &str, kind: Option<&str>) -> anyhow::Result<()> {
    watch_history_stream(&data_dir, conversation_id, kind, |item| {
        println!("{}", render_history_line(&item));
    })
}

async fn reply(data_dir: PathBuf, message_id: &str, message: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::Reply {
            message_id: message_id.to_owned(),
            message: message.to_owned(),
        },
    )
    .await?;
    let result = expect_action(response, "reply")?;
    print_action_result("reply", &result);
    Ok(())
}

async fn react(data_dir: PathBuf, message_id: &str, emoji: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::React {
            message_id: message_id.to_owned(),
            emoji: emoji.to_owned(),
        },
    )
    .await?;
    let result = expect_action(response, "react")?;
    print_action_result("react", &result);
    Ok(())
}

async fn unreact(data_dir: PathBuf, message_id: &str, emoji: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::Unreact {
            message_id: message_id.to_owned(),
            emoji: emoji.to_owned(),
        },
    )
    .await?;
    let result = expect_action(response, "unreact")?;
    print_action_result("unreact", &result);
    Ok(())
}

async fn leave(data_dir: PathBuf, conversation_id: &str) -> anyhow::Result<()> {
    let response = send_daemon_request(
        &data_dir,
        DaemonRequest::LeaveConversation {
            conversation_id: conversation_id.to_owned(),
        },
    )
    .await?;
    let result = expect_action(response, "leave_conversation")?;
    print_action_result("leave", &result);
    Ok(())
}

async fn info(data_dir: PathBuf, command: InfoCommand) -> anyhow::Result<()> {
    match command {
        InfoCommand::Conversation { conversation_id } => {
            let response = send_daemon_request(
                &data_dir,
                DaemonRequest::ConversationInfo { conversation_id },
            )
            .await?;
            let info = expect_conversation_info(response)?;
            print_conversation_info(&info);
            Ok(())
        }
        InfoCommand::Message { message_id } => {
            let response =
                send_daemon_request(&data_dir, DaemonRequest::MessageInfo { message_id }).await?;
            let info = expect_message_info(response)?;
            print_message_info(&info);
            Ok(())
        }
    }
}

async fn daemon(data_dir: PathBuf, command: DaemonCommand) -> anyhow::Result<()> {
    match command {
        DaemonCommand::Start => daemon_start(data_dir).await,
        DaemonCommand::Restart => daemon_restart(data_dir).await,
        DaemonCommand::Status => daemon_status(data_dir).await,
        DaemonCommand::Stop => daemon_stop(data_dir).await,
        DaemonCommand::Run => serve(&data_dir).await,
    }
}

async fn daemon_start(data_dir: PathBuf) -> anyhow::Result<()> {
    std::fs::create_dir_all(&data_dir).context("create daemon dir")?;
    ensure_logs_dir(&data_dir)?;
    stop_existing_daemon(&data_dir).await?;
    let socket = socket_path(&data_dir);
    if socket.exists() {
        std::fs::remove_file(&socket).context("remove stale daemon socket")?;
    }
    let stdout_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(daemon_stdout_log_path(&data_dir))
        .context("open daemon stdout log")?;
    let stderr_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(daemon_stderr_log_path(&data_dir))
        .context("open daemon stderr log")?;
    let exe = std::env::current_exe().context("current exe")?;
    let mut command = ProcessCommand::new(exe);
    command
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("daemon")
        .arg("run")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().context("spawn daemon")?;

    wait_for_daemon_ready(&data_dir, 4_000).await?;
    println!("daemon_started");
    Ok(())
}

async fn daemon_restart(data_dir: PathBuf) -> anyhow::Result<()> {
    daemon_stop(data_dir.clone()).await?;
    daemon_start(data_dir).await?;
    println!("daemon_restarted");
    Ok(())
}

async fn daemon_status(data_dir: PathBuf) -> anyhow::Result<()> {
    let response = daemon_status_request(&data_dir).await?;
    print_status_response(expect_status(response)?)?;
    Ok(())
}

async fn daemon_stop(data_dir: PathBuf) -> anyhow::Result<()> {
    stop_existing_daemon(&data_dir).await?;
    println!("daemon_stopped");
    Ok(())
}

async fn send_daemon_request(data_dir: &PathBuf, request: DaemonRequest) -> anyhow::Result<DaemonResponse> {
    if !socket_path(data_dir).exists() {
        daemon_start(data_dir.clone()).await?;
    }
    send_daemon_request_without_autostart(data_dir, request).await
}

async fn daemon_status_request(data_dir: &PathBuf) -> anyhow::Result<DaemonResponse> {
    let mut last_error = None;
    for _ in 0..20 {
        match send_daemon_request_without_autostart(data_dir, DaemonRequest::GetStatus).await {
            Ok(response) => return Ok(response),
            Err(err) => {
                last_error = Some(err);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("daemon status unavailable")))
}

async fn wait_for_daemon_ready(data_dir: &PathBuf, timeout_ms: u64) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut last_error = None;
    loop {
        if socket_path(data_dir).exists() && pid_path(data_dir).exists() {
            match daemon_status_request(data_dir).await {
                Ok(_) => return Ok(()),
                Err(err) => last_error = Some(err),
            }
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("daemon socket did not appear")))
}

async fn logs(data_dir: PathBuf, kind: &str, follow: bool) -> anyhow::Result<()> {
    let path = match kind {
        "stdout" => daemon_stdout_log_path(&data_dir),
        "stderr" => daemon_stderr_log_path(&data_dir),
        _ => daemon_events_log_path(&data_dir),
    };
    if !path.exists() {
        anyhow::bail!("log file not found at {}", path.display());
    }

    let mut printed = 0usize;
    loop {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("read log file at {}", path.display()))?;
        let lines: Vec<&str> = content.lines().collect();
        for line in lines.iter().skip(printed) {
            println!("{line}");
        }
        printed = lines.len();
        if !follow {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(())
}

async fn send_daemon_request_without_autostart(
    data_dir: &PathBuf,
    request: DaemonRequest,
) -> anyhow::Result<DaemonResponse> {
    let socket = socket_path(data_dir);
    let mut stream = UnixStream::connect(&socket)
        .await
        .with_context(|| format!("connect daemon socket at {}", socket.display()))?;
    let envelope = IpcEnvelope {
        version: 1,
        request_id: "req-1".to_owned(),
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

async fn stop_existing_daemon(data_dir: &PathBuf) -> anyhow::Result<()> {
    let socket = socket_path(data_dir);
    if socket.exists() {
        let _ = send_daemon_request_without_autostart(data_dir, DaemonRequest::Shutdown).await;
    }

    if let Some(pid) = read_pid(data_dir)? {
        if process_is_alive(pid) {
            kill_process(pid)?;
            wait_for_process_exit(pid)?;
        }
        let pid_file = pid_path(data_dir);
        if pid_file.exists() {
            std::fs::remove_file(pid_file).context("remove daemon pid file")?;
        }
    }

    if socket.exists() {
        std::fs::remove_file(&socket).context("remove daemon socket")?;
    }

    Ok(())
}

fn read_pid(data_dir: &PathBuf) -> anyhow::Result<Option<i32>> {
    let pid_file = pid_path(data_dir);
    if !pid_file.exists() {
        return Ok(None);
    }
    let pid = std::fs::read_to_string(&pid_file).context("read daemon pid file")?;
    Ok(Some(
        pid.trim()
            .parse::<i32>()
            .context("parse daemon pid file")?,
    ))
}

fn process_is_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

fn kill_process(pid: i32) -> anyhow::Result<()> {
    let result = unsafe { libc::kill(pid, libc::SIGTERM) };
    if result == 0 {
        Ok(())
    } else {
        anyhow::bail!("kill daemon process {pid} failed")
    }
}

fn wait_for_process_exit(pid: i32) -> anyhow::Result<()> {
    for _ in 0..30 {
        if !process_is_alive(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("daemon process {pid} did not exit")
}

fn expect_status(response: DaemonResponse) -> anyhow::Result<StatusResponse> {
    match response.result.context("missing daemon result")? {
        DaemonResponseData::Status(status) => Ok(status),
        _ => anyhow::bail!("unexpected daemon response type"),
    }
}

fn expect_conversation_list(response: DaemonResponse) -> anyhow::Result<ConversationListResponse> {
    match response.result.context("missing daemon result")? {
        DaemonResponseData::ConversationList(list) => Ok(list),
        _ => anyhow::bail!("unexpected daemon response type"),
    }
}

fn expect_group_members(response: DaemonResponse) -> anyhow::Result<GroupMembersResponse> {
    match response.result.context("missing daemon result")? {
        DaemonResponseData::GroupMembers(result) => Ok(result),
        _ => anyhow::bail!("unexpected daemon response type"),
    }
}

fn expect_group_info(response: DaemonResponse) -> anyhow::Result<GroupInfoResponse> {
    match response.result.context("missing daemon result")? {
        DaemonResponseData::GroupInfo(result) => Ok(result),
        _ => anyhow::bail!("unexpected daemon response type"),
    }
}

fn expect_conversation_info(response: DaemonResponse) -> anyhow::Result<ConversationInfoResponse> {
    match response.result.context("missing daemon result")? {
        DaemonResponseData::ConversationInfo(result) => Ok(result),
        _ => anyhow::bail!("unexpected daemon response type"),
    }
}

fn expect_message_info(response: DaemonResponse) -> anyhow::Result<MessageInfoResponse> {
    match response.result.context("missing daemon result")? {
        DaemonResponseData::MessageInfo(result) => Ok(result),
        _ => anyhow::bail!("unexpected daemon response type"),
    }
}

fn expect_send_dm(response: DaemonResponse) -> anyhow::Result<SendDmResponse> {
    match response.result.context("missing daemon result")? {
        DaemonResponseData::SendDm(result) => Ok(result),
        _ => anyhow::bail!("unexpected daemon response type"),
    }
}

fn expect_action(response: DaemonResponse, action: &str) -> anyhow::Result<ActionResponse> {
    match response.result.context("missing daemon result")? {
        DaemonResponseData::Reply(result) if action == "reply" => Ok(result),
        DaemonResponseData::React(result) if action == "react" => Ok(result),
        DaemonResponseData::Unreact(result) if action == "unreact" => Ok(result),
        DaemonResponseData::CreateGroup(result) if action == "create_group" => Ok(result),
        DaemonResponseData::SendGroup(result) if action == "send_group" => Ok(result),
        DaemonResponseData::RenameGroup(result) if action == "rename_group" => Ok(result),
        DaemonResponseData::AddGroupMembers(result) if action == "add_group_members" => Ok(result),
        DaemonResponseData::RemoveGroupMembers(result) if action == "remove_group_members" => {
            Ok(result)
        }
        DaemonResponseData::LeaveConversation(result) if action == "leave_conversation" => {
            Ok(result)
        }
        _ => anyhow::bail!("unexpected daemon response type"),
    }
}

fn print_status_response(status: StatusResponse) -> anyhow::Result<()> {
    println!(
        "{}",
        render_status_row("daemon_state", &status.daemon_state.to_string())
    );
    println!(
        "{}",
        render_status_row("connection_state", &status.connection_state.to_string())
    );
    if let Some(inbox_id) = status.inbox_id {
        println!("{}", render_status_row("inbox_id", &short_id(&inbox_id)));
    }
    if let Some(installation_id) = status.installation_id {
        println!(
            "{}",
            render_status_row("installation_id", &short_id(&installation_id))
        );
    }
    Ok(())
}

fn short_id(value: &str) -> String {
    if value.starts_with("0x") && value.len() > 10 {
        return format!("{}....{}", &value[..6], &value[value.len() - 4..]);
    }
    if value.len() <= 8 {
        return value.to_owned();
    }
    format!("{}....{}", &value[..4], &value[value.len() - 4..])
}

fn format_sent_at(sent_at_ns: i64) -> String {
    if sent_at_ns <= 0 {
        return "-".to_owned();
    }
    let secs = sent_at_ns / 1_000_000_000;
    match chrono::DateTime::from_timestamp(secs, 0) {
        Some(value) => value.format("%m-%d %H:%M").to_string(),
        None => "-".to_owned(),
    }
}

fn print_action_result(action: &str, result: &ActionResponse) {
    println!("{}", action_result_header());
    println!("{}", render_action_result(action, result));
}

fn render_history_line(entry: &xmtp_ipc::HistoryItem) -> String {
    let mut content = entry.content.clone();
    let mut suffix = Vec::new();
    if entry.reply_count > 0 {
        suffix.push(format!("replies:{}", entry.reply_count));
    }
    if entry.reaction_count > 0 {
        suffix.push(format!("reactions:{}", entry.reaction_count));
    }
    if !suffix.is_empty() {
        content.push_str(" [");
        content.push_str(&suffix.join(" "));
        content.push(']');
    }
    format!(
        "{:<16} {:<20} {:<20} {}",
        format_sent_at(entry.sent_at_ns),
        short_id(&entry.message_id),
        short_id(&entry.sender_inbox_id),
        content
    )
}

fn render_history_entry(entry: &HistoryEntry) -> String {
    let mut content = entry.content.clone();
    let mut suffix = Vec::new();
    if entry.reply_count > 0 {
        suffix.push(format!("replies:{}", entry.reply_count));
    }
    if entry.reaction_count > 0 {
        suffix.push(format!("reactions:{}", entry.reaction_count));
    }
    if !suffix.is_empty() {
        content.push_str(" [");
        content.push_str(&suffix.join(" "));
        content.push(']');
    }
    format!(
        "{:<16} {:<20} {:<20} {}",
        format_sent_at(entry.sent_at_ns),
        short_id(&entry.message_id),
        short_id(&entry.sender_inbox_id),
        content
    )
}

fn history_header() -> String {
    format!("{:<16} {:<20} {:<20} {}", "time", "message_id", "sender", "content")
}

fn conversation_list_header() -> String {
    format!("{:<20} {:<12} {}", "conversation_id", "type", "name")
}

fn render_conversation_row(conversation: &ConversationItem) -> String {
    format!(
        "{:<20} {:<12} {}",
        short_id(&conversation.id),
        conversation.kind,
        conversation.name.clone().unwrap_or_default()
    )
}

fn group_members_header() -> String {
    format!(
        "{:<20} {:<12} {:<12} {:<6} {}",
        "inbox_id", "permission", "consent", "inst", "accounts"
    )
}

fn render_group_member_row(member: &GroupMemberItem) -> String {
    format!(
        "{:<20} {:<12} {:<12} {:<6} {}",
        short_id(&member.inbox_id),
        member.permission_level,
        member.consent_state,
        member.installation_count,
        member.account_identifiers.join(",")
    )
}

fn group_info_header() -> String {
    format!(
        "{:<20} {:<12} {:<12} {:<20} {}",
        "conversation_id", "type", "members", "permission", "name"
    )
}

fn render_group_info_row(info: &GroupInfoResponse) -> String {
    format!(
        "{:<20} {:<12} {:<12} {:<20} {}",
        short_id(&info.conversation_id),
        info.conversation_type,
        info.member_count,
        info.permission_preset,
        info.name.clone().unwrap_or_default()
    )
}

fn action_result_header() -> String {
    format!("{:<16} {:<20} {}", "action", "conversation_id", "message_id")
}

fn render_action_result(action: &str, result: &ActionResponse) -> String {
    let message_id = if result.message_id.is_empty() {
        "-".to_owned()
    } else {
        short_id(&result.message_id)
    };
    format!(
        "{:<16} {:<20} {}",
        action,
        short_id(&result.conversation_id),
        message_id
    )
}

fn render_status_row(label: &str, value: &str) -> String {
    format!("{:<18} {}", label, value)
}

fn bool_label(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn print_conversation_info(info: &ConversationInfoResponse) {
    println!("{}", render_status_row("conversation_id", &short_id(&info.conversation_id)));
    println!("{}", render_status_row("type", &info.conversation_type));
    println!(
        "{}",
        render_status_row("created_at", &format_sent_at(info.created_at_ns))
    );
    println!("{}", render_status_row("active", &info.is_active.to_string()));
    println!(
        "{}",
        render_status_row("membership", &info.membership_state)
    );
    println!("{}", render_status_row("members", &info.member_count.to_string()));
    println!(
        "{}",
        render_status_row("messages", &info.message_count.to_string())
    );
    if let Some(name) = &info.name {
        println!("{}", render_status_row("name", name));
    }
    if let Some(peer) = &info.dm_peer_inbox_id {
        println!("{}", render_status_row("dm_peer", &short_id(peer)));
    }
}

fn print_message_info(info: &MessageInfoResponse) {
    println!("{}", render_status_row("message_id", &short_id(&info.message_id)));
    println!(
        "{}",
        render_status_row("conversation_id", &short_id(&info.conversation_id))
    );
    println!("{}", render_status_row("sender", &short_id(&info.sender_inbox_id)));
    println!(
        "{}",
        render_status_row("sent_at", &format_sent_at(info.sent_at_ns))
    );
    println!(
        "{}",
        render_status_row("delivery", &info.delivery_status)
    );
    println!(
        "{}",
        render_status_row("replies", &info.reply_count.to_string())
    );
    println!(
        "{}",
        render_status_row("reactions", &info.reaction_count.to_string())
    );
    if let Some(content_type) = &info.content_type {
        println!("{}", render_status_row("content_type", content_type));
    }
    println!("{}", render_status_row("content", &info.content_summary));
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, Command, DaemonCommand, GroupCommand, InfoCommand, action_result_header,
        conversation_list_header, format_sent_at, group_info_header, group_members_header, history_header,
        render_action_result, render_conversation_row, render_group_info_row,
        render_group_member_row, render_history_line, render_status_row, short_id,
    };
    use clap::Parser;
    use xmtp_ipc::{
        ActionResponse, ConversationItem, GroupInfoResponse, GroupMemberItem, HistoryItem,
    };

    #[test]
    fn short_id_truncates_long_values_in_the_middle() {
        let value = "1234567890abcdefghijklmnopqrstuvwxyz";
        assert_eq!(short_id(value), "1234....wxyz");
    }

    #[test]
    fn short_id_preserves_hex_address_shape() {
        let value = "0x1234567890abcdef";
        assert_eq!(short_id(value), "0x1234....cdef");
    }

    #[test]
    fn format_sent_at_formats_known_timestamp() {
        assert_eq!(format_sent_at(1_710_000_000_000_000_000), "03-09 16:00");
    }

    #[test]
    fn render_history_line_appends_reply_and_reaction_counts() {
        let line = render_history_line(&HistoryItem {
            message_id: "1234567890abcdefghijklmnopqrstuvwxyz".to_owned(),
            sender_inbox_id: "abcdef0123456789abcdef0123456789".to_owned(),
            sent_at_ns: 1_710_000_000_000_000_000,
            content_kind: "text".to_owned(),
            content: "hello".to_owned(),
            reply_count: 2,
            reaction_count: 3,
            reply_target_message_id: None,
            reaction_target_message_id: None,
            reaction_emoji: None,
            reaction_action: None,
            attached_reactions: Vec::new(),
        });

        assert_eq!(
            line,
            format!(
                "{:<16} {:<20} {:<20} {}",
                "03-09 16:00",
                "1234....wxyz",
                "abcd....6789",
                "hello [replies:2 reactions:3]"
            )
        );
    }

    #[test]
    fn history_header_uses_expected_columns() {
        assert_eq!(
            history_header(),
            format!(
                "{:<16} {:<20} {:<20} {}",
                "time", "message_id", "sender", "content"
            )
        );
    }

    #[test]
    fn conversation_list_header_uses_expected_columns() {
        assert_eq!(
            conversation_list_header(),
            format!(
                "{:<20} {:<12} {}",
                "conversation_id", "type", "name"
            )
        );
    }

    #[test]
    fn direct_message_accepts_dm_alias() {
        let cli = Cli::parse_from([
            "xmtp-cli",
            "dm",
            "recipient-1",
            "hello",
        ]);

        match cli.command {
            Command::DirectMessage { recipient, message } => {
                assert_eq!(recipient, "recipient-1");
                assert_eq!(message, "hello");
            }
            _ => panic!("expected direct-message command"),
        }
    }

    #[test]
    fn doctor_parses_as_top_level_command() {
        let cli = Cli::parse_from(["xmtp-cli", "doctor"]);

        match cli.command {
            Command::Doctor => {}
            _ => panic!("expected doctor command"),
        }
    }

    #[test]
    fn logs_parses_kind_and_follow() {
        let cli = Cli::parse_from(["xmtp-cli", "logs", "--kind", "stderr", "--follow"]);

        match cli.command {
            Command::Logs { kind, follow } => {
                assert_eq!(kind, "stderr");
                assert!(follow);
            }
            _ => panic!("expected logs command"),
        }
    }

    #[test]
    fn group_create_parses_name_and_members() {
        let cli = Cli::parse_from([
            "xmtp-cli",
            "group",
            "create",
            "--name",
            "team",
            "--member",
            "member-1",
            "--member",
            "member-2",
        ]);

        match cli.command {
            Command::Group {
                command: GroupCommand::Create { name, members },
            } => {
                assert_eq!(name.as_deref(), Some("team"));
                assert_eq!(members, vec!["member-1", "member-2"]);
            }
            _ => panic!("expected group create command"),
        }
    }

    #[test]
    fn group_list_parses_as_subcommand() {
        let cli = Cli::parse_from(["xmtp-cli", "group", "list"]);

        match cli.command {
            Command::Group {
                command: GroupCommand::List,
            } => {}
            _ => panic!("expected group list command"),
        }
    }

    #[test]
    fn group_history_parses_conversation_id() {
        let cli = Cli::parse_from(["xmtp-cli", "group", "history", "Andelf"]);

        match cli.command {
            Command::Group {
                command: GroupCommand::History {
                    conversation_id,
                    watch,
                },
            } => {
                assert_eq!(conversation_id, "Andelf");
                assert!(!watch);
            }
            _ => panic!("expected group history command"),
        }
    }

    #[test]
    fn group_history_accepts_watch_flag() {
        let cli = Cli::parse_from(["xmtp-cli", "group", "history", "Andelf", "--watch"]);

        match cli.command {
            Command::Group {
                command: GroupCommand::History {
                    conversation_id,
                    watch,
                },
            } => {
                assert_eq!(conversation_id, "Andelf");
                assert!(watch);
            }
            _ => panic!("expected group history command"),
        }
    }

    #[test]
    fn group_send_parses_conversation_and_message() {
        let cli = Cli::parse_from(["xmtp-cli", "group", "send", "conv-1", "hello-group"]);

        match cli.command {
            Command::Group {
                command: GroupCommand::Send {
                    conversation_id,
                    message,
                },
            } => {
                assert_eq!(conversation_id, "conv-1");
                assert_eq!(message, "hello-group");
            }
            _ => panic!("expected group send command"),
        }
    }

    #[test]
    fn list_conversations_accepts_group_filter() {
        let cli = Cli::parse_from(["xmtp-cli", "list-conversations", "--group"]);

        match cli.command {
            Command::ListConversations { group, dm } => {
                assert!(group);
                assert!(!dm);
            }
            _ => panic!("expected list-conversations command"),
        }
    }

    #[test]
    fn group_members_parses_conversation_id() {
        let cli = Cli::parse_from(["xmtp-cli", "group", "members", "conv-3"]);

        match cli.command {
            Command::Group {
                command: GroupCommand::Members { conversation_id },
            } => {
                assert_eq!(conversation_id, "conv-3");
            }
            _ => panic!("expected group members command"),
        }
    }

    #[test]
    fn group_rename_parses_name() {
        let cli = Cli::parse_from(["xmtp-cli", "group", "rename", "conv-4", "renamed-group"]);

        match cli.command {
            Command::Group {
                command: GroupCommand::Rename {
                    conversation_id,
                    name,
                },
            } => {
                assert_eq!(conversation_id, "conv-4");
                assert_eq!(name, "renamed-group");
            }
            _ => panic!("expected group rename command"),
        }
    }

    #[test]
    fn group_add_parses_members() {
        let cli = Cli::parse_from([
            "xmtp-cli",
            "group",
            "add",
            "conv-5",
            "--member",
            "member-1",
            "--member",
            "member-2",
        ]);

        match cli.command {
            Command::Group {
                command: GroupCommand::Add {
                    conversation_id,
                    members,
                },
            } => {
                assert_eq!(conversation_id, "conv-5");
                assert_eq!(members, vec!["member-1", "member-2"]);
            }
            _ => panic!("expected group add command"),
        }
    }

    #[test]
    fn group_remove_parses_members() {
        let cli = Cli::parse_from([
            "xmtp-cli",
            "group",
            "remove",
            "conv-6",
            "--member",
            "member-9",
        ]);

        match cli.command {
            Command::Group {
                command: GroupCommand::Remove {
                    conversation_id,
                    members,
                },
            } => {
                assert_eq!(conversation_id, "conv-6");
                assert_eq!(members, vec!["member-9"]);
            }
            _ => panic!("expected group remove command"),
        }
    }

    #[test]
    fn group_info_parses_conversation_id() {
        let cli = Cli::parse_from(["xmtp-cli", "group", "info", "conv-7"]);

        match cli.command {
            Command::Group {
                command: GroupCommand::Info { conversation_id },
            } => {
                assert_eq!(conversation_id, "conv-7");
            }
            _ => panic!("expected group info command"),
        }
    }

    #[test]
    fn info_conversation_parses_id() {
        let cli = Cli::parse_from(["xmtp-cli", "info", "conversation", "conv-8"]);

        match cli.command {
            Command::Info {
                command: InfoCommand::Conversation { conversation_id },
            } => assert_eq!(conversation_id, "conv-8"),
            _ => panic!("expected info conversation command"),
        }
    }

    #[test]
    fn info_message_parses_id() {
        let cli = Cli::parse_from(["xmtp-cli", "info", "message", "msg-9"]);

        match cli.command {
            Command::Info {
                command: InfoCommand::Message { message_id },
            } => assert_eq!(message_id, "msg-9"),
            _ => panic!("expected info message command"),
        }
    }

    #[test]
    fn history_accepts_watch_flag() {
        let cli = Cli::parse_from(["xmtp-cli", "history", "conv-10", "--watch"]);

        match cli.command {
            Command::History {
                conversation_id,
                watch,
                group,
                dm,
            } => {
                assert_eq!(conversation_id, "conv-10");
                assert!(watch);
                assert!(!group);
                assert!(!dm);
            }
            _ => panic!("expected history command"),
        }
    }

    #[test]
    fn history_accepts_group_filter() {
        let cli = Cli::parse_from(["xmtp-cli", "history", "Andelf", "--watch", "--group"]);

        match cli.command {
            Command::History {
                conversation_id,
                watch,
                group,
                dm,
            } => {
                assert_eq!(conversation_id, "Andelf");
                assert!(watch);
                assert!(group);
                assert!(!dm);
            }
            _ => panic!("expected history command"),
        }
    }

    #[test]
    fn history_accepts_dm_filter() {
        let cli = Cli::parse_from(["xmtp-cli", "history", "conv-10", "--watch", "--dm"]);

        match cli.command {
            Command::History {
                conversation_id,
                watch,
                group,
                dm,
            } => {
                assert_eq!(conversation_id, "conv-10");
                assert!(watch);
                assert!(!group);
                assert!(dm);
            }
            _ => panic!("expected history command"),
        }
    }

    #[test]
    fn unreact_parses_message_and_emoji() {
        let cli = Cli::parse_from(["xmtp-cli", "unreact", "msg-10", "👍"]);

        match cli.command {
            Command::Unreact { message_id, emoji } => {
                assert_eq!(message_id, "msg-10");
                assert_eq!(emoji, "👍");
            }
            _ => panic!("expected unreact command"),
        }
    }

    #[test]
    fn leave_parses_conversation_id() {
        let cli = Cli::parse_from(["xmtp-cli", "leave", "conv-9"]);

        match cli.command {
            Command::Leave { conversation_id } => assert_eq!(conversation_id, "conv-9"),
            _ => panic!("expected leave command"),
        }
    }

    #[test]
    fn action_result_header_uses_expected_columns() {
        assert_eq!(
            action_result_header(),
            format!(
                "{:<16} {:<20} {}",
                "action", "conversation_id", "message_id"
            )
        );
    }

    #[test]
    fn render_action_result_uses_tabular_format() {
        let rendered = render_action_result(
            "reply",
            &ActionResponse {
                conversation_id: "1234567890abcdefghijklmnopqrstuvwxyz".to_owned(),
                message_id: "abcdef0123456789abcdef0123456789".to_owned(),
            },
        );

        assert_eq!(
            rendered,
            format!(
                "{:<16} {:<20} {}",
                "reply", "1234....wxyz", "abcd....6789"
            )
        );
    }

    #[test]
    fn render_conversation_row_uses_fixed_width_columns() {
        let rendered = render_conversation_row(&ConversationItem {
            id: "1234567890abcdefghijklmnopqrstuvwxyz".to_owned(),
            kind: "dm".to_owned(),
            name: Some("primary".to_owned()),
        });

        assert_eq!(
            rendered,
            format!("{:<20} {:<12} {}", "1234....wxyz", "dm", "primary")
        );
    }

    #[test]
    fn group_members_header_uses_expected_columns() {
        assert_eq!(
            group_members_header(),
            format!(
                "{:<20} {:<12} {:<12} {:<6} {}",
                "inbox_id", "permission", "consent", "inst", "accounts"
            )
        );
    }

    #[test]
    fn render_group_member_row_uses_fixed_width_columns() {
        let rendered = render_group_member_row(&GroupMemberItem {
            inbox_id: "1234567890abcdefghijklmnopqrstuvwxyz".to_owned(),
            permission_level: "admin".to_owned(),
            consent_state: "allowed".to_owned(),
            account_identifiers: vec!["0xabc".to_owned()],
            installation_count: 2,
        });

        assert_eq!(
            rendered,
            format!(
                "{:<20} {:<12} {:<12} {:<6} {}",
                "1234....wxyz", "admin", "allowed", 2, "0xabc"
            )
        );
    }

    #[test]
    fn group_info_header_uses_expected_columns() {
        assert_eq!(
            group_info_header(),
            format!(
                "{:<20} {:<12} {:<12} {:<20} {}",
                "conversation_id", "type", "members", "permission", "name"
            )
        );
    }

    #[test]
    fn render_group_info_row_uses_fixed_width_columns() {
        let rendered = render_group_info_row(&GroupInfoResponse {
            conversation_id: "1234567890abcdefghijklmnopqrstuvwxyz".to_owned(),
            name: Some("team".to_owned()),
            description: None,
            creator_inbox_id: "creator".to_owned(),
            conversation_type: "group".to_owned(),
            permission_preset: "default".to_owned(),
            member_count: 3,
        });

        assert_eq!(
            rendered,
            format!(
                "{:<20} {:<12} {:<12} {:<20} {}",
                "1234....wxyz", "group", 3, "default", "team"
            )
        );
    }

    #[test]
    fn render_history_line_uses_fixed_width_columns() {
        let line = render_history_line(&HistoryItem {
            message_id: "1234567890abcdefghijklmnopqrstuvwxyz".to_owned(),
            sender_inbox_id: "abcdef0123456789abcdef0123456789".to_owned(),
            sent_at_ns: 1_710_000_000_000_000_000,
            content_kind: "text".to_owned(),
            content: "hello".to_owned(),
            reply_count: 0,
            reaction_count: 0,
            reply_target_message_id: None,
            reaction_target_message_id: None,
            reaction_emoji: None,
            reaction_action: None,
            attached_reactions: Vec::new(),
        });

        assert_eq!(
            line,
            format!(
                "{:<16} {:<20} {:<20} {}",
                "03-09 16:00",
                "1234....wxyz",
                "abcd....6789",
                "hello"
            )
        );
    }

    #[test]
    fn render_status_row_uses_fixed_width_columns() {
        let rendered = render_status_row("daemon_state", "running");
        assert_eq!(rendered, format!("{:<18} {}", "daemon_state", "running"));
    }

    #[test]
    fn daemon_restart_parses_subcommand() {
        let cli = Cli::parse_from(["xmtp-cli", "daemon", "restart"]);

        match cli.command {
            Command::Daemon {
                command: DaemonCommand::Restart,
            } => {}
            _ => panic!("expected daemon restart command"),
        }
    }
}
