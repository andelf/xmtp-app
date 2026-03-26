use serde::{Deserialize, Serialize};
use xmtp_core::{ConnectionState, DaemonState};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusResponse {
    pub daemon_state: DaemonState,
    pub connection_state: ConnectionState,
    pub inbox_id: Option<String>,
    pub installation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationListResponse {
    pub items: Vec<ConversationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationItem {
    pub id: String,
    pub kind: String,
    pub name: Option<String>,
    pub dm_peer_inbox_id: Option<String>,
    pub last_message_ns: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationUpdatedEvent {
    pub conversation_id: String,
    pub name: Option<String>,
    pub member_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupMembersUpdatedEvent {
    pub conversation_id: String,
    pub members: Vec<GroupMemberItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMembersResponse {
    pub items: Vec<GroupMemberItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupMemberItem {
    pub inbox_id: String,
    pub permission_level: String,
    pub consent_state: String,
    pub account_identifiers: Vec<String>,
    pub installation_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfoResponse {
    pub conversation_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub creator_inbox_id: String,
    pub conversation_type: String,
    pub permission_preset: String,
    pub member_count: usize,
 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInfoResponse {
    pub conversation_id: String,
    pub name: Option<String>,
    pub conversation_type: String,
    pub created_at_ns: i64,
    pub is_active: bool,
    pub membership_state: String,
    pub dm_peer_inbox_id: Option<String>,
    pub member_count: usize,
    pub message_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfoResponse {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_inbox_id: String,
    pub sent_at_ns: i64,
    pub delivery_status: String,
    pub content_type: Option<String>,
    pub content_summary: String,
    pub reply_count: i32,
    pub reaction_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendDmResponse {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResponse {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryResponse {
    pub items: Vec<HistoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryItem {
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
    #[serde(default)]
    pub attached_reactions: Vec<ReactionDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReactionDetail {
    pub sender_inbox_id: String,
    pub emoji: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoginRequest {
    pub env: String,
    pub api_url: Option<String>,
    pub gateway_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecipientRequest {
    pub recipient: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendMessageRequest {
    pub message: String,
    pub conversation_id: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecipientMessageRequest {
    pub recipient: String,
    pub message: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupCreateRequest {
    pub name: Option<String>,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenameGroupRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupMembersUpdateRequest {
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmojiRequest {
    pub emoji: String,
    pub action: Option<String>,
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonEventEnvelope {
    pub event_id: String,
    pub payload: DaemonEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEventData {
    Status(StatusResponse),
    ConversationList(ConversationListResponse),
    ConversationUpdated(ConversationUpdatedEvent),
    GroupMembersUpdated(GroupMembersUpdatedEvent),
    HistoryItem {
        conversation_id: String,
        item: HistoryItem,
    },
    DaemonError {
        message: String,
    },
}
