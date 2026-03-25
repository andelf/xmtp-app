use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::Context;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use xmtp::content::{Content, ReactionAction};
use xmtp::{AlloySigner, Client, CreateGroupOptions, Env, Recipient};
use xmtp_config::{AppConfig, load_config, save_config};
use xmtp_core::{ConnectionState, DaemonState};
use xmtp_ipc::{
    ActionResponse, ConversationInfoResponse, ConversationItem, ConversationListResponse,
    DaemonRequest, DaemonResponse, DaemonResponseData, GroupInfoResponse, GroupMemberItem,
    GroupMembersResponse, HistoryItem, HistoryResponse, MessageInfoResponse, ReactionDetail,
    SendDmResponse, StatusResponse,
};
use xmtp_logging::append_daemon_event;
use xmtp_store::{load_state, save_state};

static TRACING_INIT: OnceLock<()> = OnceLock::new();

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

pub fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join("daemon.sock")
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
    let client = builder.build(signer).context("build XMTP client")?;
    Ok(client)
}

fn open_client_with_login(data_dir: &Path, env: &str) -> anyhow::Result<Client> {
    let signer_hex = load_or_create_signer_key_hex(data_dir)?;
    let signer = AlloySigner::from_hex(&signer_hex).context("load signer from hex")?;
    let mut config = load_config(&data_dir.join("config.json"))?;
    if config.xmtp_env != env {
        config.xmtp_env = env.to_owned();
        save_config(&data_dir.join("config.json"), &config)?;
    }
    build_client(&config, &signer, data_dir)
}

pub fn configure_runtime(data_dir: &Path, env: &str, api_url: Option<&str>) -> anyhow::Result<()> {
    let mut config = load_config(&data_dir.join("config.json"))?;
    config.xmtp_env = env.to_owned();
    config.api_url = api_url.map(str::to_owned);
    save_config(&data_dir.join("config.json"), &config)
}

pub struct ConversationSummary {
    pub id: String,
    pub kind: String,
    pub name: Option<String>,
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
}

pub fn list_conversations(data_dir: &Path) -> anyhow::Result<Vec<ConversationSummary>> {
    let client = open_existing_client(data_dir)?;
    list_conversations_with_client(&client, None)
}

pub fn send_dm(data_dir: &Path, recipient: &str, text: &str) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    send_dm_with_client(&client, recipient, text)
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
    history_with_client(&client, conversation_id, kind)
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
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    send_group_with_client(&client, conversation_id, text)
}

pub fn group_members(
    data_dir: &Path,
    conversation_id: &str,
) -> anyhow::Result<Vec<GroupMemberItem>> {
    let client = open_existing_client(data_dir)?;
    group_members_with_client(&client, conversation_id)
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
    reply_with_client(&client, message_id, text)
}

pub fn react(data_dir: &Path, message_id: &str, emoji: &str) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    react_with_client(&client, message_id, emoji)
}

pub fn unreact(
    data_dir: &Path,
    message_id: &str,
    emoji: &str,
) -> anyhow::Result<SendMessageResult> {
    let client = open_existing_client(data_dir)?;
    unreact_with_client(&client, message_id, emoji)
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
        });
    }
    Ok(summaries)
}

fn send_dm_with_client(
    client: &Client,
    recipient: &str,
    text: &str,
) -> anyhow::Result<SendMessageResult> {
    let recipient = Recipient::parse(recipient);
    let conversation = client.dm(&recipient).context("create or find DM")?;
    let message_id = conversation.send_text(text).context("send DM text")?;
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
) -> anyhow::Result<SendMessageResult> {
    client.sync_welcomes().context("sync welcomes")?;
    client.sync_all(&[]).context("sync conversations")?;
    let conversations = client.conversations().context("list conversations")?;
    let resolved_id = resolve_conversation_query(&conversations, conversation_id, Some("group"))?;
    let conversation = conversations
        .into_iter()
        .find(|conversation| conversation.id() == resolved_id)
        .context("conversation not found")?;
    let message_id = conversation.send_text(text).context("send group text")?;
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
) -> anyhow::Result<Vec<GroupMemberItem>> {
    let conversation = find_conversation_by_id(client, conversation_id)?;
    let members = conversation.members().context("list group members")?;
    Ok(members
        .into_iter()
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
        creator_inbox_id: String::new(),
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
    anyhow::bail!(
        "leave conversation is temporarily disabled because the current XMTP SDK path is unstable for this runtime: {}",
        conversation.id()
    );
}

fn reply_with_client(client: &Client, message_id: &str, text: &str) -> anyhow::Result<SendMessageResult> {
    let (conversation, resolved_message_id) = find_message_conversation(client, message_id)?;
    let sent_message_id = conversation
        .send_text_reply(&resolved_message_id, text)
        .context("send reply text")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: sent_message_id,
    })
}

