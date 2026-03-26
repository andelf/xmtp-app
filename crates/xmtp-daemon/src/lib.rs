use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::Context;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use futures_util::stream::Stream;
use prost::Message as ProstMessage;
use rusqlite::Connection;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{debug, error, info, warn};
use xmtp::content::{Content, ReactionAction};
use xmtp::{AlloySigner, Client, CreateGroupOptions, Env, Recipient};
use xmtp_config::{AppConfig, load_config, save_config};
use xmtp_core::{ConnectionState, DaemonState};
use xmtp_ipc::{
    ActionResponse, ApiErrorBody, ApiErrorDetail, ConversationInfoResponse, ConversationItem,
    ConversationListResponse, ConversationUpdatedEvent, DaemonEventData, DaemonEventEnvelope,
    EmojiRequest, GroupCreateRequest, GroupInfoResponse, GroupMemberItem, GroupMembersResponse,
    GroupMembersUpdateRequest, GroupMembersUpdatedEvent, HistoryItem, HistoryResponse,
    LoginRequest, MessageInfoResponse, ReactionDetail, RecipientMessageRequest, RecipientRequest,
    RenameGroupRequest, SendDmResponse, SendMessageRequest, StatusResponse,
};
use xmtp_logging::append_daemon_event;
use xmtp_store::{load_state, save_state};

static TRACING_INIT: OnceLock<()> = OnceLock::new();
static EVENT_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, prost::Message)]
struct GroupUpdatedInbox {
    #[prost(string, tag = "1")]
    inbox_id: String,
}

#[derive(Clone, prost::Message)]
struct MetadataFieldChange {
    #[prost(string, tag = "1")]
    field_name: String,
    #[prost(string, optional, tag = "2")]
    old_value: Option<String>,
    #[prost(string, optional, tag = "3")]
    new_value: Option<String>,
}

#[derive(Clone, prost::Message)]
struct GroupUpdated {
    #[prost(string, tag = "1")]
    initiated_by_inbox_id: String,
    #[prost(message, repeated, tag = "2")]
    added_inboxes: Vec<GroupUpdatedInbox>,
    #[prost(message, repeated, tag = "3")]
    removed_inboxes: Vec<GroupUpdatedInbox>,
    #[prost(message, repeated, tag = "4")]
    metadata_field_changes: Vec<MetadataFieldChange>,
    #[prost(message, repeated, tag = "5")]
    left_inboxes: Vec<GroupUpdatedInbox>,
}

#[derive(Clone, prost::Message)]
struct ReactionV2 {
    #[prost(string, tag = "1")]
    reference: String,
    #[prost(string, tag = "2")]
    reference_inbox_id: String,
    #[prost(int32, tag = "3")]
    action: i32,
    #[prost(string, tag = "4")]
    content: String,
    #[prost(int32, tag = "5")]
    schema: i32,
}

fn init_tracing() {
    TRACING_INIT.get_or_init(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_target(false)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_ansi(true)
            .with_writer(std::io::stderr)
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

fn daemon_log(data_dir: &Path, level: &str, message: impl AsRef<str>) {
    let message = message.as_ref();
    match level {
        "debug" => debug!("{message}"),
        "info" => info!("{message}"),
        "warn" => warn!("{message}"),
        "error" => error!("{message}"),
        _ => info!("{message}"),
    }
    let _ = append_daemon_event(data_dir, level, message);
}

pub struct RuntimeInfo {
    pub inbox_id: String,
    pub installation_id: String,
}

pub trait XmtpRuntimeAdapter {
    fn connect(&self, env: &str, data_dir: &Path) -> anyhow::Result<RuntimeInfo>;
}

pub fn login_with_adapter(
    adapter: &dyn XmtpRuntimeAdapter,
    env: &str,
    data_dir: &Path,
) -> anyhow::Result<()> {
    ensure_initialized(data_dir)?;
    let runtime = adapter.connect(env, data_dir)?;
    let mut state = load_state(&data_dir.join("state.json"))?;
    state.daemon_state = DaemonState::Running;
    state.connection_state = ConnectionState::Connected;
    state.inbox_id = Some(runtime.inbox_id);
    state.installation_id = Some(runtime.installation_id);
    save_state(&data_dir.join("state.json"), &state)?;
    Ok(())
}

pub fn load_or_create_signer_key_hex(data_dir: &Path) -> anyhow::Result<String> {
    let key_path = signer_key_path(data_dir);
    if key_path.exists() {
        let key = fs::read_to_string(&key_path).context("read signer key")?;
        return Ok(key.trim().to_owned());
    }

    fs::create_dir_all(data_dir).context("create data dir for signer key")?;
    let signer = AlloySigner::random();
    let key_hex = format!("{:x}", signer.into_inner().to_bytes());
    fs::write(&key_path, &key_hex).context("write signer key")?;
    Ok(key_hex)
}

fn signer_key_path(data_dir: &Path) -> PathBuf {
    data_dir.join("signer.key")
}

pub fn addr_path(data_dir: &Path) -> PathBuf {
    data_dir.join("daemon.addr")
}

pub fn pid_path(data_dir: &Path) -> PathBuf {
    data_dir.join("daemon.pid")
}

pub struct RealXmtpAdapter;

impl XmtpRuntimeAdapter for RealXmtpAdapter {
    fn connect(&self, env: &str, data_dir: &Path) -> anyhow::Result<RuntimeInfo> {
        let client = open_client_with_login(data_dir, env)?;

        Ok(RuntimeInfo {
            inbox_id: client.inbox_id().context("get inbox id")?,
            installation_id: client
                .installation_id()
                .context("get installation id")?,
        })
    }
}

fn parse_env(value: &str) -> Env {
    match value {
        "local" => Env::Local,
        "production" | "prod" => Env::Production,
        _ => Env::Dev,
    }
}

fn build_client(config: &AppConfig, signer: &AlloySigner, data_dir: &Path) -> anyhow::Result<Client> {
    let mut builder = Client::builder()
        .env(parse_env(&config.xmtp_env))
        .db_path(data_dir.join("xmtp.db3").display().to_string());
    if let Some(api_url) = &config.api_url {
        builder = builder.api_url(api_url.clone());
    }
    if let Some(gateway_url) = &config.gateway_url {
        builder = builder.gateway_host(gateway_url.clone());
    }
    let client = builder.build(signer).context("build XMTP client")?;
    Ok(client)
}

fn open_client_with_login(data_dir: &Path, env: &str) -> anyhow::Result<Client> {
    ensure_initialized(data_dir)?;
    let signer_hex = load_or_create_signer_key_hex(data_dir)?;
    let signer = AlloySigner::from_hex(&signer_hex).context("load signer from hex")?;
    let mut config = load_config(&data_dir.join("config.json"))?;
    if config.xmtp_env != env {
        config.xmtp_env = env.to_owned();
        save_config(&data_dir.join("config.json"), &config)?;
    }
    build_client(&config, &signer, data_dir)
}

pub fn configure_runtime(
    data_dir: &Path,
    env: &str,
    api_url: Option<&str>,
    gateway_url: Option<&str>,
) -> anyhow::Result<()> {
    ensure_initialized(data_dir)?;
    let mut config = load_config(&data_dir.join("config.json"))?;
    config.xmtp_env = env.to_owned();
    config.api_url = api_url.map(str::to_owned);
    config.gateway_url = gateway_url.map(str::to_owned);
    save_config(&data_dir.join("config.json"), &config)
}

fn ensure_initialized(data_dir: &Path) -> anyhow::Result<()> {
    let config_path = data_dir.join("config.json");
    let state_path = data_dir.join("state.json");
    if config_path.exists() && state_path.exists() {
        return Ok(());
    }
    anyhow::bail!(
        "data dir is not initialized; run `xmtp-cli --data-dir {} init` first",
        data_dir.display()
    );
}

pub struct ConversationSummary {
    pub id: String,
    pub kind: String,
    pub name: Option<String>,
    pub dm_peer_inbox_id: Option<String>,
    pub last_message_ns: Option<i64>,
}

pub struct SendMessageResult {
    pub conversation_id: String,
    pub message_id: String,
}

pub struct HistoryEntry {
    pub message_id: String,
    pub sender_inbox_id: String,
    pub sent_at_ns: i64,
    pub content_kind: String,
    pub content: String,
    pub reply_count: i32,
    pub reaction_count: i32,
    pub reply_target_message_id: Option<String>,
    pub reaction_target_message_id: Option<String>,
    pub reaction_emoji: Option<String>,
    pub reaction_action: Option<String>,
    pub attached_reactions: Vec<ReactionDetail>,
}

pub fn list_conversations(data_dir: &Path) -> anyhow::Result<Vec<ConversationSummary>> {
    let client = open_existing_client(data_dir)?;
    list_conversations_with_client(&client, None)
}

pub fn send_dm(
    data_dir: &Path,
    recipient: &str,
    text: &str,
    content_type: Option<&str>,
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    send_dm_with_client(&client, recipient, text, content_type)
}

pub fn open_dm(data_dir: &Path, recipient: &str) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    open_dm_with_client(&client, recipient)
}

pub fn history(data_dir: &Path, conversation_id: &str) -> anyhow::Result<Vec<HistoryEntry>> {
    history_with_kind(data_dir, conversation_id, None)
}

pub fn history_with_kind(
    data_dir: &Path,
    conversation_id: &str,
    kind: Option<&str>,
) -> anyhow::Result<Vec<HistoryEntry>> {
    let client = open_existing_client(data_dir)?;
    history_with_client(data_dir, &client, conversation_id, kind, None, 50)
}

pub fn resolve_conversation_id(
    data_dir: &Path,
    conversation_id: &str,
    kind: Option<&str>,
) -> anyhow::Result<String> {
    let client = open_existing_client(data_dir)?;
    let conversation = find_conversation_by_id_with_kind(&client, conversation_id, kind)?;
    Ok(conversation.id())
}

pub fn watch_history<F>(
    data_dir: &Path,
    conversation_id: &str,
    on_item: F,
) -> anyhow::Result<()>
where
    F: FnMut(HistoryItem),
{
    watch_history_with_kind(data_dir, conversation_id, None, on_item)
}

pub fn watch_history_with_kind<F>(
    data_dir: &Path,
    conversation_id: &str,
    kind: Option<&str>,
    mut on_item: F,
) -> anyhow::Result<()>
where
    F: FnMut(HistoryItem),
{
    let client = open_existing_client(data_dir)?;
    let conversation = find_conversation_by_id_with_kind(&client, conversation_id, kind)?;
    let subscription =
        xmtp::stream::conversation_messages(&conversation).context("watch conversation messages")?;

    for event in subscription {
        let item = history_item_by_message_id_with_client(&client, &event.message_id)?;
        on_item(item);
    }

    Ok(())
}

pub fn create_group(
    data_dir: &Path,
    name: Option<String>,
    members: &[String],
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    create_group_with_client(&client, name, members)
}

pub fn send_group(
    data_dir: &Path,
    conversation_id: &str,
    text: &str,
    content_type: Option<&str>,
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    send_group_with_client(&client, conversation_id, text, content_type)
}

pub fn group_members(
    data_dir: &Path,
    conversation_id: &str,
) -> anyhow::Result<Vec<GroupMemberItem>> {
    let client = open_existing_client(data_dir)?;
    group_members_with_client(&client, conversation_id, 200)
}

pub fn rename_group(
    data_dir: &Path,
    conversation_id: &str,
    name: &str,
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    rename_group_with_client(&client, conversation_id, name)
}

pub fn add_group_members(
    data_dir: &Path,
    conversation_id: &str,
    members: &[String],
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    add_group_members_with_client(&client, conversation_id, members)
}

pub fn remove_group_members(
    data_dir: &Path,
    conversation_id: &str,
    members: &[String],
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    remove_group_members_with_client(&client, conversation_id, members)
}

pub fn group_info(data_dir: &Path, conversation_id: &str) -> anyhow::Result<GroupInfoResponse> {
    let client = open_existing_client(data_dir)?;
    group_info_with_client(&client, conversation_id)
}

pub fn leave_conversation(
    data_dir: &Path,
    conversation_id: &str,
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    leave_conversation_with_client(&client, conversation_id)
}

pub fn reply(data_dir: &Path, message_id: &str, text: &str) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    reply_with_client(&client, message_id, text, None)
}

