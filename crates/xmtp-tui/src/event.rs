use crossterm::event::Event as TerminalEvent;
use xmtp_ipc::{
    ActionResponse, ConversationInfoResponse, ConversationItem, ConversationUpdatedEvent,
    GroupInfoResponse, GroupMemberItem, GroupMembersUpdatedEvent, GroupPermissionsResponse,
    HistoryItem, StatusResponse,
};

#[derive(Debug, Clone)]
pub enum AppEvent {
    Terminal(TerminalEvent),
    StatusLoaded(StatusResponse),
    ConversationsLoaded(Vec<ConversationItem>),
    ConversationUpdated(ConversationUpdatedEvent),
    GroupMembersUpdated(GroupMembersUpdatedEvent),
    ConversationInfoLoaded(ConversationInfoResponse),
    GroupInfoLoaded(GroupInfoResponse),
    GroupMembersLoaded(Vec<GroupMemberItem>),
    GroupPermissionsLoaded(GroupPermissionsResponse),
    HistoryLoaded {
        conversation_id: String,
        items: Vec<HistoryItem>,
    },
    HistoryEvent {
        conversation_id: String,
        item: HistoryItem,
    },
    ActionCompleted(ActionOutcome),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ActionOutcome {
    OpenedDm(ActionResponse),
    Sent {
        conversation_id: String,
        message_id: String,
        text: String,
    },
    CreatedGroup(ActionResponse),
    Reacted,
    GroupUpdated(String),
    PermissionUpdated,
    LeftConversation(String),
}

#[derive(Debug, Clone)]
pub enum Effect {
    SubscribeAppEvents,
    SwitchConversation {
        conversation_id: String,
    },
    SendReadReceipt {
        conversation_id: String,
    },
    OpenDm {
        recipient: String,
    },
    CreateGroup {
        name: Option<String>,
        members: Vec<String>,
    },
    LoadGroupInfo {
        conversation_id: String,
    },
    LoadGroupMembers {
        conversation_id: String,
    },
    LoadGroupPermissions {
        conversation_id: String,
    },
    UpdateGroupPermission {
        conversation_id: String,
        permission: String,
        policy: String,
    },
    AddGroupMembers {
        conversation_id: String,
        members: Vec<String>,
    },
    RemoveGroupMembers {
        conversation_id: String,
        members: Vec<String>,
    },
    RenameGroup {
        conversation_id: String,
        name: String,
    },
    LeaveConversation {
        conversation_id: String,
    },
    SendMessage {
        conversation_id: String,
        kind: String,
        target: Option<String>,
        text: String,
    },
    Reply {
        message_id: String,
        text: String,
        conversation_id: String,
    },
    React {
        message_id: String,
        emoji: String,
        conversation_id: String,
    },
}