fn react_with_client(client: &Client, message_id: &str, emoji: &str) -> anyhow::Result<SendMessageResult> {
    let (conversation, resolved_message_id) = find_message_conversation(client, message_id)?;
    let sent_message_id = conversation
        .send_reaction(&resolved_message_id, emoji, ReactionAction::Added)
        .context("send reaction")?;
    Ok(SendMessageResult {
        conversation_id: conversation.id(),
        message_id: sent_message_id,
    })
}

fn unreact_with_client(
    client: &Client,
    message_id: &str,
    emoji: &str,
) -> anyhow::Result<SendMessageResult> {
    let (conversation, resolved_message_id) = find_message_conversation(client, message_id)?;
    let sent_message_id = conversation
        .send_reaction(&resolved_message_id, emoji, ReactionAction::Removed)
        .context("remove reaction")?;
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
    client: &Client,
    conversation_id: &str,
    kind: Option<&str>,
) -> anyhow::Result<Vec<HistoryEntry>> {
    let conversation = find_conversation_by_id_with_kind(client, conversation_id, kind)?;
    conversation.sync().context("sync conversation")?;
    let messages = conversation.messages().context("list messages")?;
    let mut entries = Vec::with_capacity(messages.len());
    for message in messages {
        entries.push(history_entry_from_message(&message));
    }
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
        Ok(Content::Unknown { content_type, .. }) => (
            "unknown".to_owned(),
            message
                .fallback
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("type=unknown content_type={content_type}")),
            None,
            None,
            None,
            None,
        ),
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
        attached_reactions: Vec::<ReactionDetail>::new(),
    }
}

fn summarize_message_content(message: &xmtp::conversation::Message) -> String {
    match message.decode() {
        Ok(Content::Unknown { content_type, .. }) => message
            .fallback
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("type=unknown content_type={content_type}")),
        Ok(decoded) => summarize_decoded_content(&decoded),
        Err(_) => message
            .fallback
            .clone()
            .unwrap_or_else(|| "<undecodable>".to_owned()),
    }
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
    resolve_conversation_id(&lookups, query, kind)
}

struct ConversationLookup {
    id: String,
    name: Option<String>,
    kind: String,
}

fn resolve_conversation_id(
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
        Content::Reply(reply) => format!("reply to {}", short_id(&reply.reference)),
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
        Content::Unknown { content_type, .. } => format!("unsupported {content_type}"),
    }
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
}