pub fn react(data_dir: &Path, message_id: &str, emoji: &str) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    react_with_action_with_client(&client, message_id, emoji, ReactionAction::Added, None)
}

pub fn unreact(
    data_dir: &Path,
    message_id: &str,
    emoji: &str,
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    react_with_action_with_client(&client, message_id, emoji, ReactionAction::Removed, None)
}

pub fn conversation_info(
    data_dir: &Path,
    conversation_id: &str,
) -> anyhow::Result<ConversationInfoResponse> {
    let client = open_existing_client(data_dir)?;
    conversation_info_with_client(&client, conversation_id)
}

pub fn message_info(data_dir: &Path, message_id: &str) -> anyhow::Result<MessageInfoResponse> {
    let client = open_existing_client(data_dir)?;
    message_info_with_client(&client, message_id)
}

fn open_existing_client(data_dir: &Path) -> anyhow::Result<Client> {
    ensure_initialized(data_dir)?;
    let signer_hex = load_or_create_signer_key_hex(data_dir)?;
    let signer = AlloySigner::from_hex(&signer_hex).context("load signer from hex")?;
    let config = load_config(&data_dir.join("config.json"))?;
    build_client(&config, &signer, data_dir)
}

fn list_conversations_with_client(
    client: &Client,
    filter_kind: Option<&str>,
) -> anyhow::Result<Vec<ConversationSummary>> {
    client.sync_welcomes().context("sync welcomes")?;
    client.sync_all(&[]).context("sync conversations")?;
    let conversations = client.conversations().context("list conversations")?;
    let mut summaries = Vec::with_capacity(conversations.len());
    for conversation in conversations {
        let kind = match conversation.conversation_type() {
            Some(kind) => format!("{kind:?}").to_lowercase(),
            None => "unknown".to_owned(),
        };
        if let Some(filter_kind) = filter_kind {
            if kind != filter_kind {
                continue;
            }
        }
        summaries.push(ConversationSummary {
            id: conversation.id(),
            kind,
            name: conversation.name(),
            dm_peer_inbox_id: conversation.dm_peer_inbox_id(),
            last_message_ns: conversation
                .last_message()
                .ok()
                .and_then(|message| message.map(|message| message.sent_at_ns)),
        });
    }
    sort_conversation_summaries(&mut summaries);
    Ok(summaries)
}

fn sort_conversation_summaries(items: &mut [ConversationSummary]) {
    items.sort_by_key(|item| Reverse(item.last_message_ns.unwrap_or_default()));
}

fn send_dm_with_client(
    client: &Client,
    recipient: &str,
    text: &str,
    content_type: Option<&str>,
) -> anyhow::Result<SendMessageResult> {
    let recipient = Recipient::parse(recipient);
    let conversation = client.dm(&recipient).context("create or find DM")?;
    let message_id = match content_type {
        Some("markdown") => conversation.send_markdown(text).context("send DM markdown")?,
        _ => conversation.send_text(text).context("send DM text")?,
    };
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id,
    })
}

fn open_dm_with_client(client: &Client, recipient: &str) -> anyhow::Result<SendMessageResult> {
    let recipient = Recipient::parse(recipient);
    let conversation = client.dm(&recipient).context("create or find DM")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: String::new(),
    })
}

fn create_group_with_client(
    client: &Client,
    name: Option<String>,
    members: &[String],
) -> anyhow::Result<SendMessageResult> {
    let recipients: Vec<Recipient> = members.iter().map(|member| Recipient::parse(member)).collect();
    let conversation = client
        .group(
            &recipients,
            &CreateGroupOptions {
                name,
                ..Default::default()
            },
        )
        .context("create group")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: String::new(),
    })
}

fn send_group_with_client(
    client: &Client,
    conversation_id: &str,
    text: &str,
    content_type: Option<&str>,
) -> anyhow::Result<SendMessageResult> {
    send_group_with_client_logged(client, conversation_id, text, content_type, None)
}

fn send_group_with_client_logged(
    client: &Client,
    conversation_id: &str,
    text: &str,
    content_type: Option<&str>,
    data_dir: Option<&Path>,
) -> anyhow::Result<SendMessageResult> {
    let resolve_started = Instant::now();
    let conversation = find_conversation_by_id(client, conversation_id)?;
    if let Some(data_dir) = data_dir {
        daemon_log(
            data_dir,
            "debug",
            format!(
                "group send stage=resolve query={} resolved={} elapsed_ms={}",
                conversation_id,
                conversation.id(),
                resolve_started.elapsed().as_millis()
            ),
        );
    }
    let send_started = Instant::now();
    let message_id = match content_type {
        Some("markdown") => conversation
            .send_markdown(text)
            .context("send group markdown")?,
        _ => conversation.send_text(text).context("send group text")?,
    };
    if let Some(data_dir) = data_dir {
        daemon_log(
            data_dir,
            "debug",
            format!(
                "group send stage=send_text conversation={} elapsed_ms={}",
                conversation.id(),
                send_started.elapsed().as_millis()
            ),
        );
    }
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id,
    })
}

fn rename_group_with_client(
    client: &Client,
    conversation_id: &str,
    name: &str,
) -> anyhow::Result<SendMessageResult> {
    let conversation = find_conversation_by_id(client, conversation_id)?;
    conversation.set_name(name).context("rename group")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: String::new(),
    })
}

fn add_group_members_with_client(
    client: &Client,
    conversation_id: &str,
    members: &[String],
) -> anyhow::Result<SendMessageResult> {
    let conversation = find_conversation_by_id(client, conversation_id)?;
    let recipients: Vec<Recipient> = members.iter().map(|member| Recipient::parse(member)).collect();
    client
        .add_members(&conversation, &recipients)
        .context("add group members")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: String::new(),
    })
}

fn remove_group_members_with_client(
    client: &Client,
    conversation_id: &str,
    members: &[String],
) -> anyhow::Result<SendMessageResult> {
    let conversation = find_conversation_by_id(client, conversation_id)?;
    let recipients: Vec<Recipient> = members.iter().map(|member| Recipient::parse(member)).collect();
    client
        .remove_members(&conversation, &recipients)
        .context("remove group members")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: String::new(),
    })
}

fn group_members_with_client(
    client: &Client,
    conversation_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<GroupMemberItem>> {
    let conversation = find_conversation_by_id(client, conversation_id)?;
    let members = conversation.members().context("list group members")?;
    Ok(members
        .into_iter()
        .take(limit)
        .map(|member| GroupMemberItem {
            inbox_id: member.inbox_id,
            permission_level: format!("{:?}", member.permission_level).to_lowercase(),
            consent_state: format!("{:?}", member.consent_state).to_lowercase(),
            account_identifiers: member.account_identifiers,
            installation_count: member.installation_ids.len(),
        })
        .collect())
}

fn group_info_with_client(client: &Client, conversation_id: &str) -> anyhow::Result<GroupInfoResponse> {
    let conversation = find_conversation_by_id(client, conversation_id)?;
    let members = conversation.members().context("list group members")?;
    Ok(GroupInfoResponse {
        conversation_id: conversation.id(),
        name: conversation.name(),
        description: conversation.description(),
        creator_inbox_id: members
            .iter()
            .find(|member| format!("{:?}", member.permission_level).eq_ignore_ascii_case("superadmin"))
            .map(|member| member.inbox_id.clone())
            .or_else(|| members.first().map(|member| member.inbox_id.clone()))
            .unwrap_or_default(),
        conversation_type: conversation
            .conversation_type()
            .map(|kind| format!("{kind:?}").to_lowercase())
            .unwrap_or_else(|| "unknown".to_owned()),
        permission_preset: "unknown".to_owned(),
        member_count: members.len(),
    })
}

fn leave_conversation_with_client(
    client: &Client,
    conversation_id: &str,
) -> anyhow::Result<SendMessageResult> {
    let conversation = find_conversation_by_id(client, conversation_id)?;
    conversation.leave().context("leave conversation")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: String::new(),
    })
}

