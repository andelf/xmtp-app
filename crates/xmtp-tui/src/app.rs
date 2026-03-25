use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Color;
use xmtp_ipc::{ConversationInfoResponse, ConversationItem, HistoryItem, ReactionDetail, StatusResponse};

use crate::event::{ActionOutcome, AppEvent, Effect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Conversations,
    Messages,
    Input,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Self::Conversations => Self::Messages,
            Self::Messages => Self::Input,
            Self::Input => Self::Conversations,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Conversations => Self::Input,
            Self::Messages => Self::Conversations,
            Self::Input => Self::Messages,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    None,
    MessageMenu,
    ReactionPicker,
    CreateDm,
    CreateGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageMenuAction {
    Reply,
    Reaction,
}

impl MessageMenuAction {
    pub fn all() -> [Self; 2] {
        [Self::Reply, Self::Reaction]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Reply => "reply",
            Self::Reaction => "reaction",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupDialogField {
    Name,
    Members,
}

#[derive(Debug, Clone, Default)]
pub struct CreateDmDialog {
    pub recipient: String,
}

#[derive(Debug, Clone, Default)]
pub struct CreateGroupDialog {
    pub name: String,
    pub members: String,
    pub field: Option<GroupDialogField>,
}

#[derive(Debug, Clone)]
pub struct App {
    pub focus: Focus,
    pub modal: Modal,
    pub should_quit: bool,
    pub status: Option<StatusResponse>,
    pub conversations: Vec<ConversationItem>,
    pub selected_conversation: usize,
    pub active_conversation_id: Option<String>,
    pub active_conversation: Option<ConversationItem>,
    pub active_info: Option<ConversationInfoResponse>,
    pub active_history_loading: bool,
    pub messages: Vec<HistoryItem>,
    pub selected_message: usize,
    pub input: String,
    pub reply_to_message_id: Option<String>,
    pub message_menu_index: usize,
    pub reaction_picker_index: usize,
    pub dm_dialog: CreateDmDialog,
    pub group_dialog: CreateGroupDialog,
    pub last_error: Option<String>,
    pub exit_armed: bool,
}

impl App {
    pub fn new() -> (Self, Vec<Effect>) {
        (
            Self {
                focus: Focus::Conversations,
                modal: Modal::None,
                should_quit: false,
                status: None,
                conversations: Vec::new(),
                selected_conversation: 0,
                active_conversation_id: None,
                active_conversation: None,
                active_info: None,
                active_history_loading: false,
                messages: Vec::new(),
                selected_message: 0,
                input: String::new(),
                reply_to_message_id: None,
                message_menu_index: 0,
                reaction_picker_index: 0,
                dm_dialog: CreateDmDialog::default(),
                group_dialog: CreateGroupDialog {
                    field: Some(GroupDialogField::Name),
                    ..Default::default()
                },
                last_error: None,
                exit_armed: false,
            },
            vec![Effect::RefreshStatus, Effect::RefreshConversations],
        )
    }

    pub fn handle_event(&mut self, event: AppEvent) -> Vec<Effect> {
        match event {
            AppEvent::Terminal(event) => self.handle_terminal_event(event),
            AppEvent::Tick => {
                vec![Effect::RefreshStatus, Effect::RefreshConversations]
            }
            AppEvent::StatusLoaded(status) => {
                self.status = Some(status);
                Vec::new()
            }
            AppEvent::ConversationsLoaded(items) => self.update_conversations(items),
            AppEvent::ConversationInfoLoaded(info) => {
                self.active_info = Some(info);
                Vec::new()
            }
            AppEvent::HistoryLoaded {
                conversation_id,
                items,
            } => {
                if self.active_conversation_id.as_deref() == Some(conversation_id.as_str()) {
                    let previous_selected = self.selected_history_item().map(|item| item.message_id.clone());
                    self.messages = normalize_history(items);
                    self.active_history_loading = false;
                    self.selected_message = if self.should_auto_scroll_messages() {
                        self.messages.len().saturating_sub(1)
                    } else {
                        previous_selected
                            .and_then(|message_id| {
                                self.messages.iter().position(|item| item.message_id == message_id)
                            })
                            .unwrap_or_else(|| self.selected_message.min(self.messages.len().saturating_sub(1)))
                    };
                }
                Vec::new()
            }
            AppEvent::HistoryEvent {
                conversation_id,
                item,
            } => {
                if self.active_conversation_id.as_deref() == Some(conversation_id.as_str()) {
                    merge_history_item(&mut self.messages, item);
                    if self.should_auto_scroll_messages() {
                        self.selected_message = self.messages.len().saturating_sub(1);
                    } else {
                        self.selected_message = self.selected_message.min(self.messages.len().saturating_sub(1));
                    }
                }
                Vec::new()
            }
            AppEvent::ActionCompleted(outcome) => self.handle_action_completed(outcome),
            AppEvent::Error(error) => {
                self.last_error = Some(error);
                Vec::new()
            }
        }
    }

    fn update_conversations(&mut self, items: Vec<ConversationItem>) -> Vec<Effect> {
        self.conversations = items;
        if self.conversations.is_empty() {
            self.selected_conversation = 0;
            self.active_conversation = None;
            self.active_conversation_id = None;
            self.active_info = None;
            self.active_history_loading = false;
            self.messages.clear();
            return Vec::new();
        }

        let current_active = self.active_conversation_id.clone();
        if let Some(active_id) = current_active {
            if let Some(index) = self
                .conversations
                .iter()
                .position(|conversation| conversation.id == active_id)
            {
                self.selected_conversation = index;
                self.active_conversation = Some(self.conversations[index].clone());
                return Vec::new();
            }
        }

        self.selected_conversation = self.selected_conversation.min(self.conversations.len() - 1);
        let conversation = self.conversations[self.selected_conversation].clone();
        self.activate_conversation(conversation)
    }

    fn handle_action_completed(&mut self, outcome: ActionOutcome) -> Vec<Effect> {
        match outcome {
            ActionOutcome::OpenedDm(result) => {
                self.modal = Modal::None;
                self.focus = Focus::Input;
                self.dm_dialog = CreateDmDialog::default();
                self.active_conversation_id = Some(result.conversation_id.clone());
                vec![
                    Effect::RefreshConversations,
                    Effect::SwitchConversation {
                        conversation_id: result.conversation_id,
                    },
                ]
            }
            ActionOutcome::CreatedGroup(result) => {
                self.modal = Modal::None;
                self.focus = Focus::Input;
                self.group_dialog = CreateGroupDialog {
                    field: Some(GroupDialogField::Name),
                    ..Default::default()
                };
                self.active_conversation_id = Some(result.conversation_id.clone());
                vec![
                    Effect::RefreshConversations,
                    Effect::SwitchConversation {
                        conversation_id: result.conversation_id,
                    },
                ]
            }
            ActionOutcome::Sent | ActionOutcome::Reacted => Vec::new(),
        }
    }

    fn handle_terminal_event(&mut self, event: Event) -> Vec<Effect> {
        if let Event::Key(key) = event {
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                return Vec::new();
            }
            return self.handle_key(key);
        }
        Vec::new()
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.code == KeyCode::Esc {
            return self.handle_escape();
        }
        self.exit_armed = false;

        if key.code == KeyCode::Tab {
            if self.modal == Modal::None {
                self.focus = self.focus.next();
            } else if self.modal == Modal::CreateGroup {
                self.group_dialog.field = Some(match self.group_dialog.field.unwrap_or(GroupDialogField::Name) {
                    GroupDialogField::Name => GroupDialogField::Members,
                    GroupDialogField::Members => GroupDialogField::Name,
                });
            }
            return Vec::new();
        }
        if key.code == KeyCode::BackTab {
            if self.modal == Modal::None {
                self.focus = self.focus.previous();
            } else if self.modal == Modal::CreateGroup {
                self.group_dialog.field = Some(match self.group_dialog.field.unwrap_or(GroupDialogField::Name) {
                    GroupDialogField::Name => GroupDialogField::Members,
                    GroupDialogField::Members => GroupDialogField::Name,
                });
            }
            return Vec::new();
        }

        match self.modal {
            Modal::None => self.handle_key_without_modal(key),
            Modal::MessageMenu => self.handle_message_menu_key(key),
            Modal::ReactionPicker => self.handle_reaction_picker_key(key),
            Modal::CreateDm => self.handle_create_dm_key(key),
            Modal::CreateGroup => self.handle_create_group_key(key),
        }
    }

    fn handle_escape(&mut self) -> Vec<Effect> {
        match self.modal {
            Modal::MessageMenu | Modal::ReactionPicker | Modal::CreateDm | Modal::CreateGroup => {
                self.modal = Modal::None;
                self.exit_armed = false;
                return Vec::new();
            }
            Modal::None => {}
        }

        if self.focus != Focus::Conversations {
            self.focus = Focus::Conversations;
            self.exit_armed = false;
            return Vec::new();
        }

        if self.exit_armed {
            self.should_quit = true;
        } else {
            self.exit_armed = true;
        }
        Vec::new()
    }

    fn handle_key_without_modal(&mut self, key: KeyEvent) -> Vec<Effect> {
        if self.focus != Focus::Input {
            match key.code {
                KeyCode::Char('q') => {
                    self.should_quit = true;
                    return Vec::new();
                }
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.modal = Modal::CreateDm;
                    self.dm_dialog = CreateDmDialog::default();
                    return Vec::new();
                }
                KeyCode::Char('g') => {
                    self.modal = Modal::CreateGroup;
                    self.group_dialog = CreateGroupDialog {
                        field: Some(GroupDialogField::Name),
                        ..Default::default()
                    };
                    return Vec::new();
                }
                _ => {}
            }
        }

        match self.focus {
            Focus::Conversations => self.handle_conversation_key(key),
            Focus::Messages => self.handle_message_list_key(key),
            Focus::Input => self.handle_input_key(key),
        }
    }

    fn handle_conversation_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if self.conversations.is_empty() {
            if key.code == KeyCode::Enter {
                self.focus = Focus::Input;
            }
            return Vec::new();
        }
        match key.code {
            KeyCode::Up => {
                if self.selected_conversation > 0 {
                    self.selected_conversation -= 1;
                    let conversation = self.conversations[self.selected_conversation].clone();
                    return self.activate_conversation(conversation);
                }
            }
            KeyCode::Down => {
                if self.selected_conversation + 1 < self.conversations.len() {
                    self.selected_conversation += 1;
                    let conversation = self.conversations[self.selected_conversation].clone();
                    return self.activate_conversation(conversation);
                }
            }
            KeyCode::Enter => {
                self.focus = Focus::Input;
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_message_list_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Up => {
                if self.selected_message > 0 {
                    self.selected_message -= 1;
                }
            }
            KeyCode::Down => {
                if self.selected_message + 1 < self.messages.len() {
                    self.selected_message += 1;
                }
            }
            KeyCode::Enter => {
                if !self.messages.is_empty() {
                    self.modal = Modal::MessageMenu;
                    self.message_menu_index = 0;
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                self.input.push('\n');
            }
            KeyCode::Enter => {
                let text = self.input.trim_end().to_owned();
                if text.is_empty() {
                    return Vec::new();
                }
                self.input.clear();
                if let Some(message_id) = self.reply_to_message_id.take() {
                    return vec![Effect::Reply { message_id, text }];
                }
                if let Some(conversation) = &self.active_conversation {
                    let target = self.active_info.as_ref().and_then(|info| info.dm_peer_inbox_id.clone());
                    return vec![Effect::SendMessage {
                        conversation_id: conversation.id.clone(),
                        kind: conversation.kind.clone(),
                        target,
                        text,
                    }];
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input.push(ch);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_message_menu_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Up => {
                if self.message_menu_index > 0 {
                    self.message_menu_index -= 1;
                }
            }
            KeyCode::Down => {
                if self.message_menu_index + 1 < MessageMenuAction::all().len() {
                    self.message_menu_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(message) = self.selected_history_item() {
                    match MessageMenuAction::all()[self.message_menu_index] {
                        MessageMenuAction::Reply => {
                            self.reply_to_message_id = Some(message.message_id.clone());
                            self.modal = Modal::None;
                            self.focus = Focus::Input;
                        }
                        MessageMenuAction::Reaction => {
                            self.modal = Modal::ReactionPicker;
                            self.reaction_picker_index = 0;
                        }
                    }
                } else {
                    self.modal = Modal::None;
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_reaction_picker_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Up => {
                if self.reaction_picker_index > 0 {
                    self.reaction_picker_index -= 1;
                }
            }
            KeyCode::Down => {
                if self.reaction_picker_index + 1 < reaction_choices().len() {
                    self.reaction_picker_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(message) = self.selected_history_item() {
                    let message_id = message.message_id.clone();
                    self.modal = Modal::None;
                    return vec![Effect::React {
                        message_id,
                        emoji: reaction_choices()[self.reaction_picker_index].to_owned(),
                    }];
                }
                self.modal = Modal::None;
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_create_dm_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Backspace => {
                self.dm_dialog.recipient.pop();
            }
            KeyCode::Enter => {
                let recipient = self.dm_dialog.recipient.trim().to_owned();
                if !recipient.is_empty() {
                    return vec![Effect::OpenDm { recipient }];
                }
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.dm_dialog.recipient.push(ch);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_create_group_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let field = self.group_dialog.field.unwrap_or(GroupDialogField::Name);
        match key.code {
            KeyCode::Backspace => match field {
                GroupDialogField::Name => {
                    self.group_dialog.name.pop();
                }
                GroupDialogField::Members => {
                    self.group_dialog.members.pop();
                }
            },
            KeyCode::Enter => {
                if field == GroupDialogField::Name {
                    self.group_dialog.field = Some(GroupDialogField::Members);
                } else {
                    let members: Vec<String> = self
                        .group_dialog
                        .members
                        .split(|ch: char| ch == ',' || ch.is_whitespace())
                        .filter(|value| !value.trim().is_empty())
                        .map(|value| value.trim().to_owned())
                        .collect();
                    if !members.is_empty() {
                        let name = self.group_dialog.name.trim();
                        return vec![Effect::CreateGroup {
                            name: if name.is_empty() { None } else { Some(name.to_owned()) },
                            members,
                        }];
                    }
                }
            }
            KeyCode::Char(ch) => match field {
                GroupDialogField::Name => self.group_dialog.name.push(ch),
                GroupDialogField::Members => self.group_dialog.members.push(ch),
            },
            _ => {}
        }
        Vec::new()
    }

    fn selected_history_item(&self) -> Option<&HistoryItem> {
        self.messages.get(self.selected_message)
    }

    fn should_auto_scroll_messages(&self) -> bool {
        self.focus != Focus::Messages
    }

    fn activate_conversation(&mut self, conversation: ConversationItem) -> Vec<Effect> {
        self.active_conversation_id = Some(conversation.id.clone());
        self.active_conversation = Some(conversation.clone());
        self.active_info = None;
        self.active_history_loading = true;
        self.messages.clear();
        self.selected_message = 0;
        vec![Effect::SwitchConversation {
            conversation_id: conversation.id,
        }]
    }

    pub fn self_inbox_id(&self) -> Option<&str> {
        self.status.as_ref().and_then(|status| status.inbox_id.as_deref())
    }

    pub fn color_for_message(&self, item: &HistoryItem) -> Color {
        if item.content_kind == "unknown" || item.content.starts_with("type=unknown content_type=") {
            return Color::DarkGray;
        }
        if self.self_inbox_id() == Some(item.sender_inbox_id.as_str()) {
            Color::Green
        } else {
            Color::Cyan
        }
    }
}

pub fn reaction_choices() -> [&'static str; 5] {
    ["👍", "❤️", "🔥", "😂", "👀"]
}

fn normalize_history(items: Vec<HistoryItem>) -> Vec<HistoryItem> {
    let mut visible = Vec::new();
    for item in items {
        merge_history_item(&mut visible, item);
    }
    visible
}

fn merge_history_item(visible: &mut Vec<HistoryItem>, item: HistoryItem) {
    if visible.iter().any(|existing| existing.message_id == item.message_id) {
        return;
    }

    if item.content_kind == "reaction" {
        if let Some(target_message_id) = item.reaction_target_message_id.clone() {
            if let Some(target) = visible
                .iter_mut()
                .find(|existing| existing.message_id == target_message_id)
            {
                if let (Some(emoji), Some(action)) =
                    (item.reaction_emoji.clone(), item.reaction_action.clone())
                {
                    target.attached_reactions.push(ReactionDetail {
                        sender_inbox_id: item.sender_inbox_id,
                        emoji,
                        action,
                    });
                    return;
                }
            }
        }
    }

    visible.push(item);
}

#[cfg(test)]
mod tests {
    use super::{App, Focus, Modal};
    use crate::event::Effect;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn focus_cycles_forward() {
        assert_eq!(Focus::Conversations.next(), Focus::Messages);
        assert_eq!(Focus::Messages.next(), Focus::Input);
        assert_eq!(Focus::Input.next(), Focus::Conversations);
    }

    #[test]
    fn input_focus_treats_char_as_text() {
        let (mut app, _) = App::new();
        app.focus = Focus::Input;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE,
        ))));
        assert!(effects.is_empty());
        assert_eq!(app.input, "c");
    }

    #[test]
    fn conversation_navigation_switches_immediately() {
        let (mut app, _) = App::new();
        app.conversations = vec![
            xmtp_ipc::ConversationItem { id: "one".into(), kind: "dm".into(), name: None },
            xmtp_ipc::ConversationItem { id: "two".into(), kind: "group".into(), name: None },
        ];
        app.messages.push(xmtp_ipc::HistoryItem {
            message_id: "old-msg".into(),
            sender_inbox_id: "sender-1".into(),
            sent_at_ns: 1,
            content_kind: "text".into(),
            content: "old".into(),
            reply_count: 0,
            reaction_count: 0,
            reply_target_message_id: None,
            reaction_target_message_id: None,
            reaction_emoji: None,
            reaction_action: None,
            attached_reactions: Vec::new(),
        });
        app.focus = Focus::Conversations;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ))));
        assert_eq!(app.selected_conversation, 1);
        assert_eq!(app.active_conversation_id.as_deref(), Some("two"));
        assert!(app.active_history_loading);
        assert!(app.messages.is_empty());
        assert!(matches!(effects.as_slice(), [Effect::SwitchConversation { conversation_id }] if conversation_id == "two"));
    }

    #[test]
    fn ctrl_n_opens_create_dm_modal_outside_input() {
        let (mut app, _) = App::new();
        app.focus = Focus::Conversations;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('n'),
            KeyModifiers::CONTROL,
        ))));
        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::CreateDm);
    }

    #[test]
    fn enter_in_conversations_jumps_to_input() {
        let (mut app, _) = App::new();
        app.focus = Focus::Conversations;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
        assert!(effects.is_empty());
        assert_eq!(app.focus, Focus::Input);
        assert!(app.input.is_empty());
    }

    #[test]
    fn enter_on_message_list_opens_message_menu() {
        let (mut app, _) = App::new();
        app.focus = Focus::Messages;
        app.messages.push(xmtp_ipc::HistoryItem {
            message_id: "msg-1".into(),
            sender_inbox_id: "sender-1".into(),
            sent_at_ns: 1,
            content_kind: "text".into(),
            content: "hello".into(),
            reply_count: 0,
            reaction_count: 0,
            reply_target_message_id: None,
            reaction_target_message_id: None,
            reaction_emoji: None,
            reaction_action: None,
            attached_reactions: Vec::new(),
        });
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::MessageMenu);
    }

    #[test]
    fn history_load_merges_reaction_into_target_message() {
        let (mut app, _) = App::new();
        app.active_conversation_id = Some("conv-1".into());
        app.handle_event(crate::event::AppEvent::HistoryLoaded {
            conversation_id: "conv-1".into(),
            items: vec![
                xmtp_ipc::HistoryItem {
                    message_id: "msg-1".into(),
                    sender_inbox_id: "sender-1".into(),
                    sent_at_ns: 1,
                    content_kind: "text".into(),
                    content: "hello".into(),
                    reply_count: 0,
                    reaction_count: 1,
                    reply_target_message_id: None,
                    reaction_target_message_id: None,
                    reaction_emoji: None,
                    reaction_action: None,
                    attached_reactions: Vec::new(),
                },
                xmtp_ipc::HistoryItem {
                    message_id: "msg-2".into(),
                    sender_inbox_id: "sender-2".into(),
                    sent_at_ns: 2,
                    content_kind: "reaction".into(),
                    content: "reacted 👍 to msg-1".into(),
                    reply_count: 0,
                    reaction_count: 0,
                    reply_target_message_id: None,
                    reaction_target_message_id: Some("msg-1".into()),
                    reaction_emoji: Some("👍".into()),
                    reaction_action: Some("added".into()),
                    attached_reactions: Vec::new(),
                },
            ],
        });

        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].message_id, "msg-1");
        assert_eq!(app.messages[0].attached_reactions.len(), 1);
        assert_eq!(app.messages[0].attached_reactions[0].emoji, "👍");
    }

    #[test]
    fn tick_does_not_force_active_history_refresh() {
        let (mut app, _) = App::new();
        app.active_conversation_id = Some("conv-1".into());
        let effects = app.handle_event(crate::event::AppEvent::Tick);
        assert_eq!(effects.len(), 2);
        assert!(matches!(effects[0], Effect::RefreshStatus));
        assert!(matches!(effects[1], Effect::RefreshConversations));
    }

    #[test]
    fn history_event_auto_scrolls_when_messages_panel_is_not_focused() {
        let (mut app, _) = App::new();
        app.active_conversation_id = Some("conv-1".into());
        app.focus = Focus::Conversations;
        app.messages = vec![
            xmtp_ipc::HistoryItem {
                message_id: "msg-1".into(),
                sender_inbox_id: "sender-1".into(),
                sent_at_ns: 1,
                content_kind: "text".into(),
                content: "first".into(),
                reply_count: 0,
                reaction_count: 0,
                reply_target_message_id: None,
                reaction_target_message_id: None,
                reaction_emoji: None,
                reaction_action: None,
                attached_reactions: Vec::new(),
            },
            xmtp_ipc::HistoryItem {
                message_id: "msg-2".into(),
                sender_inbox_id: "sender-1".into(),
                sent_at_ns: 2,
                content_kind: "text".into(),
                content: "second".into(),
                reply_count: 0,
                reaction_count: 0,
                reply_target_message_id: None,
                reaction_target_message_id: None,
                reaction_emoji: None,
                reaction_action: None,
                attached_reactions: Vec::new(),
            },
        ];
        app.selected_message = 0;

        app.handle_event(crate::event::AppEvent::HistoryEvent {
            conversation_id: "conv-1".into(),
            item: xmtp_ipc::HistoryItem {
                message_id: "msg-3".into(),
                sender_inbox_id: "sender-1".into(),
                sent_at_ns: 3,
                content_kind: "text".into(),
                content: "third".into(),
                reply_count: 0,
                reaction_count: 0,
                reply_target_message_id: None,
                reaction_target_message_id: None,
                reaction_emoji: None,
                reaction_action: None,
                attached_reactions: Vec::new(),
            },
        });

        assert_eq!(app.selected_message, 2);
    }

    #[test]
    fn history_event_does_not_auto_scroll_when_messages_panel_is_focused() {
        let (mut app, _) = App::new();
        app.active_conversation_id = Some("conv-1".into());
        app.focus = Focus::Messages;
        app.messages = vec![
            xmtp_ipc::HistoryItem {
                message_id: "msg-1".into(),
                sender_inbox_id: "sender-1".into(),
                sent_at_ns: 1,
                content_kind: "text".into(),
                content: "first".into(),
                reply_count: 0,
                reaction_count: 0,
                reply_target_message_id: None,
                reaction_target_message_id: None,
                reaction_emoji: None,
                reaction_action: None,
                attached_reactions: Vec::new(),
            },
            xmtp_ipc::HistoryItem {
                message_id: "msg-2".into(),
                sender_inbox_id: "sender-1".into(),
                sent_at_ns: 2,
                content_kind: "text".into(),
                content: "second".into(),
                reply_count: 0,
                reaction_count: 0,
                reply_target_message_id: None,
                reaction_target_message_id: None,
                reaction_emoji: None,
                reaction_action: None,
                attached_reactions: Vec::new(),
            },
        ];
        app.selected_message = 0;

        app.handle_event(crate::event::AppEvent::HistoryEvent {
            conversation_id: "conv-1".into(),
            item: xmtp_ipc::HistoryItem {
                message_id: "msg-3".into(),
                sender_inbox_id: "sender-1".into(),
                sent_at_ns: 3,
                content_kind: "text".into(),
                content: "third".into(),
                reply_count: 0,
                reaction_count: 0,
                reply_target_message_id: None,
                reaction_target_message_id: None,
                reaction_emoji: None,
                reaction_action: None,
                attached_reactions: Vec::new(),
            },
        });

        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn esc_from_input_returns_to_conversations_without_quitting() {
        let (mut app, _) = App::new();
        app.focus = Focus::Input;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ))));
        assert!(effects.is_empty());
        assert_eq!(app.focus, Focus::Conversations);
        assert!(!app.should_quit);
        assert!(!app.exit_armed);
    }

    #[test]
    fn esc_twice_in_conversations_quits() {
        let (mut app, _) = App::new();
        app.focus = Focus::Conversations;
        let first = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ))));
        assert!(first.is_empty());
        assert!(!app.should_quit);
        assert!(app.exit_armed);

        let second = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ))));
        assert!(second.is_empty());
        assert!(app.should_quit);
    }

    #[test]
    fn esc_closes_modal_without_arming_exit() {
        let (mut app, _) = App::new();
        app.modal = Modal::CreateDm;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ))));
        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::None);
        assert!(!app.exit_armed);
        assert!(!app.should_quit);
    }
}