impl DaemonApp {
    fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            client: None,
        }
    }

    fn status(&self) -> anyhow::Result<StatusResponse> {
        let state = load_state(&self.data_dir.join("state.json"))?;
        Ok(StatusResponse {
            daemon_state: state.daemon_state,
            connection_state: state.connection_state,
            inbox_id: state.inbox_id,
            installation_id: state.installation_id,
        })
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

    fn handle(&mut self, request: DaemonRequest) -> anyhow::Result<(DaemonResponse, bool)> {
        let mut should_continue = true;
        let result = match request {
            DaemonRequest::GetStatus => {
                DaemonResponseData::Status(self.status()?)
            }
            DaemonRequest::Shutdown => {
                should_continue = false;
                return Ok((
                    DaemonResponse {
                        ok: true,
                        result: None,
                        error: None,
                    },
                    should_continue,
                ));
            }
            DaemonRequest::Reply { message_id, message } => {
                let result = reply_with_client(self.ensure_client()?, &message_id, &message)?;
                DaemonResponseData::Reply(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::React { message_id, emoji } => {
                let result = react_with_client(self.ensure_client()?, &message_id, &emoji)?;
                DaemonResponseData::React(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::Unreact { message_id, emoji } => {
                let result = unreact_with_client(self.ensure_client()?, &message_id, &emoji)?;
                DaemonResponseData::Unreact(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::CreateGroup { name, members } => {
                let result = create_group_with_client(self.ensure_client()?, name, &members)?;
                DaemonResponseData::CreateGroup(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::SendGroup {
                conversation_id,
                message,
            } => {
                let result =
                    send_group_with_client(self.ensure_client()?, &conversation_id, &message)?;
                DaemonResponseData::SendGroup(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::Login { env, api_url } => {
                configure_runtime(&self.data_dir, &env, api_url.as_deref())?;
                let client = open_client_with_login(&self.data_dir, &env)?;
                let runtime = RuntimeInfo {
                    inbox_id: client.inbox_id().context("get inbox id")?,
                    installation_id: client.installation_id().context("get installation id")?,
                };
                let mut state = load_state(&self.data_dir.join("state.json"))?;
                state.daemon_state = DaemonState::Running;
                state.connection_state = ConnectionState::Connected;
                state.inbox_id = Some(runtime.inbox_id.clone());
                state.installation_id = Some(runtime.installation_id.clone());
                save_state(&self.data_dir.join("state.json"), &state)?;
                self.client = Some(client);
                DaemonResponseData::Status(self.status()?)
            }
            DaemonRequest::ListConversations { kind } => {
                let items = list_conversations_with_client(self.ensure_client()?, kind.as_deref())?
                    .into_iter()
                    .map(|item| ConversationItem {
                        id: item.id,
                        kind: item.kind,
                        name: item.name,
                    })
                    .collect();
                DaemonResponseData::ConversationList(ConversationListResponse { items })
            }
            DaemonRequest::GroupMembers { conversation_id } => {
                let items = group_members_with_client(self.ensure_client()?, &conversation_id)?;
                DaemonResponseData::GroupMembers(GroupMembersResponse { items })
            }
            DaemonRequest::RenameGroup {
                conversation_id,
                name,
            } => {
                let result =
                    rename_group_with_client(self.ensure_client()?, &conversation_id, &name)?;
                DaemonResponseData::RenameGroup(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::AddGroupMembers {
                conversation_id,
                members,
            } => {
                let result = add_group_members_with_client(
                    self.ensure_client()?,
                    &conversation_id,
                    &members,
                )?;
                DaemonResponseData::AddGroupMembers(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::RemoveGroupMembers {
                conversation_id,
                members,
            } => {
                let result = remove_group_members_with_client(
                    self.ensure_client()?,
                    &conversation_id,
                    &members,
                )?;
                DaemonResponseData::RemoveGroupMembers(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::LeaveConversation { conversation_id } => {
                let result =
                    leave_conversation_with_client(self.ensure_client()?, &conversation_id)?;
                DaemonResponseData::LeaveConversation(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::GroupInfo { conversation_id } => {
                let info = group_info_with_client(self.ensure_client()?, &conversation_id)?;
                DaemonResponseData::GroupInfo(info)
            }
            DaemonRequest::ConversationInfo { conversation_id } => {
                let info = conversation_info_with_client(self.ensure_client()?, &conversation_id)?;
                DaemonResponseData::ConversationInfo(info)
            }
            DaemonRequest::MessageInfo { message_id } => {
                let info = message_info_with_client(self.ensure_client()?, &message_id)?;
                DaemonResponseData::MessageInfo(info)
            }
            DaemonRequest::WatchHistory { .. } => {
                anyhow::bail!("watch history must be handled by the streaming path")
            }
            DaemonRequest::SendDm { recipient, message } => {
                let result = send_dm_with_client(self.ensure_client()?, &recipient, &message)?;
                DaemonResponseData::SendDm(SendDmResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::OpenDm { recipient } => {
                let result = open_dm_with_client(self.ensure_client()?, &recipient)?;
                DaemonResponseData::OpenDm(ActionResponse {
                    conversation_id: result.conversation_id,
                    message_id: result.message_id,
                })
            }
            DaemonRequest::History { conversation_id } => {
                let items = history_with_client(self.ensure_client()?, &conversation_id, None)?
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
                        attached_reactions: Vec::new(),
                    })
                    .collect();
                DaemonResponseData::History(HistoryResponse { items })
            }
        };

        Ok((
            DaemonResponse {
                ok: true,
                result: Some(result),
                error: None,
            },
            should_continue,
        ))
    }
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
    let socket_path = socket_path(data_dir);
    if socket_path.exists() {
        daemon_log(
            data_dir,
            "warn",
            format!("removing stale socket {}", socket_path.display()),
        );
        fs::remove_file(&socket_path).context("remove stale socket")?;
    }
    fs::write(pid_path(data_dir), std::process::id().to_string()).context("write daemon pid")?;
    let _cleanup = DaemonFilesGuard::new(socket_path.clone(), pid_path(data_dir));
    let listener = UnixListener::bind(&socket_path).context("bind unix socket")?;
    daemon_log(
        data_dir,
        "info",
        format!("listening on socket {}", socket_path.display()),
    );
    let app = Arc::new(Mutex::new(DaemonApp::new(data_dir.to_path_buf())));
    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<()>();
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                daemon_log(data_dir, "info", "shutdown signal received");
                break
            },
            accept = listener.accept() => {
                let stream = match accept.context("accept unix socket connection") {
                    Ok((stream, _addr)) => {
                        daemon_log(data_dir, "debug", "accepted unix socket connection");
                        stream
                    },
                    Err(err) => {
                        daemon_log(data_dir, "error", format!("accept failed: {err:#}"));
                        continue;
                    }
                };
                let app = Arc::clone(&app);
                let app_for_error = Arc::clone(&app);
                let shutdown_tx = shutdown_tx.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_stream(stream, app, shutdown_tx).await {
                        let data_dir = {
                            let guard = app_for_error.lock().expect("lock daemon app");
                            guard.data_dir.clone()
                        };
                        daemon_log(&data_dir, "error", format!("stream failed: {err:#}"));
                    }
                });
            }
        }
    }
    daemon_log(data_dir, "info", "daemon serve stopped");
    Ok(())
}

async fn handle_stream(
    stream: UnixStream,
    app: Arc<Mutex<DaemonApp>>,
    shutdown_tx: mpsc::UnboundedSender<()>,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await.context("read request line")?;
    let request: xmtp_ipc::IpcEnvelope<DaemonRequest> =
        serde_json::from_str(&line).context("decode daemon request")?;
    let data_dir = {
        let guard = app.lock().expect("lock daemon app");
        guard.data_dir.clone()
    };
    let request_summary = format!("{:?}", request.payload);
    daemon_log(
        &data_dir,
        "debug",
        format!("request id={} payload={}", request.request_id, request_summary),
    );
    if let DaemonRequest::WatchHistory { conversation_id } = request.payload.clone() {
        daemon_log(
            &data_dir,
            "info",
            format!(
                "watch history opened request_id={} conversation={}",
                request.request_id,
                conversation_id
            ),
        );
        stream_watch_history(
            &mut write_half,
            request.version,
            &request.request_id,
            data_dir.clone(),
            &conversation_id,
        )
        .await?;
        daemon_log(
            &data_dir,
            "info",
            format!(
                "watch history closed request_id={} elapsed_ms={}",
                request.request_id,
                started.elapsed().as_millis()
            ),
        );
        return Ok(());
    }

    let (response, should_continue) = match tokio::task::block_in_place(|| {
        let mut guard = app.lock().expect("lock daemon app");
        guard.handle(request.payload)
    }) {
        Ok(result) => result,
        Err(err) => (
            DaemonResponse {
                ok: false,
                result: None,
                error: Some(format!("{err:#}")),
            },
            true,
        ),
    };
    if !response.ok {
        daemon_log(
            &data_dir,
            "error",
            format!(
                "request failed id={} payload={} error={}",
                request.request_id,
                request_summary,
                response.error.clone().unwrap_or_default()
            ),
        );
    } else {
        daemon_log(
            &data_dir,
            "debug",
            format!(
                "request ok id={} elapsed_ms={}",
                request.request_id,
                started.elapsed().as_millis()
            ),
        );
    }

    let envelope = xmtp_ipc::IpcEnvelope {
        version: request.version,
        request_id: request.request_id,
        payload: response,
    };

    let json = serde_json::to_string(&envelope).context("encode daemon response")?;
    write_half.write_all(json.as_bytes()).await.context("write response")?;
    write_half.write_all(b"\n").await.context("write newline")?;
    write_half.flush().await.context("flush response")?;
    if !should_continue {
        daemon_log(&data_dir, "info", "shutdown requested by client");
        let _ = shutdown_tx.send(());
    }
    Ok(())
}

async fn stream_watch_history(
    write_half: &mut tokio::net::unix::OwnedWriteHalf,
    version: u32,
    request_id: &str,
    data_dir: PathBuf,
    conversation_id: &str,
) -> anyhow::Result<()> {
    daemon_log(
        &data_dir,
        "debug",
        format!("sending watch ack request_id={request_id}"),
    );
    let ack = xmtp_ipc::IpcEnvelope {
        version,
        request_id: request_id.to_owned(),
        payload: DaemonResponse {
            ok: true,
            result: None,
            error: None,
        },
    };
    let json = serde_json::to_string(&ack).context("encode watch ack")?;
    write_half.write_all(json.as_bytes()).await.context("write watch ack")?;
    write_half.write_all(b"\n").await.context("write watch ack newline")?;
    write_half.flush().await.context("flush watch ack")?;

    let data_dir_for_stream = data_dir.clone();
    let conversation = conversation_id.to_owned();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<anyhow::Result<HistoryItem>>();
    std::thread::spawn(move || {
        let result = watch_history_with_kind(&data_dir_for_stream, &conversation, None, |item| {
            let _ = event_tx.send(Ok(item));
        });
        if let Err(err) = result {
            let _ = event_tx.send(Err(err));
        }
    });

    while let Some(item) = event_rx.recv().await {
        let item = item?;
        daemon_log(
            &data_dir,
            "debug",
            format!(
                "watch event request_id={} conversation={} message={}",
                request_id,
                conversation_id,
                item.message_id
            ),
        );
        let envelope = xmtp_ipc::IpcEnvelope {
            version,
            request_id: request_id.to_owned(),
            payload: DaemonResponse {
                ok: true,
                result: Some(DaemonResponseData::HistoryEvent(
                    xmtp_ipc::HistoryEventResponse { item },
                )),
                error: None,
            },
        };
        let json = serde_json::to_string(&envelope).context("encode history event")?;
        if let Err(err) = write_half.write_all(json.as_bytes()).await {
            if err.kind() == std::io::ErrorKind::BrokenPipe {
                break;
            }
            return Err(err).context("write history event");
        }
        if let Err(err) = write_half.write_all(b"\n").await {
            if err.kind() == std::io::ErrorKind::BrokenPipe {
                break;
            }
            return Err(err).context("write history event newline");
        }
        if let Err(err) = write_half.flush().await {
            if err.kind() == std::io::ErrorKind::BrokenPipe {
                break;
            }
            return Err(err).context("flush history event");
        }
    }
    daemon_log(
        &data_dir,
        "info",
        format!(
            "watch history stream ended request_id={} conversation={}",
            request_id,
            conversation_id
        ),
    );
    Ok(())
}

struct DaemonFilesGuard {
    socket_path: PathBuf,
    pid_path: PathBuf,
}

impl DaemonFilesGuard {
    fn new(socket_path: PathBuf, pid_path: PathBuf) -> Self {
        Self {
            socket_path,
            pid_path,
        }
    }
}

impl Drop for DaemonFilesGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.pid_path);
    }
}

#[cfg(test)]
mod tests {
    use super::{ConversationLookup, resolve_conversation_id, resolve_message_id, summarize_decoded_content};
    use xmtp::content::{Content, Reaction, ReactionAction, ReactionSchema, Reply, EncodedContent};

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
            resolve_conversation_id(&ids, "12345678....66667777", None).expect("resolved id");

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

        let resolved = resolve_conversation_id(&ids, "abcdef01", None).expect("resolved id");

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

        let resolved = resolve_conversation_id(&ids, "Andelf", None).expect("resolved id");

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

        let error = resolve_conversation_id(&ids, "dup", None).expect_err("ambiguous name");

        assert!(error.to_string().contains("conversation name dup is ambiguous"));
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
            content: EncodedContent::default(),
        }));

        assert_eq!(reaction, "reacted 👍 to abcd1234");
        assert_eq!(reply, "reply to dcba4321");
    }

    #[test]
    fn summarize_decoded_content_formats_unknown_without_dumping_raw_bytes() {
        let summary = summarize_decoded_content(&Content::Unknown {
            content_type: "xmtp.org/group_updated:1.0".to_owned(),
            raw: vec![1, 2, 3, 4],
        });

        assert_eq!(summary, "unsupported xmtp.org/group_updated:1.0");
    }
}