fn reply_with_client(
    client: &Client,
    message_id: &str,
    text: &str,
    conversation_id: Option<&str>,
) -> anyhow::Result<SendMessageResult> {
    let (conversation, resolved_message_id) = if let Some(conversation_id) = conversation_id {
        let conversation = find_conversation_by_id(client, conversation_id)?;
        let messages = conversation.messages().context("list messages")?;
        let ids: Vec<String> = messages.iter().map(|message| message.id.clone()).collect();
        let resolved_message_id = resolve_message_id(&ids, message_id)?;
        (conversation, resolved_message_id)
    } else {
        find_message_conversation(client, message_id)?
    };
    let sent_message_id = conversation
        .send_text_reply(&resolved_message_id, text)
        .context("send reply text")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: sent_message_id,
    })
}

fn react_with_action_with_client(
    client: &Client,
    message_id: &str,
    emoji: &str,
    action: ReactionAction,
    conversation_id: Option<&str>,
) -> anyhow::Result<SendMessageResult> {
    let (conversation, resolved_message_id) = if let Some(conversation_id) = conversation_id {
        let conversation = find_conversation_by_id(client, conversation_id)?;
        let messages = conversation.messages().context("list messages")?;
        let ids: Vec<String> = messages.iter().map(|message| message.id.clone()).collect();
        let resolved_message_id = resolve_message_id(&ids, message_id)?;
        (conversation, resolved_message_id)
    } else {
        find_message_conversation(client, message_id)?
    };
    let sent_message_id = conversation
        .send_reaction(&resolved_message_id, emoji, action)
        .context("send reaction")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: sent_message_id,
    })
}

fn conversation_info_with_client(
    client: &Client,
    conversation_id: &str,
) -> anyhow::Result<ConversationInfoResponse> {
    let conversation = find_conversation_by_id(client, conversation_id)?;
    let member_count = conversation.members().map(|members| members.len()).unwrap_or(0);
    let message_count = conversation.count_messages(&xmtp::ListMessagesOptions::default());

    Ok(ConversationInfoResponse {
        conversation_id: conversation.id(),
        name: conversation.name(),
        conversation_type: conversation
            .conversation_type()
            .map(|kind| format!("{kind:?}").to_lowercase())
            .unwrap_or_else(|| "unknown".to_owned()),
        created_at_ns: conversation.created_at_ns(),
        is_active: conversation.is_active(),
        membership_state: conversation
            .membership_state()
            .map(|state| format!("{state:?}").to_lowercase())
            .unwrap_or_else(|| "unknown".to_owned()),
        dm_peer_inbox_id: conversation.dm_peer_inbox_id(),
        member_count,
        message_count,
    })
}

fn message_info_with_client(client: &Client, message_id: &str) -> anyhow::Result<MessageInfoResponse> {
    let (_conversation, resolved_message_id) = find_message_conversation(client, message_id)?;
    let message = client
        .message_by_id(&resolved_message_id)
        .context("load message by id")?
        .context("message not found")?;
    let content_summary = summarize_message_content(&message);

    Ok(MessageInfoResponse {
        message_id: message.id,
        conversation_id: message.conversation_id,
        sender_inbox_id: message.sender_inbox_id,
        sent_at_ns: message.sent_at_ns,
        delivery_status: format!("{:?}", message.delivery_status).to_lowercase(),
        content_type: message.content_type,
        content_summary,
        reply_count: message.num_replies,
        reaction_count: message.num_reactions,
    })
}

fn history_with_client(
    data_dir: &Path,
    client: &Client,
    conversation_id: &str,
    kind: Option<&str>,
    before_ns: Option<i64>,
    limit: usize,
) -> anyhow::Result<Vec<HistoryEntry>> {
    let conversation = find_conversation_by_id_with_kind(client, conversation_id, kind)?;
    conversation.sync().context("sync conversation")?;
    let messages = conversation
        .list_messages(&xmtp::ListMessagesOptions {
            sent_before_ns: before_ns.unwrap_or_default(),
            limit: limit as i64,
            direction: Some(xmtp::SortDirection::Descending),
            ..Default::default()
        })
        .context("list messages")?;
    let mut entries = Vec::with_capacity(messages.len());
    for message in messages {
        entries.push(history_entry_from_message(&message));
    }
    let message_ids = entries
        .iter()
        .map(|entry| entry.message_id.clone())
        .collect::<Vec<_>>();
    let reaction_map = fetch_reactions_from_db(data_dir, &conversation.id(), &message_ids)
        .context("fetch reactions from sqlite")?;
    for entry in &mut entries {
        entry.attached_reactions = reaction_map
            .get(&entry.message_id.to_lowercase())
            .cloned()
            .unwrap_or_default();
    }
    let reaction_count: usize = entries.iter().map(|entry| entry.attached_reactions.len()).sum();
    for entry in &entries {
        tracing::debug!(
            message_id = %entry.message_id,
            attached_reactions = entry.attached_reactions.len(),
            "history message"
        );
    }
    tracing::debug!(
        total_messages = entries.len(),
        total_reactions = reaction_count,
        "history loaded"
    );
    entries.reverse();
    Ok(entries)
}

fn history_item_by_message_id_with_client(
    client: &Client,
    message_id: &str,
) -> anyhow::Result<HistoryItem> {
    for _ in 0..10 {
        if let Some(message) = client
            .message_by_id(message_id)
            .context("load message by id")?
        {
            return Ok(history_item_from_message(&message));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    anyhow::bail!("message not found for id {message_id}")
}

fn history_entry_from_message(message: &xmtp::conversation::Message) -> HistoryEntry {
    let item = history_item_from_message(message);
    HistoryEntry {
        message_id: item.message_id,
        sender_inbox_id: item.sender_inbox_id,
        sent_at_ns: item.sent_at_ns,
        content_kind: item.content_kind,
        content: item.content,
        reply_count: item.reply_count,
        reaction_count: item.reaction_count,
        reply_target_message_id: item.reply_target_message_id,
        reaction_target_message_id: item.reaction_target_message_id,
        reaction_emoji: item.reaction_emoji,
        reaction_action: item.reaction_action,
        attached_reactions: item.attached_reactions,
    }
}

fn history_item_from_message(message: &xmtp::conversation::Message) -> HistoryItem {
    let (
        content_kind,
        content,
        reply_target_message_id,
        reaction_target_message_id,
        reaction_emoji,
        reaction_action,
    ) = match message.decode() {
        Ok(Content::Text(text)) => ("text".to_owned(), text, None, None, None, None),
        Ok(Content::Markdown(markdown)) => ("markdown".to_owned(), markdown, None, None, None, None),
        Ok(Content::Reaction(reaction)) => (
            "reaction".to_owned(),
            summarize_decoded_content(&Content::Reaction(reaction.clone())),
            None,
            Some(reaction.reference),
            Some(reaction.content),
            Some(match reaction.action {
                ReactionAction::Added => "added".to_owned(),
                ReactionAction::Removed => "removed".to_owned(),
                ReactionAction::Unspecified => "unspecified".to_owned(),
            }),
        ),
        Ok(Content::Reply(reply)) => (
            "reply".to_owned(),
            summarize_decoded_content(&Content::Reply(reply.clone())),
            Some(reply.reference),
            None,
            None,
            None,
        ),
        Ok(Content::ReadReceipt) => (
            "read_receipt".to_owned(),
            "read receipt".to_owned(),
            None,
            None,
            None,
            None,
        ),
        Ok(Content::Attachment(attachment)) => (
            "attachment".to_owned(),
            summarize_decoded_content(&Content::Attachment(attachment)),
            None,
            None,
            None,
            None,
        ),
        Ok(Content::RemoteAttachment(attachment)) => (
            "remote_attachment".to_owned(),
            summarize_decoded_content(&Content::RemoteAttachment(attachment)),
            None,
            None,
            None,
            None,
        ),
        Ok(Content::Unknown { content_type, raw }) => {
            log_unknown_message_type(Some(&message.id), &content_type, &raw, message.fallback.as_ref());
            (
                "unknown".to_owned(),
                message
                    .fallback
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| summarize_unknown_content(&content_type, &raw)),
                None,
                None,
                None,
                None,
            )
        }
        Err(_) => (
            "unknown".to_owned(),
            message
                .fallback
                .clone()
                .unwrap_or_else(|| "<undecodable>".to_owned()),
            None,
            None,
            None,
            None,
        ),
    };

    HistoryItem {
        message_id: message.id.clone(),
        sender_inbox_id: message.sender_inbox_id.clone(),
        sent_at_ns: message.sent_at_ns,
        content_kind,
        content,
        reply_count: message.num_replies,
        reaction_count: message.num_reactions,
        reply_target_message_id,
        reaction_target_message_id,
        reaction_emoji,
        reaction_action,
        attached_reactions: Vec::new(),
    }
}

fn summarize_message_content(message: &xmtp::conversation::Message) -> String {
    match message.decode() {
        Ok(Content::Unknown { content_type, raw }) => {
            log_unknown_message_type(None, &content_type, &raw, message.fallback.as_ref());
            message
                .fallback
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| summarize_unknown_content(&content_type, &raw))
        }
        Ok(decoded) => summarize_decoded_content(&decoded),
        Err(_) => message
            .fallback
            .clone()
            .unwrap_or_else(|| "<undecodable>".to_owned()),
    }
}

