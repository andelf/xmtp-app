use crossterm::event::Event as TerminalEvent;
use xmtp_ipc::{ActionResponse, ConversationInfoResponse, ConversationItem, HistoryItem, StatusResponse};

#[derive(Debug, Clone)]
pub enum AppEvent {
    Terminal(TerminalEvent),
    StatusLoaded(StatusResponse),
    ConversationsLoaded(Vec<ConversationItem>),
    ConversationInfoLoaded(ConversationInfoResponse),
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
    Sent,
    CreatedGroup(ActionResponse),
    Reacted,
}

#[derive(Debug, Clone)]
pub enum Effect {
    SubscribeAppEvents,
    SwitchConversation { conversation_id: String },
    OpenDm { recipient: String },
    CreateGroup { name: Option<String>, members: Vec<String> },
    SendMessage { conversation_id: String, kind: String, target: Option<String>, text: String },
    Reply { message_id: String, text: String },
    React { message_id: String, emoji: String },
}
