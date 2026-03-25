use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Color;
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use xmtp_ipc::{
    ConversationInfoResponse, ConversationItem, GroupInfoResponse, GroupMemberItem, HistoryItem,
    ReactionDetail, StatusResponse,
};

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
    Help,
    MessageMenu,
    ReactionPicker,
    CreateDm,
    CreateGroup,
    GroupManagement,
    GroupInfo,
    GroupAddMembers,
    GroupRemoveMembers,
    GroupRename,
    GroupLeaveConfirm,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupManagementAction {
    ViewInfo,
    AddMembers,
    RemoveMembers,
    Rename,
    LeaveGroup,
}

impl GroupManagementAction {
    pub fn all() -> [Self; 5] {
        [
            Self::ViewInfo,
            Self::AddMembers,
            Self::RemoveMembers,
            Self::Rename,
            Self::LeaveGroup,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ViewInfo => "view info",
            Self::AddMembers => "add members",
            Self::RemoveMembers => "remove members",
            Self::Rename => "rename",
            Self::LeaveGroup => "leave group",
        }
    }
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

#[derive(Debug, Clone, Default)]
pub struct GroupManagementState {
    pub menu_index: usize,
    pub info: Option<GroupInfoResponse>,
    pub members: Vec<GroupMemberItem>,
    pub selected_member: usize,
    pub add_members_input: String,
    pub rename_input: String,
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
    pub unread_counts: HashMap<String, u32>,
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
    pub group_management: GroupManagementState,
    pub last_error: Option<String>,
    pub suppressed_error: Option<(String, Instant)>,
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
                unread_counts: HashMap::new(),
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
                group_management: GroupManagementState::default(),
                last_error: None,
                suppressed_error: None,
                exit_armed: false,
            },
            vec![Effect::SubscribeAppEvents],
        )
    }

    pub fn handle_event(&mut self, event: AppEvent) -> Vec<Effect> {
        match event {
            AppEvent::Terminal(event) => self.handle_terminal_event(event),
            AppEvent::StatusLoaded(status) => {
                self.status = Some(status);
                Vec::new()
            }
            AppEvent::ConversationsLoaded(items) => self.update_conversations(items),
            AppEvent::ConversationUpdated(update) => {
                self.apply_conversation_updated(update);
                Vec::new()
            }
            AppEvent::GroupMembersUpdated(update) => {
                if self.active_group_id() == Some(update.conversation_id.as_str()) {
                    self.group_management.members = update.members;
                    self.group_management.selected_member = self
                        .group_management
                        .selected_member
                        .min(self.group_management.members.len().saturating_sub(1));
                }
                Vec::new()
            }
            AppEvent::ConversationInfoLoaded(info) => {
                self.active_info = Some(info);
                Vec::new()
            }
            AppEvent::GroupInfoLoaded(info) => {
                self.group_management.info = Some(info);
                Vec::new()
            }
            AppEvent::GroupMembersLoaded(items) => {
                self.group_management.members = items;
                self.group_management.selected_member = self
                    .group_management
                    .selected_member
                    .min(self.group_management.members.len().saturating_sub(1));
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
                } else {
                    *self.unread_counts.entry(conversation_id).or_insert(0) += 1;
                }
                Vec::new()
            }
            AppEvent::ActionCompleted(outcome) => self.handle_action_completed(outcome),
            AppEvent::Error(error) => {
                if let Some((suppressed, until)) = &self.suppressed_error {
                    if suppressed == &error && Instant::now() < *until {
                        return Vec::new();
                    }
                }
                self.last_error = Some(error);
                Vec::new()
            }
        }
    }

    fn update_conversations(&mut self, items: Vec<ConversationItem>) -> Vec<Effect> {
        let previous_markers = self
            .conversations
            .iter()
            .map(|conversation| (conversation.id.clone(), conversation.last_message_ns))
            .collect::<HashMap<_, _>>();
        self.conversations = items;
        self.unread_counts
            .retain(|conversation_id, _| self.conversations.iter().any(|conversation| &conversation.id == conversation_id));
        for conversation in &self.conversations {
            let previous_last_message_ns = previous_markers
                .get(&conversation.id)
                .copied()
                .flatten();
            if self.active_conversation_id.as_deref() != Some(conversation.id.as_str())
                && matches!(
                    (previous_last_message_ns, conversation.last_message_ns),
                    (Some(previous), Some(current)) if current > previous
                )
            {
                *self.unread_counts.entry(conversation.id.clone()).or_insert(0) += 1;
            }
        }
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
                    Effect::SwitchConversation {
                        conversation_id: result.conversation_id,
                    },
                ]
            }
            ActionOutcome::GroupUpdated(conversation_id) => {
                self.modal = Modal::None;
                self.group_management.add_members_input.clear();
                self.group_management.rename_input.clear();
                vec![
                    Effect::SwitchConversation {
                        conversation_id: conversation_id.clone(),
                    },
                    Effect::LoadGroupInfo {
                        conversation_id: conversation_id.clone(),
                    },
                    Effect::LoadGroupMembers { conversation_id },
                ]
            }
            ActionOutcome::Sent {
                conversation_id,
                message_id,
                text,
            } => {
                if self.active_conversation_id.as_deref() == Some(conversation_id.as_str()) {
                    let sender_inbox_id = self.self_inbox_id().unwrap_or_default().to_owned();
                    let sent_at_ns = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|duration| duration.as_nanos().min(i64::MAX as u128) as i64)
                        .unwrap_or_default();
                    merge_history_item(
                        &mut self.messages,
                        HistoryItem {
                            message_id,
                            sender_inbox_id,
                            sent_at_ns,
                            content_kind: "text".to_owned(),
                            content: text,
                            reply_count: 0,
                            reaction_count: 0,
                            reply_target_message_id: None,
                            reaction_target_message_id: None,
                            reaction_emoji: None,
                            reaction_action: None,
                            attached_reactions: Vec::new(),
                        },
                    );
                    if self.should_auto_scroll_messages() {
                        self.selected_message = self.messages.len().saturating_sub(1);
                    }
                }
                Vec::new()
            }
            ActionOutcome::Reacted => Vec::new(),
        }
    }

    fn apply_conversation_updated(&mut self, update: xmtp_ipc::ConversationUpdatedEvent) {
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == update.conversation_id)
        {
            conversation.name = update.name.clone();
        }

        if let Some(active) = self.active_conversation.as_mut() {
            if active.id == update.conversation_id {
                active.name = update.name.clone();
            }
        }

        if let Some(info) = self.active_info.as_mut() {
            if info.conversation_id == update.conversation_id {
                info.name = update.name.clone();
                info.member_count = update.member_count;
            }
        }

        if let Some(info) = self.group_management.info.as_mut() {
            if info.conversation_id == update.conversation_id {
                info.name = update.name;
                info.member_count = update.member_count;
            }
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
        if let Some(error) = self.last_error.take() {
            self.suppressed_error = Some((error, Instant::now() + Duration::from_secs(2)));
        }
        if key.code == KeyCode::Esc {
            return self.handle_escape();
        }
        self.exit_armed = false;

        if self.modal == Modal::None && matches!(key.code, KeyCode::Char('?') | KeyCode::Char('/')) {
            self.modal = Modal::Help;
            return Vec::new();
        }

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
            Modal::Help => self.handle_help_key(key),
            Modal::MessageMenu => self.handle_message_menu_key(key),
            Modal::ReactionPicker => self.handle_reaction_picker_key(key),
            Modal::CreateDm => self.handle_create_dm_key(key),
            Modal::CreateGroup => self.handle_create_group_key(key),
            Modal::GroupManagement => self.handle_group_management_key(key),
            Modal::GroupInfo => self.handle_group_info_key(key),
            Modal::GroupAddMembers => self.handle_group_add_members_key(key),
            Modal::GroupRemoveMembers => self.handle_group_remove_members_key(key),
            Modal::GroupRename => self.handle_group_rename_key(key),
            Modal::GroupLeaveConfirm => self.handle_group_leave_confirm_key(key),
        }
    }

    fn handle_escape(&mut self) -> Vec<Effect> {
        match self.modal {
            Modal::Help
            | Modal::MessageMenu
            | Modal::ReactionPicker
            | Modal::CreateDm
            | Modal::CreateGroup
            | Modal::GroupManagement
            | Modal::GroupInfo
            | Modal::GroupAddMembers
            | Modal::GroupRemoveMembers
            | Modal::GroupRename
            | Modal::GroupLeaveConfirm => {
                self.modal = Modal::None;
                self.exit_armed = false;
                return Vec::new();
            }
            Modal::None => {}
        }

        if self.focus == Focus::Input && self.reply_to_message_id.is_some() {
            self.reply_to_message_id = None;
            self.exit_armed = false;
            return Vec::new();
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
                if self
                    .conversations
                    .get(self.selected_conversation)
                    .is_some_and(|conversation| conversation.kind == "group")
                {
                    self.modal = Modal::GroupManagement;
                    self.group_management.menu_index = 0;
                    return Vec::new();
                }
                self.focus = Focus::Input;
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if matches!(key.code, KeyCode::Enter | KeyCode::Char('?')) {
            self.modal = Modal::None;
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

    fn handle_group_management_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Up => {
                if self.group_management.menu_index > 0 {
                    self.group_management.menu_index -= 1;
                }
            }
            KeyCode::Down => {
                if self.group_management.menu_index + 1 < GroupManagementAction::all().len() {
                    self.group_management.menu_index += 1;
                }
            }
            KeyCode::Enter => {
                let Some(conversation_id) = self.active_group_id().map(str::to_owned) else {
                    self.modal = Modal::None;
                    return Vec::new();
                };
                match GroupManagementAction::all()[self.group_management.menu_index] {
                    GroupManagementAction::ViewInfo => {
                        self.modal = Modal::GroupInfo;
                        return vec![Effect::LoadGroupInfo { conversation_id }];
                    }
                    GroupManagementAction::AddMembers => {
                        self.modal = Modal::GroupAddMembers;
                        self.group_management.add_members_input.clear();
                    }
                    GroupManagementAction::RemoveMembers => {
                        self.modal = Modal::GroupRemoveMembers;
                        self.group_management.selected_member = 0;
                        return vec![Effect::LoadGroupMembers { conversation_id }];
                    }
                    GroupManagementAction::Rename => {
                        self.modal = Modal::GroupRename;
                        self.group_management.rename_input.clear();
                    }
                    GroupManagementAction::LeaveGroup => {
                        self.modal = Modal::GroupLeaveConfirm;
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_group_info_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if matches!(key.code, KeyCode::Enter) {
            self.modal = Modal::GroupManagement;
        }
        Vec::new()
    }

    fn handle_group_add_members_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Backspace => {
                self.group_management.add_members_input.pop();
            }
            KeyCode::Enter => {
                let Some(conversation_id) = self.active_group_id().map(str::to_owned) else {
                    self.modal = Modal::None;
                    return Vec::new();
                };
                let members: Vec<String> = self
                    .group_management
                    .add_members_input
                    .split(|ch: char| ch == ',' || ch.is_whitespace())
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| value.trim().to_owned())
                    .collect();
                if !members.is_empty() {
                    return vec![Effect::AddGroupMembers {
                        conversation_id,
                        members,
                    }];
                }
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.group_management.add_members_input.push(ch);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_group_remove_members_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Up => {
                if self.group_management.selected_member > 0 {
                    self.group_management.selected_member -= 1;
                }
            }
            KeyCode::Down => {
                if self.group_management.selected_member + 1 < self.group_management.members.len() {
                    self.group_management.selected_member += 1;
                }
            }
            KeyCode::Enter => {
                let Some(conversation_id) = self.active_group_id().map(str::to_owned) else {
                    self.modal = Modal::None;
                    return Vec::new();
                };
                if let Some(member) = self
                    .group_management
                    .members
                    .get(self.group_management.selected_member)
                {
                    return vec![Effect::RemoveGroupMembers {
                        conversation_id,
                        members: vec![member.inbox_id.clone()],
                    }];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_group_rename_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Backspace => {
                self.group_management.rename_input.pop();
            }
            KeyCode::Enter => {
                let Some(conversation_id) = self.active_group_id().map(str::to_owned) else {
                    self.modal = Modal::None;
                    return Vec::new();
                };
                let name = self.group_management.rename_input.trim();
                if !name.is_empty() {
                    return vec![Effect::RenameGroup {
                        conversation_id,
                        name: name.to_owned(),
                    }];
                }
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.group_management.rename_input.push(ch);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_group_leave_confirm_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            self.modal = Modal::None;
            self.last_error = Some("Leave group is not supported in this version".to_owned());
            return Vec::new();
        }
        Vec::new()
    }

    fn active_group_id(&self) -> Option<&str> {
        self.active_conversation
            .as_ref()
            .filter(|conversation| conversation.kind == "group")
            .map(|conversation| conversation.id.as_str())
    }

    fn selected_history_item(&self) -> Option<&HistoryItem> {
        self.messages.get(self.selected_message)
    }

    fn should_auto_scroll_messages(&self) -> bool {
        self.focus != Focus::Messages
    }

    fn activate_conversation(&mut self, conversation: ConversationItem) -> Vec<Effect> {
        self.unread_counts.remove(&conversation.id);
        self.reply_to_message_id = None;
        self.input.clear();
        self.active_conversation_id = Some(conversation.id.clone());
        self.active_conversation = Some(conversation.clone());
        self.active_info = None;
        self.active_history_loading = true;
        self.group_management.info = None;
        self.group_management.members.clear();
        self.group_management.selected_member = 0;
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
            xmtp_ipc::ConversationItem { id: "one".into(), kind: "dm".into(), name: None, dm_peer_inbox_id: None, last_message_ns: None },
            xmtp_ipc::ConversationItem { id: "two".into(), kind: "group".into(), name: None, dm_peer_inbox_id: None, last_message_ns: None },
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
    fn question_mark_opens_help_modal() {
        let (mut app, _) = App::new();
        app.focus = Focus::Conversations;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('?'),
            KeyModifiers::SHIFT,
        ))));
        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::Help);
    }

    #[test]
    fn any_key_clears_last_error() {
        let (mut app, _) = App::new();
        app.last_error = Some("boom".into());
        app.focus = Focus::Conversations;
        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ))));
        assert!(app.last_error.is_none());
        assert!(app.suppressed_error.is_some());
    }

    #[test]
    fn enter_in_conversations_jumps_to_input() {
        let (mut app, _) = App::new();
        app.focus = Focus::Conversations;
        app.conversations = vec![xmtp_ipc::ConversationItem {
            id: "dm-1".into(),
            kind: "dm".into(),
            name: None,
            dm_peer_inbox_id: None,
            last_message_ns: None,
        }];
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
        assert!(effects.is_empty());
        assert_eq!(app.focus, Focus::Input);
        assert!(app.input.is_empty());
    }

    #[test]
    fn enter_on_group_conversation_opens_group_management_modal() {
        let (mut app, _) = App::new();
        app.focus = Focus::Conversations;
        app.conversations = vec![xmtp_ipc::ConversationItem {
            id: "grp-1".into(),
            kind: "group".into(),
            name: Some("team".into()),
            dm_peer_inbox_id: None,
            last_message_ns: None,
        }];
        app.active_conversation = Some(app.conversations[0].clone());
        app.active_conversation_id = Some("grp-1".into());

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));

        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::GroupManagement);
        assert_eq!(app.focus, Focus::Conversations);
    }

    #[test]
    fn rename_group_modal_starts_with_empty_input() {
        let (mut app, _) = App::new();
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "grp-1".into(),
            kind: "group".into(),
            name: Some("old-name".into()),
            dm_peer_inbox_id: None,
            last_message_ns: None,
        });
        app.active_conversation_id = Some("grp-1".into());
        app.modal = Modal::GroupManagement;
        app.group_management.menu_index = 3;

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));

        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::GroupRename);
        assert!(app.group_management.rename_input.is_empty());
    }

    #[test]
    fn leave_group_confirm_sets_explicit_unsupported_message() {
        let (mut app, _) = App::new();
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "grp-1".into(),
            kind: "group".into(),
            name: Some("team".into()),
            dm_peer_inbox_id: None,
            last_message_ns: None,
        });
        app.active_conversation_id = Some("grp-1".into());
        app.modal = Modal::GroupLeaveConfirm;

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        ))));

        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::None);
        assert_eq!(
            app.last_error.as_deref(),
            Some("Leave group is not supported in this version")
        );
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
    fn app_starts_with_app_event_subscription_effect() {
        let (_, effects) = App::new();
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], Effect::SubscribeAppEvents));
    }

    #[test]
    fn unread_count_increments_for_inactive_conversation_and_clears_on_switch() {
        let (mut app, _) = App::new();
        app.conversations = vec![
            xmtp_ipc::ConversationItem {
                id: "conv-1".into(),
                kind: "dm".into(),
                name: Some("one".into()),
                dm_peer_inbox_id: Some("peer-1".into()),
                last_message_ns: Some(10),
            },
            xmtp_ipc::ConversationItem {
                id: "conv-2".into(),
                kind: "group".into(),
                name: Some("two".into()),
                dm_peer_inbox_id: None,
                last_message_ns: Some(20),
            },
        ];
        app.active_conversation_id = Some("conv-1".into());
        app.active_conversation = Some(app.conversations[0].clone());

        let effects = app.handle_event(crate::event::AppEvent::ConversationsLoaded(vec![
            xmtp_ipc::ConversationItem {
                id: "conv-1".into(),
                kind: "dm".into(),
                name: Some("one".into()),
                dm_peer_inbox_id: Some("peer-1".into()),
                last_message_ns: Some(10),
            },
            xmtp_ipc::ConversationItem {
                id: "conv-2".into(),
                kind: "group".into(),
                name: Some("two".into()),
                dm_peer_inbox_id: None,
                last_message_ns: Some(30),
            },
        ]));

        assert!(effects.is_empty());
        assert_eq!(app.unread_counts.get("conv-2"), Some(&1));

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ))));

        assert!(matches!(
            effects.as_slice(),
            [Effect::SwitchConversation { conversation_id }]
                if conversation_id == "conv-2"
        ));
        assert_eq!(app.active_conversation_id.as_deref(), Some("conv-2"));
        assert_eq!(app.unread_counts.get("conv-2"), None);
    }

    #[test]
    fn sent_action_optimistically_appends_message_to_active_conversation() {
        let (mut app, _) = App::new();
        app.active_conversation_id = Some("conv-1".into());
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "conv-1".into(),
            kind: "dm".into(),
            name: None,
            dm_peer_inbox_id: Some("peer-1".into()),
            last_message_ns: Some(10),
        });
        app.status = Some(
            serde_json::from_value(serde_json::json!({
                "daemon_state": "running",
                "connection_state": "connected",
                "inbox_id": "self-1",
                "installation_id": null
            }))
            .expect("build status response"),
        );

        let effects = app.handle_event(crate::event::AppEvent::ActionCompleted(
            crate::event::ActionOutcome::Sent {
                conversation_id: "conv-1".into(),
                message_id: "msg-1".into(),
                text: "hello now".into(),
            },
        ));

        assert!(effects.is_empty());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].message_id, "msg-1");
        assert_eq!(app.messages[0].sender_inbox_id, "self-1");
        assert_eq!(app.messages[0].content, "hello now");
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
    fn conversation_updated_event_updates_list_and_active_name() {
        let (mut app, _) = App::new();
        app.conversations = vec![xmtp_ipc::ConversationItem {
            id: "group-1".into(),
            kind: "group".into(),
            name: Some("old-name".into()),
            dm_peer_inbox_id: None,
            last_message_ns: None,
        }];
        app.active_conversation = Some(app.conversations[0].clone());
        app.active_conversation_id = Some("group-1".into());

        let effects = app.handle_event(crate::event::AppEvent::ConversationUpdated(
            xmtp_ipc::ConversationUpdatedEvent {
                conversation_id: "group-1".into(),
                name: Some("new-name".into()),
                member_count: 4,
            },
        ));

        assert!(effects.is_empty());
        assert_eq!(app.conversations[0].name.as_deref(), Some("new-name"));
        assert_eq!(
            app.active_conversation.as_ref().and_then(|item| item.name.as_deref()),
            Some("new-name")
        );
    }

    #[test]
    fn group_members_updated_event_refreshes_active_group_members() {
        let (mut app, _) = App::new();
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "group-1".into(),
            kind: "group".into(),
            name: Some("group".into()),
            dm_peer_inbox_id: None,
            last_message_ns: None,
        });
        app.active_conversation_id = Some("group-1".into());
        app.group_management.members = vec![xmtp_ipc::GroupMemberItem {
            inbox_id: "old-member".into(),
            permission_level: "member".into(),
            consent_state: "unknown".into(),
            account_identifiers: Vec::new(),
            installation_count: 1,
        }];

        let effects = app.handle_event(crate::event::AppEvent::GroupMembersUpdated(
            xmtp_ipc::GroupMembersUpdatedEvent {
                conversation_id: "group-1".into(),
                members: vec![xmtp_ipc::GroupMemberItem {
                    inbox_id: "new-member".into(),
                    permission_level: "member".into(),
                    consent_state: "unknown".into(),
                    account_identifiers: Vec::new(),
                    installation_count: 1,
                }],
            },
        ));

        assert!(effects.is_empty());
        assert_eq!(app.group_management.members.len(), 1);
        assert_eq!(app.group_management.members[0].inbox_id, "new-member");
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
    fn esc_in_input_clears_reply_state_before_leaving_input() {
        let (mut app, _) = App::new();
        app.focus = Focus::Input;
        app.reply_to_message_id = Some("msg-1".into());

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        ))));

        assert!(effects.is_empty());
        assert_eq!(app.focus, Focus::Input);
        assert!(app.reply_to_message_id.is_none());
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

    #[test]
    fn switching_conversation_clears_reply_state_and_input() {
        let (mut app, _) = App::new();
        app.input = "draft message".into();
        app.reply_to_message_id = Some("msg-1".into());

        let effects = app.activate_conversation(xmtp_ipc::ConversationItem {
            id: "conv-2".into(),
            kind: "dm".into(),
            name: Some("peer".into()),
            dm_peer_inbox_id: Some("peer-1".into()),
            last_message_ns: Some(20),
        });

        assert!(matches!(
            effects.as_slice(),
            [Effect::SwitchConversation { conversation_id }] if conversation_id == "conv-2"
        ));
        assert!(app.reply_to_message_id.is_none());
        assert!(app.input.is_empty());
    }
}