fn fetch_reactions_from_db(
    data_dir: &Path,
    conversation_id: &str,
    message_ids: &[String],
) -> anyhow::Result<HashMap<String, Vec<ReactionDetail>>> {
    if message_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let db_path = data_dir.join("xmtp.db3");
    let conn = Connection::open(&db_path)
        .with_context(|| format!("open sqlite db {}", db_path.display()))?;
    let mut stmt = conn.prepare(
        "SELECT lower(hex(reference_id)), sender_inbox_id, decrypted_message_bytes
         FROM group_messages
         WHERE lower(hex(group_id)) = ?1
           AND content_type = 4
           AND reference_id IS NOT NULL",
    )?;

    let wanted_ids: HashSet<String> = message_ids.iter().map(|id| id.to_lowercase()).collect();
    let rows = stmt.query_map([conversation_id.to_lowercase()], |row| {
        let reference_id: String = row.get(0)?;
        let sender_inbox_id: String = row.get(1)?;
        let decrypted_message_bytes: Vec<u8> = row.get(2)?;
        Ok((reference_id, sender_inbox_id, decrypted_message_bytes))
    })?;

    let mut reaction_map: HashMap<String, Vec<ReactionDetail>> = HashMap::new();
    for row in rows {
        let (reference_id, sender_inbox_id, decrypted_message_bytes) = row?;
        if !wanted_ids.contains(&reference_id) {
            continue;
        }

        let encoded = xmtp::content::EncodedContent::decode(decrypted_message_bytes.as_slice())
            .context("decode reaction encoded content")?;
        let reaction = ReactionV2::decode(encoded.content.as_slice())
            .context("decode reaction payload")?;

        reaction_map
            .entry(reference_id)
            .or_default()
            .push(ReactionDetail {
                sender_inbox_id,
                emoji: reaction.content,
                action: match reaction.action {
                    1 => "added".to_owned(),
                    2 => "removed".to_owned(),
                    _ => "unspecified".to_owned(),
                },
            });
    }

    Ok(reaction_map)
}

fn find_conversation_by_id(
    client: &Client,
    conversation_id: &str,
) -> anyhow::Result<xmtp::conversation::Conversation> {
    find_conversation_by_id_with_kind(client, conversation_id, None)
}

fn find_conversation_by_id_with_kind(
    client: &Client,
    conversation_id: &str,
    kind: Option<&str>,
) -> anyhow::Result<xmtp::conversation::Conversation> {
    let conversations = client.conversations().context("list local conversations")?;
    if let Ok(resolved_id) = resolve_conversation_query(&conversations, conversation_id, kind) {
        if let Some(conversation) = conversations
            .into_iter()
            .find(|conversation| conversation.id() == resolved_id)
        {
            return Ok(conversation);
        }
    }

    client.sync_welcomes().context("sync welcomes")?;
    client.sync_all(&[]).context("sync conversations")?;
    let conversations = client.conversations().context("list conversations")?;
    let resolved_id = resolve_conversation_query(&conversations, conversation_id, kind)?;
    conversations
        .into_iter()
        .find(|conversation| conversation.id() == resolved_id)
        .context("conversation not found")
}

fn resolve_conversation_query(
    conversations: &[xmtp::conversation::Conversation],
    query: &str,
    kind: Option<&str>,
) -> anyhow::Result<String> {
    let lookups: Vec<ConversationLookup> = conversations
        .iter()
        .map(|conversation| {
            let id = conversation.id();
            let name = conversation.name();
            let kind = conversation
                .conversation_type()
                .map(|kind| format!("{kind:?}").to_lowercase())
                .unwrap_or_else(|| "unknown".to_owned());
            ConversationLookup { id, name, kind }
        })
        .collect();
    resolve_conversation_lookup_id(&lookups, query, kind)
}

struct ConversationLookup {
    id: String,
    name: Option<String>,
    kind: String,
}

fn resolve_conversation_lookup_id(
    conversations: &[ConversationLookup],
    query: &str,
    kind: Option<&str>,
) -> anyhow::Result<String> {
    let candidates: Vec<&ConversationLookup> = conversations
        .iter()
        .filter(|conversation| kind.is_none_or(|expected| conversation.kind == expected))
        .collect();

    if let Some(conversation) = candidates.iter().find(|conversation| conversation.id == query) {
        return Ok(conversation.id.clone());
    }

    let matched_by_id: Vec<&ConversationLookup> = candidates
        .iter()
        .copied()
        .filter(|conversation| conversation_id_matches(&conversation.id, query))
        .collect();

    match matched_by_id.as_slice() {
        [conversation] => return Ok(conversation.id.clone()),
        [] => {}
        _ => anyhow::bail!("conversation id {query} is ambiguous"),
    }

    let matched_by_name: Vec<&ConversationLookup> = candidates
        .iter()
        .copied()
        .filter(|conversation| conversation.name.as_deref() == Some(query))
        .collect();

    match matched_by_name.as_slice() {
        [conversation] => Ok(conversation.id.clone()),
        [] => anyhow::bail!("conversation not found for id {query}"),
        _ => anyhow::bail!("conversation name {query} is ambiguous"),
    }
}

fn resolve_message_id(ids: &[String], query: &str) -> anyhow::Result<String> {
    if let Some(id) = ids.iter().find(|id| *id == query) {
        return Ok(id.clone());
    }

    let matched: Vec<&String> = ids
        .iter()
        .filter(|id| conversation_id_matches(id, query))
        .collect();

    match matched.as_slice() {
        [id] => Ok((*id).clone()),
        [] => anyhow::bail!("message not found for id {query}"),
        _ => anyhow::bail!("message id {query} is ambiguous"),
    }
}

fn conversation_id_matches(id: &str, query: &str) -> bool {
    if let Some((prefix, suffix)) = query.split_once("....") {
        return id.starts_with(prefix) && id.ends_with(suffix);
    }
    id.starts_with(query)
}

fn summarize_decoded_content(content: &Content) -> String {
    match content {
        Content::Text(text) => text.clone(),
        Content::Markdown(markdown) => markdown.clone(),
        Content::Reaction(reaction) => {
            format!(
                "{} {} to {}",
                match reaction.action {
                    ReactionAction::Added => "reacted",
                    ReactionAction::Removed => "removed reaction",
                    ReactionAction::Unspecified => "reaction",
                },
                reaction.content,
                short_id(&reaction.reference)
            )
        }
        Content::Reply(reply) => {
            reply
                .content
                .r#type
                .as_ref()
                .filter(|t| t.type_id == "text" || t.type_id == "markdown")
                .and_then(|_| String::from_utf8(reply.content.content.clone()).ok())
                .or_else(|| reply.content.fallback.clone())
                .unwrap_or_else(|| "(reply)".to_owned())
        }
        Content::ReadReceipt => "read receipt".to_owned(),
        Content::Attachment(attachment) => format!(
            "attachment {}",
            attachment
                .filename
                .clone()
                .unwrap_or_else(|| attachment.mime_type.clone())
        ),
        Content::RemoteAttachment(attachment) => format!(
            "remote attachment {}",
            attachment
                .filename
                .clone()
                .unwrap_or_else(|| attachment.url.clone())
        ),
        Content::Unknown { content_type, raw } => summarize_unknown_content(content_type, raw),
    }
}

fn summarize_unknown_content(content_type: &str, raw: &[u8]) -> String {
    if content_type.contains("group_updated") {
        if let Some(group_updated) = decode_group_updated(raw) {
            let mut parts = Vec::new();

            if !group_updated.added_inboxes.is_empty() {
                let count = group_updated.added_inboxes.len();
                parts.push(format!("added {count} member{}", if count == 1 { "" } else { "s" }));
            }
            if !group_updated.removed_inboxes.is_empty() {
                let count = group_updated.removed_inboxes.len();
                parts.push(format!("removed {count} member{}", if count == 1 { "" } else { "s" }));
            }
            if !group_updated.left_inboxes.is_empty() {
                let count = group_updated.left_inboxes.len();
                parts.push(format!("{count} member{} left", if count == 1 { "" } else { "s" }));
            }
            if let Some(rename) = group_updated
                .metadata_field_changes
                .iter()
                .find(|change| change.field_name.contains("name"))
                .and_then(|change| change.new_value.as_ref())
            {
                parts.push(format!("renamed to {rename}"));
            } else if !group_updated.metadata_field_changes.is_empty() {
                parts.push("updated group metadata".to_owned());
            }

            if !parts.is_empty() {
                return parts.join(", ");
            }
            return "updated group".to_owned();
        }
    }

    format!("unsupported {content_type}")
}

fn log_unknown_message_type(
    message_id: Option<&str>,
    content_type: &str,
    raw: &[u8],
    fallback: Option<&String>,
) {
    debug!(
        message_id = message_id.unwrap_or(""),
        content_type = %content_type,
        raw_len = raw.len(),
        fallback = ?fallback,
        "unknown message type received"
    );

    if content_type.contains("group_updated") {
        if let Some(group_updated) = decode_group_updated(raw) {
            let added: Vec<&str> = group_updated
                .added_inboxes
                .iter()
                .map(|inbox| inbox.inbox_id.as_str())
                .collect();
            let removed: Vec<&str> = group_updated
                .removed_inboxes
                .iter()
                .map(|inbox| inbox.inbox_id.as_str())
                .collect();
            let left: Vec<&str> = group_updated
                .left_inboxes
                .iter()
                .map(|inbox| inbox.inbox_id.as_str())
                .collect();
            let metadata_changes: Vec<String> = group_updated
                .metadata_field_changes
                .iter()
                .map(|change| {
                    format!(
                        "{}:{:?}->{:?}",
                        change.field_name, change.old_value, change.new_value
                    )
                })
                .collect();
            debug!(
                message_id = message_id.unwrap_or(""),
                content_type = %content_type,
                initiated_by = %group_updated.initiated_by_inbox_id,
                added = ?added,
                removed = ?removed,
                left = ?left,
                metadata_changes = ?metadata_changes,
                "decoded group_updated message"
            );
        }
    }
}

