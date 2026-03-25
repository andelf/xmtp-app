use serde::{Deserialize, Serialize};
use xmtp_core::{ConnectionState, DaemonState};

pub type RequestId = String;
pub type ProtocolVersion = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEnvelope<T> {
    pub version: ProtocolVersion,
    pub request_id: RequestId,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    GetStatus,
    Shutdown,
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
    Login {
        env: String,
        api_url: Option<String>,
    },
    ListConversations {
        kind: Option<String>,
    },
    OpenDm {
        recipient: String,
    },
    SendDm {
        recipient: String,
        message: String,
    },
    CreateGroup {
        name: Option<String>,
        members: Vec<String>,
    },
    SendGroup {
        conversation_id: String,
        message: String,
    },
    RenameGroup {
        conversation_id: String,
        name: String,
    },
    AddGroupMembers {
        conversation_id: String,
        members: Vec<String>,
    },
    RemoveGroupMembers {
        conversation_id: String,
        members: Vec<String>,
    },
    LeaveConversation {
        conversation_id: String,
    },
    GroupMembers {
        conversation_id: String,
    },
    GroupInfo {
        conversation_id: String,
    },
    ConversationInfo {
        conversation_id: String,
    },
    MessageInfo {
        message_id: String,
    },
    WatchHistory {
        conversation_id: String,
    },
    History {
        conversation_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonResponse {
    pub ok: bool,
    pub result: Option<DaemonResponseData>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponseData {
    Status(StatusResponse),
    ConversationList(ConversationListResponse),
    GroupMembers(GroupMembersResponse),
    OpenDm(ActionResponse),
    SendDm(SendDmResponse),
    CreateGroup(ActionResponse),
    SendGroup(ActionResponse),
    RenameGroup(ActionResponse),
    AddGroupMembers(ActionResponse),
    RemoveGroupMembers(ActionResponse),
    LeaveConversation(ActionResponse),
    GroupInfo(GroupInfoResponse),
    Reply(ActionResponse),
    React(ActionResponse),
    Unreact(ActionResponse),
    ConversationInfo(ConversationInfoResponse),
    MessageInfo(MessageInfoResponse),
    History(HistoryResponse),
    HistoryEvent(HistoryEventResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub daemon_state: DaemonState,
    pub connection_state: ConnectionState,
    pub inbox_id: Option<String>,
    pub installation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationListResponse {
    pub items: Vec<ConversationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationItem {
    pub id: String,
    pub kind: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMembersResponse {
    pub items: Vec<GroupMemberItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResponse {
    pub items: Vec<HistoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEventResponse {
    pub item: HistoryItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionDetail {
    pub sender_inbox_id: String,
    pub emoji: String,
    pub action: String,
}