fn decode_group_updated(raw: &[u8]) -> Option<GroupUpdated> {
    let encoded = xmtp::content::EncodedContent::decode(raw).ok()?;
    GroupUpdated::decode(encoded.content.as_slice()).ok()
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

fn find_message_conversation(client: &Client, message_id: &str) -> anyhow::Result<(xmtp::conversation::Conversation, String)> {
    client.sync_welcomes().context("sync welcomes")?;
    client.sync_all(&[]).context("sync conversations")?;
    let conversations = client.conversations().context("list conversations")?;
    let mut matches = Vec::new();

    for conversation in conversations {
        conversation.sync().context("sync conversation")?;
        let messages = conversation.messages().context("list messages")?;
        let ids: Vec<String> = messages.iter().map(|message| message.id.clone()).collect();
        if let Ok(resolved_message_id) = resolve_message_id(&ids, message_id) {
            matches.push((conversation, resolved_message_id));
        }
    }

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => anyhow::bail!("message not found for id {message_id}"),
        _ => anyhow::bail!("message id {message_id} is ambiguous"),
    }
}

struct DaemonApp {
    data_dir: PathBuf,
    client: Option<Client>,
    last_status_event: Option<StatusResponse>,
    last_conversation_event: Option<ConversationListResponse>,
    conversation_cache: Vec<ConversationLookup>,
}

#[derive(Clone)]
struct HttpState {
    app: Arc<Mutex<DaemonApp>>,
    shutdown_tx: mpsc::UnboundedSender<()>,
    events_tx: broadcast::Sender<DaemonEventEnvelope>,
}

type ApiErrorResponse = (StatusCode, Json<ApiErrorBody>);

#[derive(Debug, Clone, serde::Deserialize)]
struct ConversationsQuery {
    kind: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HistoryQuery {
    before_ns: Option<i64>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MembersQuery {
    limit: Option<usize>,
}

impl DaemonApp {
    fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            client: None,
            last_status_event: None,
            last_conversation_event: None,
            conversation_cache: Vec::new(),
        }
    }

    fn status(&self) -> anyhow::Result<StatusResponse> {
        ensure_initialized(&self.data_dir)?;
        let state = load_state(&self.data_dir.join("state.json"))?;
        Ok(StatusResponse {
            daemon_state: state.daemon_state,
            connection_state: state.connection_state,
            inbox_id: state.inbox_id,
            installation_id: state.installation_id,
        })
    }

    fn conversation_list(&mut self) -> anyhow::Result<ConversationListResponse> {
        let items: Vec<ConversationItem> = list_conversations_with_client(self.ensure_client()?, None)?
            .into_iter()
            .map(|item| ConversationItem {
                id: item.id,
                kind: item.kind,
                name: item.name,
                dm_peer_inbox_id: item.dm_peer_inbox_id,
                last_message_ns: item.last_message_ns,
            })
            .collect();
        self.remember_conversation_items(&items);
        Ok(ConversationListResponse { items })
    }

    fn ensure_client(&mut self) -> anyhow::Result<&Client> {
        if self.client.is_none() {
            daemon_log(
                &self.data_dir,
                "info",
                "opening XMTP client from local state",
            );
            self.client = Some(open_existing_client(&self.data_dir)?);
            daemon_log(&self.data_dir, "info", "XMTP client ready");
        }
        self.client
            .as_ref()
            .context("daemon client is not initialized")
    }

    fn next_status_event(&mut self) -> Option<DaemonEventData> {
        let status = self.status().ok()?;
        if self.last_status_event.as_ref() == Some(&status) {
            return None;
        }
        self.last_status_event = Some(status.clone());
        Some(DaemonEventData::Status(status))
    }

    fn next_conversation_list_event(&mut self) -> Option<DaemonEventData> {
        let conversations = self.conversation_list().ok()?;
        if self.last_conversation_event.as_ref() == Some(&conversations) {
            return None;
        }
        self.last_conversation_event = Some(conversations.clone());
        Some(DaemonEventData::ConversationList(conversations))
    }

    fn login(
        &mut self,
        env: String,
        api_url: Option<String>,
        gateway_url: Option<String>,
    ) -> anyhow::Result<StatusResponse> {
        configure_runtime(
            &self.data_dir,
            &env,
            api_url.as_deref(),
            gateway_url.as_deref(),
        )?;
        let client = open_client_with_login(&self.data_dir, &env)?;
        let runtime = RuntimeInfo {
            inbox_id: client.inbox_id().context("get inbox id")?,
            installation_id: client.installation_id().context("get installation id")?,
        };
        let mut state = load_state(&self.data_dir.join("state.json"))?;
        state.daemon_state = DaemonState::Running;
        state.connection_state = ConnectionState::Connected;
        state.inbox_id = Some(runtime.inbox_id);
        state.installation_id = Some(runtime.installation_id);
        save_state(&self.data_dir.join("state.json"), &state)?;
        self.client = Some(client);
        self.status()
    }

    fn list_conversations(&mut self, kind: Option<String>) -> anyhow::Result<ConversationListResponse> {
        let items: Vec<ConversationItem> = list_conversations_with_client(self.ensure_client()?, kind.as_deref())?
            .into_iter()
            .map(|item| ConversationItem {
                id: item.id,
                kind: item.kind,
                name: item.name,
                dm_peer_inbox_id: item.dm_peer_inbox_id,
                last_message_ns: item.last_message_ns,
            })
            .collect();
        if kind.is_none() {
            self.remember_conversation_items(&items);
        }
        Ok(ConversationListResponse { items })
    }

    fn conversation_info(&mut self, conversation_id: String) -> anyhow::Result<ConversationInfoResponse> {
        conversation_info_with_client(self.ensure_client()?, &conversation_id)
    }

    fn history_snapshot(
        &mut self,
        conversation_id: String,
        before_ns: Option<i64>,
        limit: usize,
    ) -> anyhow::Result<HistoryResponse> {
        let data_dir = self.data_dir.clone();
        let client = self.ensure_client()?;
        let items = history_with_client(
            &data_dir,
            client,
            &conversation_id,
            None,
            before_ns,
            limit,
        )?
            .into_iter()
            .map(|item| HistoryItem {
                message_id: item.message_id,
                sender_inbox_id: item.sender_inbox_id,
                sent_at_ns: item.sent_at_ns,
                content_kind: item.content_kind,
                content: item.content,
                reply_count: item.reply_count,
                reaction_count: item.reaction_count,
                reply_target_message_id: item.reply_target_message_id,
                reaction_target_message_id: item.reaction_target_message_id,
                reaction_emoji: item.reaction_emoji,
                reaction_action: item.reaction_action,
                attached_reactions: item.attached_reactions,
            })
            .collect();
        Ok(HistoryResponse { items })
    }

    fn open_dm(&mut self, recipient: String) -> anyhow::Result<ActionResponse> {
        let result = open_dm_with_client(self.ensure_client()?, &recipient)?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn send_dm(
        &mut self,
        recipient: String,
        message: String,
        content_type: Option<String>,
    ) -> anyhow::Result<SendDmResponse> {
        let result = send_dm_with_client(
            self.ensure_client()?,
            &recipient,
            &message,
            content_type.as_deref(),
        )?;
        Ok(SendDmResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn create_group(&mut self, name: Option<String>, members: Vec<String>) -> anyhow::Result<ActionResponse> {
        let result = create_group_with_client(self.ensure_client()?, name, &members)?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn send_group(
        &mut self,
        conversation_id: String,
        message: String,
        content_type: Option<String>,
    ) -> anyhow::Result<ActionResponse> {
        let resolved_conversation_id = self
            .resolve_cached_conversation_id(&conversation_id, Some("group"))
            .unwrap_or_else(|| conversation_id.clone());
        let data_dir = self.data_dir.clone();
        let result = send_group_with_client_logged(
            self.ensure_client()?,
            &resolved_conversation_id,
            &message,
            content_type.as_deref(),
            Some(data_dir.as_path()),
        )?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn reply(
        &mut self,
        message_id: String,
        message: String,
        conversation_id: Option<String>,
    ) -> anyhow::Result<ActionResponse> {
        let result = reply_with_client(
            self.ensure_client()?,
            &message_id,
            &message,
            conversation_id.as_deref(),
        )?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn react_with_action(
        &mut self,
        message_id: String,
        emoji: String,
        action: ReactionAction,
        conversation_id: Option<String>,
    ) -> anyhow::Result<ActionResponse> {
        let result = react_with_action_with_client(
            self.ensure_client()?,
            &message_id,
            &emoji,
            action,
            conversation_id.as_deref(),
        )?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn group_members(
        &mut self,
        conversation_id: String,
        limit: usize,
    ) -> anyhow::Result<GroupMembersResponse> {
        let items = group_members_with_client(self.ensure_client()?, &conversation_id, limit)?;
        Ok(GroupMembersResponse { items })
    }

    fn group_info(&mut self, conversation_id: String) -> anyhow::Result<GroupInfoResponse> {
        group_info_with_client(self.ensure_client()?, &conversation_id)
    }

    fn rename_group(&mut self, conversation_id: String, name: String) -> anyhow::Result<ActionResponse> {
        let resolved_conversation_id = self
            .resolve_cached_conversation_id(&conversation_id, Some("group"))
            .unwrap_or(conversation_id);
        let result = rename_group_with_client(self.ensure_client()?, &resolved_conversation_id, &name)?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn add_group_members(
        &mut self,
        conversation_id: String,
        members: Vec<String>,
    ) -> anyhow::Result<ActionResponse> {
        let resolved_conversation_id = self
            .resolve_cached_conversation_id(&conversation_id, Some("group"))
            .unwrap_or(conversation_id);
        let result =
            add_group_members_with_client(self.ensure_client()?, &resolved_conversation_id, &members)?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn remove_group_members(
        &mut self,
        conversation_id: String,
        members: Vec<String>,
    ) -> anyhow::Result<ActionResponse> {
        let resolved_conversation_id = self
            .resolve_cached_conversation_id(&conversation_id, Some("group"))
            .unwrap_or(conversation_id);
        let result = remove_group_members_with_client(
            self.ensure_client()?,
            &resolved_conversation_id,
            &members,
        )?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn leave_conversation(&mut self, conversation_id: String) -> anyhow::Result<ActionResponse> {
        let resolved_conversation_id = self
            .resolve_cached_conversation_id(&conversation_id, None)
            .unwrap_or(conversation_id);
        let result = leave_conversation_with_client(self.ensure_client()?, &resolved_conversation_id)?;
        Ok(ActionResponse {
            conversation_id: result.conversation_id,
            message_id: result.message_id,
        })
    }

    fn message_info(&mut self, message_id: String) -> anyhow::Result<MessageInfoResponse> {
        message_info_with_client(self.ensure_client()?, &message_id)
    }

    fn remember_conversation_items(&mut self, items: &[ConversationItem]) {
        self.conversation_cache = items
            .iter()
            .map(|item| ConversationLookup {
                id: item.id.clone(),
                name: item.name.clone(),
                kind: item.kind.clone(),
            })
            .collect();
    }

    fn resolve_cached_conversation_id(&self, query: &str, kind: Option<&str>) -> Option<String> {
        resolve_conversation_lookup_id(&self.conversation_cache, query, kind).ok()
    }
}

fn next_event_id() -> String {
    format!("evt-{}", EVENT_COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn send_event(events_tx: &broadcast::Sender<DaemonEventEnvelope>, payload: DaemonEventData) {
    let _ = events_tx.send(DaemonEventEnvelope {
        event_id: next_event_id(),
        payload,
    });
}

fn publish_snapshot_events(state: &HttpState, status: bool, conversations: bool) {
    let mut guard = state.app.lock().expect("lock daemon app");
    if status {
        if let Some(payload) = guard.next_status_event() {
            send_event(&state.events_tx, payload);
        }
    }
    if conversations {
        if let Some(payload) = guard.next_conversation_list_event() {
            send_event(&state.events_tx, payload);
        }
    }
}

fn publish_conversation_snapshot_now(state: &HttpState) {
    let payload = {
        let mut guard = state.app.lock().expect("lock daemon app");
        guard
            .conversation_list()
            .ok()
            .map(DaemonEventData::ConversationList)
    };
    if let Some(payload) = payload {
        send_event(&state.events_tx, payload);
    }
}

fn publish_conversation_updated_now(state: &HttpState, conversation_id: &str) {
    let payload = {
        let mut guard = state.app.lock().expect("lock daemon app");
        guard
            .group_info(conversation_id.to_owned())
            .ok()
            .map(|info| {
                DaemonEventData::ConversationUpdated(ConversationUpdatedEvent {
                    conversation_id: info.conversation_id,
                    name: info.name,
                    member_count: info.member_count,
                })
            })
    };
    if let Some(payload) = payload {
        send_event(&state.events_tx, payload);
    }
}

fn publish_group_members_updated_now(state: &HttpState, conversation_id: &str) {
    let payload = {
        let mut guard = state.app.lock().expect("lock daemon app");
        guard
            .group_members(conversation_id.to_owned(), 200)
            .ok()
            .map(|members| {
                DaemonEventData::GroupMembersUpdated(GroupMembersUpdatedEvent {
                    conversation_id: conversation_id.to_owned(),
                    members: members.items,
                })
            })
    };
    if let Some(payload) = payload {
        send_event(&state.events_tx, payload);
    }
}

async fn run_app<T>(
    state: &HttpState,
    request_summary: String,
    status_refresh: bool,
    conversations_refresh: bool,
    task: impl FnOnce(&mut DaemonApp) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T>
where
    T: Send + 'static,
{
    let data_dir = {
        let guard = state.app.lock().expect("lock daemon app");
        guard.data_dir.clone()
    };
    let started = Instant::now();
    daemon_log(
        &data_dir,
        "debug",
        format!("request payload={request_summary}"),
    );

    let result = tokio::task::block_in_place(|| {
        let mut guard = state.app.lock().expect("lock daemon app");
        task(&mut guard)
    });

    match result {
        Ok(value) => {
            daemon_log(
                &data_dir,
                "debug",
                format!("request ok elapsed_ms={}", started.elapsed().as_millis()),
            );
            if status_refresh || conversations_refresh {
                publish_snapshot_events(state, status_refresh, conversations_refresh);
            }
            Ok(value)
        }
        Err(err) => {
            daemon_log(
                &data_dir,
                "error",
                format!("request failed payload={} error={err:#}", request_summary),
            );
            Err(err)
        }
    }
}

async fn login_handler(
    State(state): State<HttpState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<StatusResponse>, ApiErrorResponse> {
    let status = run_app(
        &state,
        format!("login env={}", request.env),
        true,
        true,
        move |app| app.login(request.env, request.api_url, request.gateway_url),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(status))
}

async fn shutdown_handler(
    State(state): State<HttpState>,
) -> Result<StatusCode, (StatusCode, String)> {
    daemon_log(&state.app.lock().expect("lock daemon app").data_dir, "info", "shutdown requested by client");
    let _ = state.shutdown_tx.send(());
    Ok(StatusCode::NO_CONTENT)
}

async fn status_handler(
    State(state): State<HttpState>,
) -> Result<Json<StatusResponse>, ApiErrorResponse> {
    let status = run_app(&state, "get status".to_owned(), false, false, |app| app.status())
        .await
        .map_err(internal_error)?;
    Ok(Json(status))
}

async fn conversations_handler(
    State(state): State<HttpState>,
    Query(query): Query<ConversationsQuery>,
) -> Result<Json<ConversationListResponse>, ApiErrorResponse> {
    let conversations = run_app(
        &state,
        format!("list conversations kind={:?}", query.kind),
        false,
        true,
        move |app| app.list_conversations(query.kind),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(conversations))
}

async fn conversation_info_handler(
    State(state): State<HttpState>,
    AxumPath(conversation_id): AxumPath<String>,
) -> Result<Json<ConversationInfoResponse>, ApiErrorResponse> {
    let info = run_app(
        &state,
        format!("conversation info id={conversation_id}"),
        false,
        false,
        move |app| app.conversation_info(conversation_id),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(info))
}

async fn conversation_history_handler(
    State(state): State<HttpState>,
    AxumPath(conversation_id): AxumPath<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<HistoryResponse>, ApiErrorResponse> {
    let limit = query.limit.unwrap_or(50);
    let history = run_app(
        &state,
        format!("history id={conversation_id}"),
        false,
        false,
        move |app| app.history_snapshot(conversation_id, query.before_ns, limit),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(history))
}

async fn open_dm_handler(
    State(state): State<HttpState>,
    Json(request): Json<RecipientRequest>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let result = run_app(
        &state,
        format!("open dm recipient={}", request.recipient),
        false,
        false,
        move |app| app.open_dm(request.recipient),
    )
    .await
    .map_err(internal_error)?;
    publish_conversation_snapshot_now(&state);
    Ok(Json(result))
}

async fn send_dm_handler(
    State(state): State<HttpState>,
    Json(request): Json<RecipientMessageRequest>,
) -> Result<Json<SendDmResponse>, ApiErrorResponse> {
    let result = run_app(
        &state,
        format!("send dm recipient={}", request.recipient),
        false,
        false,
        move |app| app.send_dm(request.recipient, request.message, request.content_type),
    )
    .await
    .map_err(internal_error)?;
    publish_conversation_snapshot_now(&state);
    Ok(Json(result))
}

async fn create_group_handler(
    State(state): State<HttpState>,
    Json(request): Json<GroupCreateRequest>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let result = run_app(
        &state,
        format!("create group name={:?}", request.name),
        false,
        false,
        move |app| app.create_group(request.name, request.members),
    )
    .await
    .map_err(internal_error)?;
    publish_conversation_snapshot_now(&state);
    Ok(Json(result))
}

async fn send_group_handler(
    State(state): State<HttpState>,
    AxumPath(group_id): AxumPath<String>,
    Json(request): Json<SendMessageRequest>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let result = run_app(
        &state,
        format!("send group id={group_id}"),
        false,
        false,
        move |app| app.send_group(group_id, request.message, request.content_type),
    )
    .await
    .map_err(internal_error)?;
    publish_conversation_snapshot_now(&state);
    Ok(Json(result))
}

async fn reply_handler(
    State(state): State<HttpState>,
    AxumPath(message_id): AxumPath<String>,
    Json(request): Json<SendMessageRequest>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let result = run_app(
        &state,
        format!("reply message_id={message_id}"),
        false,
        true,
        move |app| app.reply(message_id, request.message, request.conversation_id),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(result))
}

async fn react_handler(
    State(state): State<HttpState>,
    AxumPath(message_id): AxumPath<String>,
    Json(request): Json<EmojiRequest>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let action = match request.action.as_deref() {
        Some("remove") => ReactionAction::Removed,
        Some("add") | None => ReactionAction::Added,
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiErrorBody {
                    error: ApiErrorDetail {
                        code: "invalid_reaction_action".to_owned(),
                        message: format!("unsupported reaction action: {other}"),
                    },
                }),
            ));
        }
    };
    let result = run_app(
        &state,
        format!("react message_id={message_id} action={action:?}"),
        false,
        true,
        move |app| app.react_with_action(message_id, request.emoji, action, request.conversation_id),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(result))
}

async fn group_members_handler(
    State(state): State<HttpState>,
    AxumPath(group_id): AxumPath<String>,
    Query(query): Query<MembersQuery>,
) -> Result<Json<GroupMembersResponse>, ApiErrorResponse> {
    let limit = query.limit.unwrap_or(200);
    let members = run_app(
        &state,
        format!("group members id={group_id}"),
        false,
        false,
        move |app| app.group_members(group_id, limit),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(members))
}

async fn group_info_handler(
    State(state): State<HttpState>,
    AxumPath(group_id): AxumPath<String>,
) -> Result<Json<GroupInfoResponse>, ApiErrorResponse> {
    let info = run_app(
        &state,
        format!("group info id={group_id}"),
        false,
        false,
        move |app| app.group_info(group_id),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(info))
}

async fn rename_group_handler(
    State(state): State<HttpState>,
    AxumPath(group_id): AxumPath<String>,
    Json(request): Json<RenameGroupRequest>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let event_conversation_id = group_id.clone();
    let result = run_app(
        &state,
        format!("rename group id={group_id}"),
        false,
        false,
        move |app| app.rename_group(group_id, request.name),
    )
    .await
    .map_err(internal_error)?;
    publish_conversation_updated_now(&state, &event_conversation_id);
    publish_group_members_updated_now(&state, &event_conversation_id);
    publish_conversation_snapshot_now(&state);
    Ok(Json(result))
}

async fn add_group_members_handler(
    State(state): State<HttpState>,
    AxumPath(group_id): AxumPath<String>,
    Json(request): Json<GroupMembersUpdateRequest>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let event_conversation_id = group_id.clone();
    let result = run_app(
        &state,
        format!("add group members id={group_id}"),
        false,
        false,
        move |app| app.add_group_members(group_id, request.members),
    )
    .await
    .map_err(internal_error)?;
    publish_conversation_updated_now(&state, &event_conversation_id);
    publish_group_members_updated_now(&state, &event_conversation_id);
    publish_conversation_snapshot_now(&state);
    Ok(Json(result))
}

async fn remove_group_members_handler(
    State(state): State<HttpState>,
    AxumPath(group_id): AxumPath<String>,
    Json(request): Json<GroupMembersUpdateRequest>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let event_conversation_id = group_id.clone();
    let result = run_app(
        &state,
        format!("remove group members id={group_id}"),
        false,
        false,
        move |app| app.remove_group_members(group_id, request.members),
    )
    .await
    .map_err(internal_error)?;
    publish_conversation_updated_now(&state, &event_conversation_id);
    publish_conversation_snapshot_now(&state);
    Ok(Json(result))
}

async fn leave_conversation_handler(
    State(state): State<HttpState>,
    AxumPath(conversation_id): AxumPath<String>,
) -> Result<Json<ActionResponse>, ApiErrorResponse> {
    let event_conversation_id = conversation_id.clone();
    let result = run_app(
        &state,
        format!("leave conversation id={conversation_id}"),
        false,
        true,
        move |app| app.leave_conversation(conversation_id),
    )
    .await
    .map_err(internal_error)?;
    publish_conversation_updated_now(&state, &event_conversation_id);
    publish_conversation_snapshot_now(&state);
    Ok(Json(result))
}

async fn message_info_handler(
    State(state): State<HttpState>,
    AxumPath(message_id): AxumPath<String>,
) -> Result<Json<MessageInfoResponse>, ApiErrorResponse> {
    let info = run_app(
        &state,
        format!("message info id={message_id}"),
        false,
        false,
        move |app| app.message_info(message_id),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(info))
}

fn internal_error(err: anyhow::Error) -> ApiErrorResponse {
    let message = format!("{err:#}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiErrorBody {
            error: ApiErrorDetail {
                code: classify_error_code(&message).to_owned(),
                message,
            },
        }),
    )
}

fn classify_error_code(message: &str) -> &'static str {
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("not found") {
        "not_found"
    } else if lowered.contains("ambiguous") {
        "ambiguous_resource"
    } else if lowered.contains("temporarily disabled")
        || lowered.contains("not supported")
        || lowered.contains("unstable")
    {
        "unsupported_operation"
    } else if lowered.contains("invalid")
        || lowered.contains("missing")
        || lowered.contains("unavailable")
    {
        "invalid_request"
    } else if lowered.contains("rate limit") || lowered.contains("resource has been exhausted") {
        "rate_limited"
    } else {
        "internal_error"
    }
}

fn sse_event_from_envelope(envelope: &DaemonEventEnvelope) -> Event {
    let data = serde_json::to_string(envelope).unwrap_or_else(|err| {
        serde_json::json!({
            "event_id": next_event_id(),
            "payload": {
                "type": "daemon_error",
                "message": format!("encode daemon event failed: {err}")
            }
        })
        .to_string()
    });
    Event::default().event("daemon_event").id(envelope.event_id.clone()).data(data)
}

async fn app_events_handler(
    State(state): State<HttpState>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let data_dir = {
        let guard = state.app.lock().expect("lock daemon app");
        guard.data_dir.clone()
    };
    daemon_log(&data_dir, "info", "app event stream opened");

    let (event_tx, event_rx) = mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();
    let mut subscription = state.events_tx.subscribe();
    let app = state.app.clone();
    let events_tx = state.events_tx.clone();

    tokio::spawn(async move {
        let initial_events = tokio::task::spawn_blocking(move || {
            let mut guard = app.lock().expect("lock daemon app");
            let mut snapshots = Vec::new();
            if let Ok(status) = guard.status() {
                snapshots.push(DaemonEventEnvelope {
                    event_id: next_event_id(),
                    payload: DaemonEventData::Status(status),
                });
            }
            if let Ok(conversations) = guard.conversation_list() {
                snapshots.push(DaemonEventEnvelope {
                    event_id: next_event_id(),
                    payload: DaemonEventData::ConversationList(conversations),
                });
            }
            snapshots
        })
        .await
        .unwrap_or_default();

        for envelope in initial_events {
            let _ = event_tx.send(Ok(sse_event_from_envelope(&envelope)));
        }

        loop {
            match subscription.recv().await {
                Ok(envelope) => {
                    let _ = event_tx.send(Ok(sse_event_from_envelope(&envelope)));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    send_event(
                        &events_tx,
                        DaemonEventData::DaemonError {
                            message: format!("app event stream lagged by {skipped} messages"),
                        },
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    Sse::new(UnboundedReceiverStream::new(event_rx)).keep_alive(KeepAlive::default())
}

async fn history_events_handler(
    State(state): State<HttpState>,
    AxumPath(conversation_id): AxumPath<String>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let data_dir = {
        let guard = state.app.lock().expect("lock daemon app");
        guard.data_dir.clone()
    };
    daemon_log(
        &data_dir,
        "info",
        format!("history event stream opened conversation={conversation_id}"),
    );

    let (event_tx, event_rx) = mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();
    let data_dir_for_thread = data_dir.clone();
    std::thread::spawn(move || {
        let stream_result = watch_history_with_kind(
            &data_dir_for_thread,
            &conversation_id,
            None,
            |item| {
                let envelope = DaemonEventEnvelope {
                    event_id: next_event_id(),
                    payload: DaemonEventData::HistoryItem {
                        conversation_id: conversation_id.clone(),
                        item,
                    },
                };
                let event = sse_event_from_envelope(&envelope);
                let _ = event_tx.send(Ok(event));
            },
        );
        if let Err(err) = stream_result {
            let envelope = DaemonEventEnvelope {
                event_id: next_event_id(),
                payload: DaemonEventData::DaemonError {
                    message: format!("{err:#}"),
                },
            };
            let event = sse_event_from_envelope(&envelope);
            let _ = event_tx.send(Ok(event));
        }
    });

    Sse::new(UnboundedReceiverStream::new(event_rx)).keep_alive(KeepAlive::default())
}

pub async fn serve(data_dir: &Path) -> anyhow::Result<()> {
    init_tracing();
    fs::create_dir_all(data_dir).context("create daemon data dir")?;
    daemon_log(
        data_dir,
        "info",
        format!(
            "daemon serve starting data_dir={} pid={}",
            data_dir.display(),
            std::process::id()
        ),
    );
    fs::write(pid_path(data_dir), std::process::id().to_string()).context("write daemon pid")?;
    let listener = TcpListener::bind("127.0.0.1:0").await.context("bind daemon tcp listener")?;
    let addr: SocketAddr = listener.local_addr().context("read daemon local addr")?;
    fs::write(addr_path(data_dir), addr.to_string()).context("write daemon addr")?;
    let _cleanup = DaemonFilesGuard::new(None, addr_path(data_dir), pid_path(data_dir));
    daemon_log(data_dir, "info", format!("listening on http://{}", addr));

    let app = Arc::new(Mutex::new(DaemonApp::new(data_dir.to_path_buf())));
    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<()>();
    let (events_tx, _) = broadcast::channel::<DaemonEventEnvelope>(128);
    let monitor_app = app.clone();
    let monitor_events_tx = events_tx.clone();
    let monitor_handle = tokio::spawn(async move {
        loop {
            // Background monitor that periodically pushes fresh status and conversation-list
            // snapshots to global SSE subscribers when those views change.
            let snapshots = tokio::task::spawn_blocking({
                let app = monitor_app.clone();
                move || {
                    let mut guard = app.lock().expect("lock daemon app");
                    let status = guard.next_status_event();
                    let conversations = guard.next_conversation_list_event();
                    (status, conversations)
                }
            })
            .await;

            match snapshots {
                Ok((status, conversations)) => {
                    if let Some(payload) = status {
                        send_event(&monitor_events_tx, payload);
                    }
                    if let Some(payload) = conversations {
                        send_event(&monitor_events_tx, payload);
                    }
                }
                Err(err) => {
                    send_event(
                        &monitor_events_tx,
                        DaemonEventData::DaemonError {
                            message: format!("snapshot monitor join error: {err}"),
                        },
                    );
                }
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
    let router = Router::new()
        .route("/v1/login", post(login_handler))
        .route("/v1/shutdown", post(shutdown_handler))
        .route("/v1/status", get(status_handler))
        .route("/v1/conversations", get(conversations_handler))
        .route("/v1/conversations/{conversation_id}", get(conversation_info_handler))
        .route(
            "/v1/conversations/{conversation_id}/history",
            get(conversation_history_handler),
        )
        .route("/v1/direct-message/open", post(open_dm_handler))
        .route("/v1/direct-message/send", post(send_dm_handler))
        .route("/v1/groups", post(create_group_handler))
        .route(
            "/v1/groups/{group_id}",
            get(group_info_handler).patch(rename_group_handler),
        )
        .route(
            "/v1/groups/{group_id}/members",
            get(group_members_handler)
                .post(add_group_members_handler)
                .delete(remove_group_members_handler),
        )
        .route("/v1/groups/{group_id}/send", post(send_group_handler))
        .route("/v1/conversations/{conversation_id}/leave", post(leave_conversation_handler))
        .route("/v1/messages/{message_id}", get(message_info_handler))
        .route("/v1/messages/{message_id}/reply", post(reply_handler))
        .route("/v1/messages/{message_id}/react", post(react_handler))
        .route("/v1/events", get(app_events_handler))
        .route("/v1/conversations/{conversation_id}/events", get(history_events_handler))
        .with_state(HttpState {
            app,
            shutdown_tx: shutdown_tx.clone(),
            events_tx: events_tx.clone(),
        });

    let result = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.recv().await;
        })
        .await
        .context("serve axum daemon");

    monitor_handle.abort();
    let _ = monitor_handle.await;
    result?;

    daemon_log(data_dir, "info", "daemon serve stopped");
    Ok(())
}

struct DaemonFilesGuard {
    socket_path: Option<PathBuf>,
    addr_path: PathBuf,
    pid_path: PathBuf,
}

impl DaemonFilesGuard {
    fn new(socket_path: Option<PathBuf>, addr_path: PathBuf, pid_path: PathBuf) -> Self {
        Self {
            socket_path,
            addr_path,
            pid_path,
        }
    }
}

impl Drop for DaemonFilesGuard {
    fn drop(&mut self) {
        if let Some(socket_path) = &self.socket_path {
            let _ = fs::remove_file(socket_path);
        }
        let _ = fs::remove_file(&self.addr_path);
        let _ = fs::remove_file(&self.pid_path);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConversationLookup, ConversationSummary, GroupUpdated, GroupUpdatedInbox,
        MetadataFieldChange, resolve_conversation_lookup_id, resolve_message_id,
        sort_conversation_summaries, summarize_decoded_content,
    };
    use prost::Message;
    use xmtp::content::{
        Content, ContentTypeId, EncodedContent, Reaction, ReactionAction, ReactionSchema, Reply,
    };

    #[test]
    fn resolve_conversation_id_accepts_short_display_id() {
        let ids = vec![
            ConversationLookup {
                id: "12345678aaaabbbbccccddddeeeeffff00001111222233334444555566667777".to_owned(),
                name: Some("group-1".to_owned()),
                kind: "group".to_owned(),
            },
            ConversationLookup {
                id: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_owned(),
                name: Some("group-2".to_owned()),
                kind: "group".to_owned(),
            },
        ];

        let resolved =
            resolve_conversation_lookup_id(&ids, "12345678....66667777", None).expect("resolved id");

        assert_eq!(resolved, ids[0].id);
    }

    #[test]
    fn resolve_conversation_id_accepts_unique_prefix() {
        let ids = vec![
            ConversationLookup {
                id: "12345678aaaabbbbccccddddeeeeffff00001111222233334444555566667777".to_owned(),
                name: Some("group-1".to_owned()),
                kind: "group".to_owned(),
            },
            ConversationLookup {
                id: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_owned(),
                name: Some("group-2".to_owned()),
                kind: "group".to_owned(),
            },
        ];

        let resolved = resolve_conversation_lookup_id(&ids, "abcdef01", None).expect("resolved id");

        assert_eq!(resolved, ids[1].id);
    }

    #[test]
    fn resolve_conversation_id_accepts_exact_name() {
        let ids = vec![
            ConversationLookup {
                id: "12345678aaaabbbbccccddddeeeeffff00001111222233334444555566667777".to_owned(),
                name: Some("Andelf".to_owned()),
                kind: "group".to_owned(),
            },
            ConversationLookup {
                id: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_owned(),
                name: Some("team".to_owned()),
                kind: "group".to_owned(),
            },
        ];

        let resolved = resolve_conversation_lookup_id(&ids, "Andelf", None).expect("resolved id");

        assert_eq!(resolved, ids[0].id);
    }

    #[test]
    fn resolve_conversation_id_rejects_ambiguous_name() {
        let ids = vec![
            ConversationLookup {
                id: "12345678aaaabbbbccccddddeeeeffff00001111222233334444555566667777".to_owned(),
                name: Some("dup".to_owned()),
                kind: "group".to_owned(),
            },
            ConversationLookup {
                id: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_owned(),
                name: Some("dup".to_owned()),
                kind: "group".to_owned(),
            },
        ];

        let error = resolve_conversation_lookup_id(&ids, "dup", None).expect_err("ambiguous name");

        assert!(error.to_string().contains("conversation name dup is ambiguous"));
    }

    #[test]
    fn conversation_summaries_are_sorted_by_last_message_desc() {
        let mut items = vec![
            ConversationSummary {
                id: "conv-1".into(),
                kind: "dm".into(),
                name: None,
                dm_peer_inbox_id: Some("peer-1".into()),
                last_message_ns: Some(10),
            },
            ConversationSummary {
                id: "conv-2".into(),
                kind: "group".into(),
                name: Some("group".into()),
                dm_peer_inbox_id: None,
                last_message_ns: None,
            },
            ConversationSummary {
                id: "conv-3".into(),
                kind: "dm".into(),
                name: None,
                dm_peer_inbox_id: Some("peer-3".into()),
                last_message_ns: Some(30),
            },
        ];

        sort_conversation_summaries(&mut items);

        assert_eq!(items[0].id, "conv-3");
        assert_eq!(items[1].id, "conv-1");
        assert_eq!(items[2].id, "conv-2");
    }

    #[test]
    fn resolve_message_id_accepts_short_display_id() {
        let ids = vec![
            "7f896ed3a1b2c3d4e5f60718293a4b5c6d7e8f901234567890abcdef78c9669d".to_owned(),
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_owned(),
        ];

        let resolved = resolve_message_id(&ids, "7f896ed3....78c9669d").expect("resolved id");

        assert_eq!(resolved, ids[0]);
    }

    #[test]
    fn summarize_decoded_content_formats_reaction_and_reply() {
        let reaction = summarize_decoded_content(&Content::Reaction(Reaction {
            reference: "abcd1234".to_owned(),
            reference_inbox_id: "inbox-1".to_owned(),
            action: ReactionAction::Added,
            content: "👍".to_owned(),
            schema: ReactionSchema::Unicode,
        }));
        let reply = summarize_decoded_content(&Content::Reply(Reply {
            reference: "dcba4321".to_owned(),
            reference_inbox_id: Some("inbox-2".to_owned()),
            content: EncodedContent {
                r#type: Some(ContentTypeId {
                    authority_id: "xmtp.org".to_owned(),
                    type_id: "text".to_owned(),
                    version_major: 1,
                    version_minor: 0,
                }),
                content: b"hello world".to_vec(),
                ..EncodedContent::default()
            },
        }));

        assert_eq!(reaction, "reacted 👍 to abcd1234");
        assert_eq!(reply, "hello world");
    }

    #[test]
    fn summarize_decoded_content_formats_unknown_without_dumping_raw_bytes() {
        let summary = summarize_decoded_content(&Content::Unknown {
            content_type: "xmtp.org/group_updated:1.0".to_owned(),
            raw: vec![1, 2, 3, 4],
        });

        assert_eq!(summary, "unsupported xmtp.org/group_updated:1.0");
    }

    #[test]
    fn summarize_decoded_content_formats_group_updated_friendly_summary() {
        let inner = GroupUpdated {
            initiated_by_inbox_id: "inbox-1".to_owned(),
            added_inboxes: vec![GroupUpdatedInbox {
                inbox_id: "inbox-2".to_owned(),
            }],
            removed_inboxes: Vec::new(),
            metadata_field_changes: vec![MetadataFieldChange {
                field_name: "group_name".to_owned(),
                old_value: Some("Old".to_owned()),
                new_value: Some("New".to_owned()),
            }],
            left_inboxes: Vec::new(),
        };
        let encoded = xmtp::content::EncodedContent {
            r#type: None,
            parameters: Default::default(),
            fallback: None,
            content: inner.encode_to_vec(),
            compression: None,
        };

        let summary = summarize_decoded_content(&Content::Unknown {
            content_type: "xmtp.org/group_updated:1.0".to_owned(),
            raw: encoded.encode_to_vec(),
        });

        assert_eq!(summary, "added 1 member, renamed to New");
    }

    #[test]
    fn next_event_id_is_unique_and_non_empty() {
        let first = super::next_event_id();
        let second = super::next_event_id();

        assert!(!first.is_empty());
        assert!(!second.is_empty());
        assert_ne!(first, second);
        assert!(first.starts_with("evt-"));
        assert!(second.starts_with("evt-"));
    }
}
