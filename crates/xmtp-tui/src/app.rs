use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Color;
use ratatui::text::Line;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use textwrap::wrap;
use xmtp_ipc::{
    ConversationInfoResponse, ConversationItem, GroupInfoResponse, GroupMemberItem,
    GroupPermissionsResponse, HistoryItem, ReactionDetail, StatusResponse,
};

use crate::event::{ActionOutcome, AppEvent, Effect};
use crate::markdown::render_markdown;

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
    MessageDetail,
    ReadByList,
    ReactionPicker,
    CreateDm,
    CreateGroup,
    GroupManagement,
    GroupInfo,
    GroupMembers,
    GroupPermissions,
    GroupAddMembers,
    GroupRemoveMembers,
    GroupRename,
    GroupLeaveConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageMenuAction {
    ViewFull,
    ViewReadBy,
    Reply,
    Reaction,
    SendReadReceipt,
}

impl MessageMenuAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::ViewFull => "view full",
            Self::ViewReadBy => "read by",
            Self::Reply => "reply",
            Self::Reaction => "reaction",
            Self::SendReadReceipt => "send read receipt",
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
    ViewMembers,
    AddMembers,
    RemoveMembers,
    Rename,
    LeaveGroup,
    Permissions,
}

impl GroupManagementAction {
    pub fn all() -> [Self; 7] {
        [
            Self::ViewInfo,
            Self::ViewMembers,
            Self::AddMembers,
            Self::RemoveMembers,
            Self::Rename,
            Self::LeaveGroup,
            Self::Permissions,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ViewInfo => "view info",
            Self::ViewMembers => "view members",
            Self::AddMembers => "add members",
            Self::RemoveMembers => "remove members",
            Self::Rename => "rename",
            Self::LeaveGroup => "leave group",
            Self::Permissions => "permissions",
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
    pub permissions: Option<GroupPermissionsResponse>,
    pub self_permission_level: Option<String>,
    pub permissions_original: Option<GroupPermissionsResponse>,
    pub permissions_loading: bool,
    pub permissions_cursor: usize,
    pub members: Vec<GroupMemberItem>,
    pub selected_member: usize,
    pub info_member_scroll: usize,
    pub members_list_visible_rows: Cell<usize>,
    pub add_members_input: String,
    pub rename_input: String,
    pub permissions_dirty: bool,
    pub permissions_pending_updates: usize,
}

#[derive(Debug, Clone)]
pub struct App {
    pub focus: Focus,
    pub modal: Modal,
    pub should_quit: bool,
    pub xmtp_env: Option<String>,
    pub status: Option<StatusResponse>,
    pub conversations: Vec<ConversationItem>,
    pub selected_conversation: usize,
    pub active_conversation_id: Option<String>,
    pub active_conversation: Option<ConversationItem>,
    pub unread_counts: HashMap<String, u32>,
    pub drafts: HashMap<String, String>,
    pub active_info: Option<ConversationInfoResponse>,
    pub active_history_loading: bool,
    pub messages: Vec<HistoryItem>,
    pub markdown_cache: RefCell<HashMap<(String, usize), Vec<Line<'static>>>>,
    pub read_receipt_auto_send: bool,
    pub last_read_receipt_sent: HashMap<String, Instant>,
    pub selected_message: usize,
    pub detail_scroll: usize,
    pub detail_message_id: Option<String>,
    pub last_detail_visible_height: Cell<usize>,
    pub last_detail_wrap_width: Cell<usize>,
    pub input: String,
    pub cursor: usize,
    pub reply_to_message_id: Option<String>,
    pub message_menu_index: usize,
    pub reaction_picker_index: usize,
    pub dm_dialog: CreateDmDialog,
    pub group_dialog: CreateGroupDialog,
    pub group_management: GroupManagementState,
    pub last_error: Option<String>,
    pub pending_status: Option<String>,
    pub suppressed_error: Option<(String, Instant)>,
    pub exit_armed: bool,
}

impl App {
    pub fn new(enable_read_receipt: bool) -> (Self, Vec<Effect>) {
        (
            Self {
                focus: Focus::Conversations,
                modal: Modal::None,
                should_quit: false,
                xmtp_env: None,
                status: None,
                conversations: Vec::new(),
                selected_conversation: 0,
                active_conversation_id: None,
                active_conversation: None,
                unread_counts: HashMap::new(),
                drafts: HashMap::new(),
                active_info: None,
                active_history_loading: false,
                messages: Vec::new(),
                markdown_cache: RefCell::new(HashMap::new()),
                read_receipt_auto_send: enable_read_receipt,
                last_read_receipt_sent: HashMap::new(),
                selected_message: 0,
                detail_scroll: 0,
                detail_message_id: None,
                last_detail_visible_height: Cell::new(0),
                last_detail_wrap_width: Cell::new(1),
                input: String::new(),
                cursor: 0,
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
                pending_status: None,
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
                    self.group_management.self_permission_level =
                        self.self_inbox_id().and_then(|self_inbox_id| {
                            self.group_management
                                .members
                                .iter()
                                .find(|member| member.inbox_id == self_inbox_id)
                                .map(|member| member.permission_level.clone())
                        });
                    self.clamp_group_member_indices();
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
                self.group_management.self_permission_level =
                    self.self_inbox_id().and_then(|self_inbox_id| {
                        self.group_management
                            .members
                            .iter()
                            .find(|member| member.inbox_id == self_inbox_id)
                            .map(|member| member.permission_level.clone())
                    });
                self.clamp_group_member_indices();
                Vec::new()
            }
            AppEvent::GroupPermissionsLoaded(permissions) => {
                self.group_management.permissions_original = Some(permissions.clone());
                self.group_management.permissions = Some(permissions);
                self.group_management.permissions_loading = false;
                self.group_management.permissions_cursor =
                    self.group_management.permissions_cursor.min(7);
                Vec::new()
            }
            AppEvent::HistoryLoaded {
                conversation_id,
                items,
            } => {
                if self.active_conversation_id.as_deref() == Some(conversation_id.as_str()) {
                    let previous_selected = self
                        .selected_history_item()
                        .map(|item| item.message_id.clone());
                    let was_at_bottom = self.is_selected_message_at_end();
                    self.messages = normalize_history(items);
                    self.active_history_loading = false;
                    self.selected_message = if was_at_bottom {
                        self.messages.len().saturating_sub(1)
                    } else {
                        previous_selected
                            .and_then(|message_id| {
                                self.messages
                                    .iter()
                                    .position(|item| item.message_id == message_id)
                            })
                            .unwrap_or_else(|| {
                                self.selected_message
                                    .min(self.messages.len().saturating_sub(1))
                            })
                    };
                    if self.read_receipt_auto_send
                        && self.should_send_read_receipt(&conversation_id)
                    {
                        return self.enqueue_read_receipt_effect(conversation_id);
                    }
                }
                Vec::new()
            }
            AppEvent::HistoryEvent {
                conversation_id,
                item,
            } => {
                if self.active_conversation_id.as_deref() == Some(conversation_id.as_str()) {
                    let was_at_bottom = self.is_selected_message_at_end();
                    merge_history_item(&mut self.messages, item);
                    if was_at_bottom {
                        self.selected_message = self.messages.len().saturating_sub(1);
                    } else {
                        self.selected_message = self
                            .selected_message
                            .min(self.messages.len().saturating_sub(1));
                    }
                } else {
                    *self.unread_counts.entry(conversation_id).or_insert(0) += 1;
                }
                Vec::new()
            }
            AppEvent::ActionCompleted(outcome) => self.handle_action_completed(outcome),
            AppEvent::Error(error) => {
                self.pending_status = None;
                if let Some((suppressed, until)) = &self.suppressed_error
                    && suppressed == &error
                    && Instant::now() < *until
                {
                    return Vec::new();
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
        self.unread_counts.retain(|conversation_id, _| {
            self.conversations
                .iter()
                .any(|conversation| &conversation.id == conversation_id)
        });
        for conversation in &self.conversations {
            let previous_last_message_ns =
                previous_markers.get(&conversation.id).copied().flatten();
            if self.active_conversation_id.as_deref() != Some(conversation.id.as_str())
                && matches!(
                    (previous_last_message_ns, conversation.last_message_ns),
                    (Some(previous), Some(current)) if current > previous
                )
            {
                *self
                    .unread_counts
                    .entry(conversation.id.clone())
                    .or_insert(0) += 1;
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
            }
            return Vec::new();
        }

        self.selected_conversation = self.selected_conversation.min(self.conversations.len() - 1);
        let conversation = self.conversations[self.selected_conversation].clone();
        self.activate_conversation(conversation)
    }

    fn handle_action_completed(&mut self, outcome: ActionOutcome) -> Vec<Effect> {
        match outcome {
            ActionOutcome::OpenedDm(result) => {
                self.pending_status = None;
                self.last_error = None;
                self.modal = Modal::None;
                self.focus = Focus::Input;
                self.dm_dialog = CreateDmDialog::default();
                self.active_conversation_id = Some(result.conversation_id.clone());
                vec![Effect::SwitchConversation {
                    conversation_id: result.conversation_id,
                }]
            }
            ActionOutcome::CreatedGroup(result) => {
                self.pending_status = None;
                self.last_error = None;
                self.modal = Modal::None;
                self.focus = Focus::Input;
                self.group_dialog = CreateGroupDialog {
                    field: Some(GroupDialogField::Name),
                    ..Default::default()
                };
                self.active_conversation_id = Some(result.conversation_id.clone());
                vec![Effect::SwitchConversation {
                    conversation_id: result.conversation_id,
                }]
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
            ActionOutcome::PermissionUpdated => {
                if self.group_management.permissions_pending_updates > 0 {
                    self.group_management.permissions_pending_updates -= 1;
                }
                if self.group_management.permissions_pending_updates == 0 {
                    self.last_error = None;
                    self.pending_status = None;
                    self.group_management.permissions_dirty = false;
                    self.group_management.permissions_original =
                        self.group_management.permissions.clone();
                    self.modal = Modal::GroupManagement;
                }
                Vec::new()
            }
            ActionOutcome::LeftConversation(conversation_id) => {
                self.pending_status = None;
                self.last_error = None;
                self.modal = Modal::None;
                self.group_management = GroupManagementState::default();
                self.conversations
                    .retain(|conversation| conversation.id != conversation_id);
                if self.conversations.is_empty() {
                    self.selected_conversation = 0;
                    self.active_conversation_id = None;
                    self.active_conversation = None;
                    self.active_info = None;
                    self.active_history_loading = false;
                    self.messages.clear();
                    self.focus = Focus::Conversations;
                    Vec::new()
                } else {
                    self.selected_conversation = self
                        .selected_conversation
                        .min(self.conversations.len().saturating_sub(1));
                    let conversation = self.conversations[self.selected_conversation].clone();
                    self.focus = Focus::Conversations;
                    self.activate_conversation(conversation)
                }
            }
            ActionOutcome::Sent {
                conversation_id,
                message_id,
                text,
            } => {
                self.pending_status = None;
                self.drafts.remove(&conversation_id);
                if self.active_conversation_id.as_deref() == Some(conversation_id.as_str()) {
                    self.cursor = 0;
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
                            read_by: Vec::new(),
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

        if let Some(active) = self.active_conversation.as_mut()
            && active.id == update.conversation_id
        {
            active.name = update.name.clone();
        }

        if let Some(info) = self.active_info.as_mut()
            && info.conversation_id == update.conversation_id
        {
            info.name = update.name.clone();
            info.member_count = update.member_count;
        }

        if let Some(info) = self.group_management.info.as_mut()
            && info.conversation_id == update.conversation_id
        {
            info.name = update.name;
            info.member_count = update.member_count;
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

        if self.modal == Modal::None
            && self.focus != Focus::Input
            && matches!(key.code, KeyCode::Char('?') | KeyCode::Char('/'))
        {
            self.modal = Modal::Help;
            return Vec::new();
        }

        if key.code == KeyCode::Tab {
            if self.modal == Modal::None {
                if self.focus == Focus::Messages {
                    self.reset_selected_message_to_end();
                }
                self.focus = self.focus.next();
            } else if self.modal == Modal::CreateGroup {
                self.group_dialog.field = Some(
                    match self.group_dialog.field.unwrap_or(GroupDialogField::Name) {
                        GroupDialogField::Name => GroupDialogField::Members,
                        GroupDialogField::Members => GroupDialogField::Name,
                    },
                );
            }
            return Vec::new();
        }
        if key.code == KeyCode::BackTab {
            if self.modal == Modal::None {
                if self.focus == Focus::Messages {
                    self.reset_selected_message_to_end();
                }
                self.focus = self.focus.previous();
            } else if self.modal == Modal::CreateGroup {
                self.group_dialog.field = Some(
                    match self.group_dialog.field.unwrap_or(GroupDialogField::Name) {
                        GroupDialogField::Name => GroupDialogField::Members,
                        GroupDialogField::Members => GroupDialogField::Name,
                    },
                );
            }
            return Vec::new();
        }

        match self.modal {
            Modal::None => self.handle_key_without_modal(key),
            Modal::Help => self.handle_help_key(key),
            Modal::MessageMenu => self.handle_message_menu_key(key),
            Modal::MessageDetail => self.handle_message_detail_key(key),
            Modal::ReadByList => self.handle_read_by_list_key(key),
            Modal::ReactionPicker => self.handle_reaction_picker_key(key),
            Modal::CreateDm => self.handle_create_dm_key(key),
            Modal::CreateGroup => self.handle_create_group_key(key),
            Modal::GroupManagement => self.handle_group_management_key(key),
            Modal::GroupInfo => self.handle_group_info_key(key),
            Modal::GroupMembers => self.handle_group_members_view_key(key),
            Modal::GroupPermissions => self.handle_group_permissions_key(key),
            Modal::GroupAddMembers => self.handle_group_add_members_key(key),
            Modal::GroupRemoveMembers => self.handle_group_remove_members_key(key),
            Modal::GroupRename => self.handle_group_rename_key(key),
            Modal::GroupLeaveConfirm => self.handle_group_leave_confirm_key(key),
        }
    }

    fn handle_escape(&mut self) -> Vec<Effect> {
        match self.modal {
            Modal::GroupInfo | Modal::GroupMembers => {
                self.modal = Modal::GroupManagement;
                self.exit_armed = false;
                return Vec::new();
            }
            Modal::Help
            | Modal::MessageMenu
            | Modal::MessageDetail
            | Modal::ReadByList
            | Modal::ReactionPicker
            | Modal::CreateDm
            | Modal::CreateGroup
            | Modal::GroupManagement
            | Modal::GroupPermissions
            | Modal::GroupAddMembers
            | Modal::GroupRemoveMembers
            | Modal::GroupRename
            | Modal::GroupLeaveConfirm => {
                if self.modal == Modal::MessageDetail {
                    self.detail_scroll = 0;
                    self.detail_message_id = None;
                }
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
            if self.focus == Focus::Messages {
                self.reset_selected_message_to_end();
            }
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
                    if let Some(conversation_id) = self.active_group_id().map(str::to_owned) {
                        return vec![
                            Effect::LoadGroupMembers {
                                conversation_id: conversation_id.clone(),
                            },
                            Effect::LoadGroupPermissions { conversation_id },
                        ];
                    }
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
            KeyCode::Char('r') => {
                if let Some(message) = self.messages.get(self.selected_message) {
                    self.reply_to_message_id = Some(message.message_id.clone());
                    self.focus = Focus::Input;
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
                self.insert_input_char('\n');
            }
            KeyCode::Enter => {
                let text = self.input.trim_end().to_owned();
                if text.is_empty() {
                    return Vec::new();
                }
                self.input.clear();
                self.cursor = 0;
                self.pending_status = Some("Sending...".to_owned());
                if let Some(message_id) = self.reply_to_message_id.take() {
                    if let Some(conversation) = &self.active_conversation {
                        return vec![Effect::Reply {
                            message_id,
                            text,
                            conversation_id: conversation.id.clone(),
                        }];
                    }
                    return Vec::new();
                }
                if let Some(conversation) = &self.active_conversation {
                    let target = self
                        .active_info
                        .as_ref()
                        .and_then(|info| info.dm_peer_inbox_id.clone());
                    return vec![Effect::SendMessage {
                        conversation_id: conversation.id.clone(),
                        kind: conversation.kind.clone(),
                        target,
                        text,
                    }];
                }
            }
            KeyCode::Backspace => {
                self.delete_before_cursor();
            }
            KeyCode::Delete => {
                self.delete_at_cursor();
            }
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::ALT)
                {
                    self.move_cursor_word_left();
                } else if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::ALT)
                {
                    self.move_cursor_word_right();
                } else if self.cursor < self.input_char_len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => {
                self.cursor = 0;
            }
            KeyCode::End => {
                self.cursor = self.input_char_len();
            }
            KeyCode::Char(ch) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match ch {
                        'a' | 'A' => self.cursor = 0,
                        'e' | 'E' => self.cursor = self.input_char_len(),
                        'k' | 'K' => self.delete_to_end_of_line(),
                        'u' | 'U' => self.delete_to_start_of_line(),
                        'w' | 'W' => self.delete_previous_word(),
                        _ => {}
                    }
                } else {
                    self.insert_input_char(ch);
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
                if self.message_menu_index + 1 < self.message_menu_actions().len() {
                    self.message_menu_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(message) = self.selected_history_item() {
                    match self.message_menu_actions()[self.message_menu_index] {
                        MessageMenuAction::ViewFull => {
                            self.detail_message_id = Some(message.message_id.clone());
                            self.detail_scroll = 0;
                            self.modal = Modal::MessageDetail;
                        }
                        MessageMenuAction::ViewReadBy => {
                            self.modal = Modal::ReadByList;
                        }
                        MessageMenuAction::Reply => {
                            self.reply_to_message_id = Some(message.message_id.clone());
                            self.modal = Modal::None;
                            self.focus = Focus::Input;
                        }
                        MessageMenuAction::Reaction => {
                            self.modal = Modal::ReactionPicker;
                            self.reaction_picker_index = 0;
                        }
                        MessageMenuAction::SendReadReceipt => {
                            self.modal = Modal::None;
                            if self.selected_history_item().is_some_and(|item| {
                                self.self_inbox_id() != Some(item.sender_inbox_id.as_str())
                            }) && let Some(conversation_id) = self.active_conversation_id.clone()
                            {
                                return self.enqueue_read_receipt_effect(conversation_id);
                            }
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

    fn handle_read_by_list_key(&mut self, _key: KeyEvent) -> Vec<Effect> {
        Vec::new()
    }

    fn handle_message_detail_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Up => {
                if self.detail_scroll > 0 {
                    self.detail_scroll -= 1;
                }
            }
            KeyCode::Down => {
                let max_scroll = self.detail_max_scroll();
                self.detail_scroll = self.detail_scroll.saturating_add(1).min(max_scroll);
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
                    if let Some(conversation) = &self.active_conversation {
                        return vec![Effect::React {
                            message_id,
                            emoji: reaction_choices()[self.reaction_picker_index].to_owned(),
                            conversation_id: conversation.id.clone(),
                        }];
                    }
                    return Vec::new();
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
                    self.modal = Modal::None;
                    self.focus = Focus::Conversations;
                    self.pending_status = Some("Opening DM...".to_owned());
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
                        self.modal = Modal::None;
                        self.focus = Focus::Conversations;
                        self.pending_status = Some("Creating group...".to_owned());
                        return vec![Effect::CreateGroup {
                            name: if name.is_empty() {
                                None
                            } else {
                                Some(name.to_owned())
                            },
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
            KeyCode::Char(ch) if ('1'..='7').contains(&ch) => {
                let index = (ch as u8 - b'1') as usize;
                if index < GroupManagementAction::all().len() {
                    self.group_management.menu_index = index;
                    return self.activate_group_management_action();
                }
            }
            KeyCode::Enter => {
                return self.activate_group_management_action();
            }
            _ => {}
        }
        Vec::new()
    }

    fn activate_group_management_action(&mut self) -> Vec<Effect> {
        let Some(conversation_id) = self.active_group_id().map(str::to_owned) else {
            self.modal = Modal::None;
            return Vec::new();
        };
        match GroupManagementAction::all()[self.group_management.menu_index] {
            GroupManagementAction::ViewInfo => {
                self.modal = Modal::GroupInfo;
                vec![Effect::LoadGroupInfo { conversation_id }]
            }
            GroupManagementAction::ViewMembers => {
                self.modal = Modal::GroupMembers;
                self.group_management.info_member_scroll = 0;
                vec![Effect::LoadGroupMembers { conversation_id }]
            }
            GroupManagementAction::AddMembers => {
                if !self.can_manage_group_members(GroupManagementAction::AddMembers) {
                    self.last_error =
                        Some("You don't have permission to perform this action".to_owned());
                    self.modal = Modal::None;
                    return Vec::new();
                }
                self.modal = Modal::GroupAddMembers;
                self.group_management.add_members_input.clear();
                Vec::new()
            }
            GroupManagementAction::RemoveMembers => {
                if !self.can_manage_group_members(GroupManagementAction::RemoveMembers) {
                    self.last_error =
                        Some("You don't have permission to perform this action".to_owned());
                    self.modal = Modal::None;
                    return Vec::new();
                }
                self.modal = Modal::GroupRemoveMembers;
                self.group_management.selected_member = 0;
                vec![Effect::LoadGroupMembers { conversation_id }]
            }
            GroupManagementAction::Rename => {
                self.modal = Modal::GroupRename;
                self.group_management.rename_input.clear();
                Vec::new()
            }
            GroupManagementAction::LeaveGroup => {
                self.modal = Modal::GroupLeaveConfirm;
                Vec::new()
            }
            GroupManagementAction::Permissions => {
                self.modal = Modal::GroupPermissions;
                self.group_management.permissions_loading = true;
                self.group_management.permissions = None;
                self.group_management.permissions_original = None;
                self.group_management.permissions_cursor = 0;
                self.group_management.permissions_dirty = false;
                self.group_management.permissions_pending_updates = 0;
                vec![Effect::LoadGroupPermissions { conversation_id }]
            }
        }
    }

    fn handle_group_info_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let _ = key;
        Vec::new()
    }

    fn handle_group_members_view_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Up => {
                if self.group_management.info_member_scroll > 0 {
                    self.group_management.info_member_scroll -= 1;
                }
            }
            KeyCode::Down => {
                let visible = self.group_management.members_list_visible_rows.get();
                let max_scroll = if visible == 0 || self.group_management.members.len() <= visible {
                    0
                } else {
                    self.group_management.members.len() - visible
                };
                if self.group_management.info_member_scroll < max_scroll {
                    self.group_management.info_member_scroll += 1;
                }
            }
            KeyCode::Enter => {
                self.modal = Modal::GroupManagement;
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_group_permissions_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                self.group_management.permissions_dirty = false;
                self.group_management.permissions_pending_updates = 0;
                if let Some(original) = self.group_management.permissions_original.clone() {
                    self.group_management.permissions = Some(original);
                }
                self.modal = Modal::GroupManagement;
            }
            KeyCode::Up => {
                if self.group_management.permissions_cursor > 0 {
                    self.group_management.permissions_cursor -= 1;
                }
            }
            KeyCode::Down => {
                if self.group_management.permissions_cursor < 7 {
                    self.group_management.permissions_cursor += 1;
                }
            }
            KeyCode::Left => {
                self.shift_permission_policy(false);
            }
            KeyCode::Right => {
                self.shift_permission_policy(true);
            }
            KeyCode::Enter => {
                let Some(conversation_id) = self.active_group_id().map(str::to_owned) else {
                    self.modal = Modal::None;
                    return Vec::new();
                };
                let Some(current) = self.group_management.permissions.clone() else {
                    return Vec::new();
                };
                let Some(original) = self.group_management.permissions_original.clone() else {
                    self.modal = Modal::GroupManagement;
                    return Vec::new();
                };
                let updates = diff_group_permissions(&original, &current)
                    .into_iter()
                    .map(|(permission, policy)| Effect::UpdateGroupPermission {
                        conversation_id: conversation_id.clone(),
                        permission,
                        policy,
                    })
                    .collect::<Vec<_>>();
                if updates.is_empty() {
                    self.group_management.permissions_dirty = false;
                    self.modal = Modal::GroupManagement;
                } else {
                    self.pending_status = Some("Saving permissions...".to_owned());
                    self.group_management.permissions_pending_updates = updates.len();
                    return updates;
                }
            }
            _ => {}
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
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match ch {
                        'u' | 'U' => self.group_management.rename_input.clear(),
                        'w' | 'W' => {
                            delete_previous_word_from_end(&mut self.group_management.rename_input);
                        }
                        _ => {}
                    }
                } else {
                    self.group_management.rename_input.push(ch);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_group_leave_confirm_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            let Some(conversation_id) = self.active_group_id().map(str::to_owned) else {
                self.modal = Modal::None;
                return Vec::new();
            };
            self.modal = Modal::None;
            self.pending_status = Some("Leaving group...".to_owned());
            return vec![Effect::LeaveConversation { conversation_id }];
        }
        Vec::new()
    }

    fn active_group_id(&self) -> Option<&str> {
        self.active_conversation
            .as_ref()
            .filter(|conversation| conversation.kind == "group")
            .map(|conversation| conversation.id.as_str())
    }

    pub fn selected_history_item(&self) -> Option<&HistoryItem> {
        self.messages.get(self.selected_message)
    }

    fn shift_permission_policy(&mut self, forward: bool) {
        let Some(permissions) = self.group_management.permissions.as_mut() else {
            return;
        };
        let value =
            editable_permission_value_mut(permissions, self.group_management.permissions_cursor);
        let next = next_permission_policy(value, forward);
        if next != *value {
            *value = next;
            self.group_management.permissions_dirty = self
                .group_management
                .permissions_original
                .as_ref()
                .is_some_and(|original| original != permissions);
        }
    }

    fn should_auto_scroll_messages(&self) -> bool {
        self.focus != Focus::Messages
    }

    fn clamp_group_member_indices(&mut self) {
        self.group_management.selected_member = self
            .group_management
            .selected_member
            .min(self.group_management.members.len().saturating_sub(1));
        self.group_management.info_member_scroll = self
            .group_management
            .info_member_scroll
            .min(self.group_management.members.len().saturating_sub(1));
    }

    fn is_selected_message_at_end(&self) -> bool {
        self.messages.is_empty() || self.selected_message + 1 >= self.messages.len()
    }

    fn should_send_read_receipt(&self, conversation_id: &str) -> bool {
        if self
            .last_read_receipt_sent
            .get(conversation_id)
            .is_some_and(|last_sent| last_sent.elapsed() < Duration::from_secs(30))
        {
            return false;
        }
        match self.self_inbox_id() {
            Some(self_inbox_id) => self
                .messages
                .iter()
                .any(|item| item.sender_inbox_id != self_inbox_id),
            None => !self.messages.is_empty(),
        }
    }

    fn enqueue_read_receipt_effect(&mut self, conversation_id: String) -> Vec<Effect> {
        self.last_read_receipt_sent
            .insert(conversation_id.clone(), Instant::now());
        vec![Effect::SendReadReceipt { conversation_id }]
    }

    pub fn cached_markdown_lines(
        &self,
        message_id: &str,
        content: &str,
        wrap_width: usize,
    ) -> Vec<Line<'static>> {
        let key = (message_id.to_owned(), wrap_width);
        if let Some(lines) = self.markdown_cache.borrow().get(&key) {
            return lines.clone();
        }
        let lines = render_markdown(content, wrap_width);
        self.markdown_cache.borrow_mut().insert(key, lines.clone());
        lines
    }

    pub fn can_manage_group_members(&self, action: GroupManagementAction) -> bool {
        let Some(permissions) = self.group_management.permissions.as_ref() else {
            return true;
        };
        let policy = match action {
            GroupManagementAction::AddMembers => permissions.add_member.as_str(),
            GroupManagementAction::RemoveMembers => permissions.remove_member.as_str(),
            _ => return true,
        };
        match policy {
            "everyone" => true,
            "admin_only" => self
                .group_management
                .self_permission_level
                .as_deref()
                .is_some_and(|level| level == "admin" || level == "super_admin"),
            "super_admin_only" => {
                self.group_management.self_permission_level.as_deref() == Some("super_admin")
            }
            "deny" => false,
            _ => true,
        }
    }

    fn activate_conversation(&mut self, conversation: ConversationItem) -> Vec<Effect> {
        if self.active_conversation_id.as_deref() == Some(conversation.id.as_str()) {
            self.active_conversation = Some(conversation);
            return Vec::new();
        }
        if let Some(current_id) = self.active_conversation_id.clone() {
            if self.input.trim().is_empty() {
                self.drafts.remove(&current_id);
            } else {
                self.drafts.insert(current_id, self.input.clone());
            }
        }
        self.unread_counts.remove(&conversation.id);
        self.reply_to_message_id = None;
        self.input = self
            .drafts
            .get(&conversation.id)
            .cloned()
            .unwrap_or_default();
        self.cursor = self.input.chars().count();
        self.active_conversation_id = Some(conversation.id.clone());
        self.active_conversation = Some(conversation.clone());
        self.active_info = None;
        self.active_history_loading = true;
        self.detail_scroll = 0;
        self.detail_message_id = None;
        self.group_management.info = None;
        self.group_management.members.clear();
        self.group_management.selected_member = 0;
        self.markdown_cache.borrow_mut().clear();
        self.messages.clear();
        self.selected_message = self.messages.len().saturating_sub(1);
        vec![Effect::SwitchConversation {
            conversation_id: conversation.id,
        }]
    }

    fn reset_selected_message_to_end(&mut self) {
        self.selected_message = self.messages.len().saturating_sub(1);
    }

    pub fn self_inbox_id(&self) -> Option<&str> {
        self.status
            .as_ref()
            .and_then(|status| status.inbox_id.as_deref())
    }

    pub fn color_for_message(&self, item: &HistoryItem) -> Color {
        if item.content_kind == "unknown" || item.content.starts_with("type=unknown content_type=")
        {
            return Color::White;
        }
        if self.self_inbox_id() == Some(item.sender_inbox_id.as_str()) {
            Color::Green
        } else {
            Color::Cyan
        }
    }

    pub fn message_menu_actions(&self) -> Vec<MessageMenuAction> {
        let mut actions = Vec::new();
        if self.selected_message_can_view_full() {
            actions.push(MessageMenuAction::ViewFull);
        }
        actions.push(MessageMenuAction::Reply);
        actions.push(MessageMenuAction::Reaction);
        actions.push(MessageMenuAction::SendReadReceipt);
        if self
            .selected_history_item()
            .is_some_and(|item| !item.read_by.is_empty())
        {
            actions.push(MessageMenuAction::ViewReadBy);
        }
        actions
    }

    pub fn detail_message(&self) -> Option<&HistoryItem> {
        let detail_id = self.detail_message_id.as_deref()?;
        self.messages
            .iter()
            .find(|item| item.message_id == detail_id)
    }

    pub fn detail_max_scroll(&self) -> usize {
        let Some(message) = self.detail_message() else {
            return 0;
        };

        let wrap_width = self.last_detail_wrap_width.get().max(1);
        let visible_height = self.last_detail_visible_height.get();
        let content_lines = if message.content_kind == "markdown" {
            self.cached_markdown_lines(&message.message_id, &message.content, wrap_width)
                .len()
        } else {
            wrap(&message.content, wrap_width).len().max(1)
        };
        let total_lines = 2 + content_lines + 2;
        total_lines.saturating_sub(visible_height)
    }

    pub fn input_char_len(&self) -> usize {
        self.input.chars().count()
    }

    fn selected_message_can_view_full(&self) -> bool {
        const THRESHOLD: usize = 4;
        const PREVIEW_WIDTH: usize = 72;

        let Some(item) = self.selected_history_item() else {
            return false;
        };

        let line_count = if item.content_kind == "markdown" {
            let rendered =
                self.cached_markdown_lines(&item.message_id, &item.content, PREVIEW_WIDTH);
            let non_empty = rendered
                .iter()
                .filter(|line| {
                    line.spans
                        .iter()
                        .any(|span| !span.content.as_ref().trim().is_empty())
                })
                .count();
            if non_empty == 0 {
                wrap_text_lines_for_count(&item.content, PREVIEW_WIDTH)
            } else {
                non_empty
            }
        } else {
            wrap_text_lines_for_count(&item.content, PREVIEW_WIDTH)
        };

        line_count > THRESHOLD
    }

    fn input_byte_index(&self, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }
        self.input
            .char_indices()
            .nth(cursor)
            .map(|(idx, _)| idx)
            .unwrap_or(self.input.len())
    }

    fn insert_input_char(&mut self, ch: char) {
        let byte_idx = self.input_byte_index(self.cursor);
        self.input.insert(byte_idx, ch);
        self.cursor += 1;
    }

    fn delete_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let end = self.input_byte_index(self.cursor);
        let start = self.input_byte_index(self.cursor - 1);
        self.input.replace_range(start..end, "");
        self.cursor -= 1;
    }

    fn delete_at_cursor(&mut self) {
        if self.cursor >= self.input_char_len() {
            return;
        }
        let start = self.input_byte_index(self.cursor);
        let end = self.input_byte_index(self.cursor + 1);
        self.input.replace_range(start..end, "");
    }

    fn delete_to_start_of_line(&mut self) {
        let end = self.input_byte_index(self.cursor);
        self.input.replace_range(0..end, "");
        self.cursor = 0;
    }

    fn delete_to_end_of_line(&mut self) {
        let start = self.input_byte_index(self.cursor);
        self.input.truncate(start);
    }

    fn delete_previous_word(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let chars: Vec<char> = self.input.chars().collect();
        let mut start = self.cursor;

        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }

        let byte_start = self.input_byte_index(start);
        let byte_end = self.input_byte_index(self.cursor);
        self.input.replace_range(byte_start..byte_end, "");
        self.cursor = start;
    }

    fn move_cursor_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let chars: Vec<char> = self.input.chars().collect();
        let mut cursor = self.cursor;

        while cursor > 0 && chars[cursor - 1].is_whitespace() {
            cursor -= 1;
        }
        while cursor > 0 && !chars[cursor - 1].is_whitespace() {
            cursor -= 1;
        }

        self.cursor = cursor;
    }

    fn move_cursor_word_right(&mut self) {
        let chars: Vec<char> = self.input.chars().collect();
        let mut cursor = self.cursor;

        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        while cursor < chars.len() && !chars[cursor].is_whitespace() {
            cursor += 1;
        }

        self.cursor = cursor;
    }
}

fn delete_previous_word_from_end(value: &mut String) {
    let mut chars: Vec<char> = value.chars().collect();
    while chars.last().is_some_and(|ch| ch.is_whitespace()) {
        chars.pop();
    }
    while chars.last().is_some_and(|ch| !ch.is_whitespace()) {
        chars.pop();
    }
    *value = chars.into_iter().collect();
}

fn editable_permission_value_mut(
    permissions: &mut GroupPermissionsResponse,
    cursor: usize,
) -> &mut String {
    match cursor {
        0 => &mut permissions.add_member,
        1 => &mut permissions.remove_member,
        2 => &mut permissions.add_admin,
        3 => &mut permissions.remove_admin,
        4 => &mut permissions.update_group_name,
        5 => &mut permissions.update_group_description,
        6 => &mut permissions.update_group_image,
        _ => &mut permissions.update_app_data,
    }
}

fn next_permission_policy(current: &str, forward: bool) -> String {
    const POLICIES: [&str; 4] = ["everyone", "admin_only", "super_admin_only", "deny"];
    let index = POLICIES
        .iter()
        .position(|policy| *policy == current)
        .unwrap_or(0);
    let next_index = if forward {
        (index + 1) % POLICIES.len()
    } else if index == 0 {
        POLICIES.len() - 1
    } else {
        index - 1
    };
    POLICIES[next_index].to_owned()
}

fn diff_group_permissions(
    original: &GroupPermissionsResponse,
    current: &GroupPermissionsResponse,
) -> Vec<(String, String)> {
    let mut updates = Vec::new();
    push_permission_update(
        &mut updates,
        "add_member",
        &original.add_member,
        &current.add_member,
    );
    push_permission_update(
        &mut updates,
        "remove_member",
        &original.remove_member,
        &current.remove_member,
    );
    push_permission_update(
        &mut updates,
        "add_admin",
        &original.add_admin,
        &current.add_admin,
    );
    push_permission_update(
        &mut updates,
        "remove_admin",
        &original.remove_admin,
        &current.remove_admin,
    );
    push_permission_update(
        &mut updates,
        "update_group_name",
        &original.update_group_name,
        &current.update_group_name,
    );
    push_permission_update(
        &mut updates,
        "update_group_description",
        &original.update_group_description,
        &current.update_group_description,
    );
    push_permission_update(
        &mut updates,
        "update_group_image",
        &original.update_group_image,
        &current.update_group_image,
    );
    push_permission_update(
        &mut updates,
        "update_app_data",
        &original.update_app_data,
        &current.update_app_data,
    );
    updates
}

fn push_permission_update(
    updates: &mut Vec<(String, String)>,
    permission: &str,
    original: &str,
    current: &str,
) {
    if original != current {
        updates.push((permission.to_owned(), current.to_owned()));
    }
}

pub fn reaction_choices() -> [&'static str; 5] {
    ["👍", "❤️", "🔥", "😂", "👀"]
}

fn wrap_text_lines_for_count(text: &str, width: usize) -> usize {
    let mut lines = 0usize;
    for raw_line in text.split('\n') {
        let wrapped = wrap(raw_line, width.max(1));
        lines += wrapped.len().max(1);
    }
    lines.max(1)
}

fn normalize_history(items: Vec<HistoryItem>) -> Vec<HistoryItem> {
    let mut visible: Vec<HistoryItem> = items
        .iter()
        .filter(|item| item.content_kind != "reaction" && item.content_kind != "read_receipt")
        .cloned()
        .collect();

    for item in items {
        if item.content_kind == "reaction"
            && let Some(target_id) = item.reaction_target_message_id.as_deref()
            && let Some(target) = visible.iter_mut().find(|m| m.message_id == target_id)
            && let (Some(emoji), Some(action)) = (item.reaction_emoji, item.reaction_action)
        {
            target.attached_reactions.push(ReactionDetail {
                sender_inbox_id: item.sender_inbox_id,
                emoji,
                action,
            });
        }
    }

    for item in &mut visible {
        dedupe_reactions(&mut item.attached_reactions);
    }

    visible
}

fn dedupe_reactions(reactions: &mut Vec<ReactionDetail>) {
    let mut seen = std::collections::HashSet::new();
    reactions.retain(|reaction| {
        seen.insert((
            reaction.sender_inbox_id.clone(),
            reaction.emoji.clone(),
            reaction.action.clone(),
        ))
    });
}

fn merge_history_item(visible: &mut Vec<HistoryItem>, item: HistoryItem) {
    if visible
        .iter()
        .any(|existing| existing.message_id == item.message_id)
    {
        return;
    }

    if item.content_kind == "reaction"
        && let Some(target_message_id) = item.reaction_target_message_id.clone()
        && let Some(target) = visible
            .iter_mut()
            .find(|existing| existing.message_id == target_message_id)
        && let (Some(emoji), Some(action)) =
            (item.reaction_emoji.clone(), item.reaction_action.clone())
    {
        target.attached_reactions.push(ReactionDetail {
            sender_inbox_id: item.sender_inbox_id,
            emoji,
            action,
        });
        return;
    }

    if item.content_kind == "read_receipt" {
        for existing in visible.iter_mut() {
            if existing.sent_at_ns <= item.sent_at_ns
                && existing.sender_inbox_id != item.sender_inbox_id
                && !existing
                    .read_by
                    .iter()
                    .any(|inbox_id| inbox_id == &item.sender_inbox_id)
            {
                existing.read_by.push(item.sender_inbox_id.clone());
            }
        }
        return;
    }

    visible.push(item);
}

#[cfg(test)]
mod tests {
    use super::{App, Focus, GroupDialogField, MessageMenuAction, Modal};
    use crate::event::Effect;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use xmtp_ipc::{ConversationItem, HistoryItem};

    fn sample_history_item(message_id: &str, content: &str) -> HistoryItem {
        HistoryItem {
            message_id: message_id.to_owned(),
            sender_inbox_id: "sender-1".to_owned(),
            sent_at_ns: 1,
            content_kind: "text".to_owned(),
            content: content.to_owned(),
            reply_count: 0,
            reaction_count: 0,
            reply_target_message_id: None,
            reaction_target_message_id: None,
            reaction_emoji: None,
            reaction_action: None,
            attached_reactions: Vec::new(),
            read_by: Vec::new(),
        }
    }

    #[test]
    fn focus_cycles_forward() {
        assert_eq!(Focus::Conversations.next(), Focus::Messages);
        assert_eq!(Focus::Messages.next(), Focus::Input);
        assert_eq!(Focus::Input.next(), Focus::Conversations);
    }

    #[test]
    fn input_focus_treats_char_as_text() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        )));
        assert!(effects.is_empty());
        assert_eq!(app.input, "c");
    }

    #[test]
    fn input_cursor_moves_left_and_right() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        app.input = "abc".into();
        app.cursor = 3;

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::NONE,
        ))));
        assert_eq!(app.cursor, 2);

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::NONE,
        ))));
        assert_eq!(app.cursor, 3);
    }

    #[test]
    fn input_home_and_end_move_cursor() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        app.input = "abc".into();
        app.cursor = 1;

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Home,
            KeyModifiers::NONE,
        ))));
        assert_eq!(app.cursor, 0);

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::End,
            KeyModifiers::NONE,
        ))));
        assert_eq!(app.cursor, 3);
    }

    #[test]
    fn input_ctrl_left_and_right_jump_by_word() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        app.input = "hello brave new world".into();
        app.cursor = app.input.chars().count();

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::CONTROL,
        ))));
        assert_eq!(app.cursor, "hello brave new ".chars().count());

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::CONTROL,
        ))));
        assert_eq!(app.cursor, "hello brave new world".chars().count());
    }

    #[test]
    fn input_alt_left_and_right_jump_by_word() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        app.input = "hello brave new".into();
        app.cursor = "hello ".chars().count();

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::ALT,
        ))));
        assert_eq!(app.cursor, "hello brave".chars().count());

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::ALT,
        ))));
        assert_eq!(app.cursor, "hello ".chars().count());

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::ALT,
        ))));
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn input_inserts_text_in_the_middle() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        app.input = "helo".into();
        app.cursor = 2;

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        ))));

        assert_eq!(app.input, "hello");
        assert_eq!(app.cursor, 3);
    }

    #[test]
    fn input_ctrl_w_deletes_previous_word() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        app.input = "hello brave new".into();
        app.cursor = app.input.chars().count();

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('w'),
            KeyModifiers::CONTROL,
        ))));

        assert_eq!(app.input, "hello brave ");
        assert_eq!(app.cursor, "hello brave ".chars().count());
    }

    #[test]
    fn input_ctrl_u_deletes_to_start() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        app.input = "hello brave new".into();
        app.cursor = "hello brave".chars().count();

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        ))));

        assert_eq!(app.input, " new");
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn conversation_navigation_switches_immediately() {
        let (mut app, _) = App::new(false);
        app.conversations = vec![
            xmtp_ipc::ConversationItem {
                id: "one".into(),
                kind: "dm".into(),
                name: None,
                dm_peer_inbox_id: None,
                last_message_ns: None,
            },
            xmtp_ipc::ConversationItem {
                id: "two".into(),
                kind: "group".into(),
                name: None,
                dm_peer_inbox_id: None,
                last_message_ns: None,
            },
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
            read_by: Vec::new(),
        });
        app.focus = Focus::Conversations;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        )));
        assert_eq!(app.selected_conversation, 1);
        assert_eq!(app.active_conversation_id.as_deref(), Some("two"));
        assert!(app.active_history_loading);
        assert!(app.messages.is_empty());
        assert!(matches!(
            effects.as_slice(),
            [Effect::SwitchConversation { conversation_id }] if conversation_id == "two"
        ));
    }

    #[test]
    fn ctrl_n_opens_create_dm_modal_outside_input() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Conversations;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        )));
        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::CreateDm);
    }

    #[test]
    fn question_mark_opens_help_modal() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Conversations;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT),
        )));
        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::Help);
    }

    #[test]
    fn any_key_clears_last_error() {
        let (mut app, _) = App::new(false);
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
        let (mut app, _) = App::new(false);
        app.focus = Focus::Conversations;
        app.conversations = vec![xmtp_ipc::ConversationItem {
            id: "dm-1".into(),
            kind: "dm".into(),
            name: None,
            dm_peer_inbox_id: None,
            last_message_ns: None,
        }];
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));
        assert!(effects.is_empty());
        assert_eq!(app.focus, Focus::Input);
        assert!(app.input.is_empty());
    }

    #[test]
    fn enter_on_group_conversation_opens_group_management_modal() {
        let (mut app, _) = App::new(false);
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

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));

        assert!(matches!(
            effects.as_slice(),
            [
                Effect::LoadGroupMembers {
                    conversation_id: members_id
                },
                Effect::LoadGroupPermissions {
                    conversation_id: permissions_id
                }
            ] if members_id == "grp-1" && permissions_id == "grp-1"
        ));
        assert_eq!(app.modal, Modal::GroupManagement);
        assert_eq!(app.focus, Focus::Conversations);
    }

    #[test]
    fn rename_group_modal_starts_with_empty_input() {
        let (mut app, _) = App::new(false);
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "grp-1".into(),
            kind: "group".into(),
            name: Some("old-name".into()),
            dm_peer_inbox_id: None,
            last_message_ns: None,
        });
        app.active_conversation_id = Some("grp-1".into());
        app.modal = Modal::GroupManagement;
        app.group_management.menu_index = 4;

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));

        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::GroupRename);
        assert!(app.group_management.rename_input.is_empty());
    }

    #[test]
    fn rename_group_supports_ctrl_u_and_ctrl_w() {
        let (mut app, _) = App::new(false);
        app.modal = Modal::GroupRename;
        app.group_management.rename_input = "old team name".into();

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('w'),
            KeyModifiers::CONTROL,
        ))));
        assert_eq!(app.group_management.rename_input, "old team ");

        let _ = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        ))));
        assert!(app.group_management.rename_input.is_empty());
    }

    #[test]
    fn create_dm_enter_closes_modal_and_sets_progress_message() {
        let (mut app, _) = App::new(false);
        app.modal = Modal::CreateDm;
        app.focus = Focus::Input;
        app.dm_dialog.recipient = "peer-1".into();

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));

        assert!(
            matches!(effects.as_slice(), [Effect::OpenDm { recipient }] if recipient == "peer-1")
        );
        assert_eq!(app.modal, Modal::None);
        assert_eq!(app.focus, Focus::Conversations);
        assert_eq!(app.pending_status.as_deref(), Some("Opening DM..."));
    }

    #[test]
    fn create_group_enter_closes_modal_and_sets_progress_message() {
        let (mut app, _) = App::new(false);
        app.modal = Modal::CreateGroup;
        app.focus = Focus::Input;
        app.group_dialog.field = Some(GroupDialogField::Members);
        app.group_dialog.name = "team".into();
        app.group_dialog.members = "peer-1".into();

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));

        assert!(
            matches!(effects.as_slice(), [Effect::CreateGroup { name, members }] if name.as_deref() == Some("team") && members == &vec!["peer-1".to_owned()])
        );
        assert_eq!(app.modal, Modal::None);
        assert_eq!(app.focus, Focus::Conversations);
        assert_eq!(app.pending_status.as_deref(), Some("Creating group..."));
    }

    #[test]
    fn leave_group_confirm_dispatches_real_leave_effect() {
        let (mut app, _) = App::new(false);
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "grp-1".into(),
            kind: "group".into(),
            name: Some("team".into()),
            dm_peer_inbox_id: None,
            last_message_ns: None,
        });
        app.active_conversation_id = Some("grp-1".into());
        app.modal = Modal::GroupLeaveConfirm;

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        )));

        assert!(matches!(
            effects.as_slice(),
            [Effect::LeaveConversation { conversation_id }] if conversation_id == "grp-1"
        ));
        assert_eq!(app.modal, Modal::None);
        assert_eq!(app.pending_status.as_deref(), Some("Leaving group..."));
    }

    #[test]
    fn enter_on_message_list_opens_message_menu() {
        let (mut app, _) = App::new(false);
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
            read_by: Vec::new(),
        });
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));
        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::MessageMenu);
    }

    #[test]
    fn short_message_hides_view_full_menu_action() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Messages;
        app.messages.push(sample_history_item("msg-1", "hello"));

        assert_eq!(
            app.message_menu_actions(),
            vec![
                MessageMenuAction::Reply,
                MessageMenuAction::Reaction,
                MessageMenuAction::SendReadReceipt,
            ]
        );
    }

    #[test]
    fn long_message_shows_view_full_menu_action() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Messages;
        app.messages.push(sample_history_item(
            "msg-1",
            &"this is a very long message that should wrap into many lines when shown in the message list and therefore expose the view full action in the menu ".repeat(4),
        ));

        assert_eq!(
            app.message_menu_actions(),
            vec![
                MessageMenuAction::ViewFull,
                MessageMenuAction::Reply,
                MessageMenuAction::Reaction,
                MessageMenuAction::SendReadReceipt,
            ]
        );
    }

    #[test]
    fn message_menu_includes_read_by_when_message_has_readers() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Messages;
        let mut item = sample_history_item("msg-1", "hello");
        item.read_by = vec!["peer-1".into()];
        app.messages.push(item);

        assert_eq!(
            app.message_menu_actions(),
            vec![
                MessageMenuAction::Reply,
                MessageMenuAction::Reaction,
                MessageMenuAction::SendReadReceipt,
                MessageMenuAction::ViewReadBy,
            ]
        );
    }

    #[test]
    fn message_menu_excludes_read_by_when_message_has_no_readers() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Messages;
        app.messages.push(sample_history_item("msg-1", "hello"));

        assert!(
            !app.message_menu_actions()
                .contains(&MessageMenuAction::ViewReadBy)
        );
    }

    #[test]
    fn selecting_read_by_action_opens_read_by_list_modal() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Messages;
        app.modal = Modal::MessageMenu;
        let mut item = sample_history_item("msg-1", "hello");
        item.read_by = vec!["peer-1".into()];
        app.messages.push(item);
        app.message_menu_index = app
            .message_menu_actions()
            .iter()
            .position(|action| *action == MessageMenuAction::ViewReadBy)
            .expect("read by action");

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));

        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::ReadByList);
    }

    #[test]
    fn pressing_r_in_messages_focus_enters_reply_mode() {
        let (mut app, _) = App::new(false);
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
            read_by: Vec::new(),
        });

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        )));

        assert!(effects.is_empty());
        assert_eq!(app.reply_to_message_id.as_deref(), Some("msg-1"));
        assert_eq!(app.focus, Focus::Input);
    }

    #[test]
    fn history_load_merges_reaction_into_target_message() {
        let (mut app, _) = App::new(false);
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
                    read_by: Vec::new(),
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
                    read_by: Vec::new(),
                },
            ],
        });

        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].message_id, "msg-1");
        assert_eq!(app.messages[0].attached_reactions.len(), 1);
        assert_eq!(app.messages[0].attached_reactions[0].emoji, "👍");
    }

    #[test]
    fn history_load_merges_reaction_even_when_reaction_appears_first() {
        let (mut app, _) = App::new(false);
        app.active_conversation_id = Some("conv-1".into());
        app.handle_event(crate::event::AppEvent::HistoryLoaded {
            conversation_id: "conv-1".into(),
            items: vec![
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
                    read_by: Vec::new(),
                },
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
                    read_by: Vec::new(),
                },
            ],
        });

        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].message_id, "msg-1");
        assert_eq!(app.messages[0].attached_reactions.len(), 1);
        assert_eq!(app.messages[0].attached_reactions[0].emoji, "👍");
    }

    #[test]
    fn history_loaded_auto_sends_read_receipt_when_remote_message_exists() {
        let (mut app, _) = App::new(true);
        app.active_conversation_id = Some("conv-1".into());
        app.status = Some(
            serde_json::from_value(serde_json::json!({
                "daemon_state": "running",
                "connection_state": "connected",
                "inbox_id": "self-1",
                "installation_id": null
            }))
            .expect("build status response"),
        );

        let effects = app.handle_event(crate::event::AppEvent::HistoryLoaded {
            conversation_id: "conv-1".into(),
            items: vec![xmtp_ipc::HistoryItem {
                message_id: "msg-1".into(),
                sender_inbox_id: "peer-1".into(),
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
                read_by: Vec::new(),
            }],
        });

        assert!(matches!(
            effects.as_slice(),
            [Effect::SendReadReceipt { conversation_id }] if conversation_id == "conv-1"
        ));
    }

    #[test]
    fn history_loaded_skips_read_receipt_when_all_messages_are_from_self() {
        let (mut app, _) = App::new(false);
        app.active_conversation_id = Some("conv-1".into());
        app.status = Some(
            serde_json::from_value(serde_json::json!({
                "daemon_state": "running",
                "connection_state": "connected",
                "inbox_id": "self-1",
                "installation_id": null
            }))
            .expect("build status response"),
        );

        let effects = app.handle_event(crate::event::AppEvent::HistoryLoaded {
            conversation_id: "conv-1".into(),
            items: vec![xmtp_ipc::HistoryItem {
                message_id: "msg-1".into(),
                sender_inbox_id: "self-1".into(),
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
                read_by: Vec::new(),
            }],
        });

        assert!(effects.is_empty());
    }

    #[test]
    fn message_menu_send_read_receipt_skips_self_message() {
        let (mut app, _) = App::new(false);
        app.active_conversation_id = Some("conv-1".into());
        app.status = Some(
            serde_json::from_value(serde_json::json!({
                "daemon_state": "running",
                "connection_state": "connected",
                "inbox_id": "self-1",
                "installation_id": null
            }))
            .expect("build status response"),
        );
        app.modal = Modal::MessageMenu;
        app.messages.push(xmtp_ipc::HistoryItem {
            message_id: "msg-1".into(),
            sender_inbox_id: "self-1".into(),
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
            read_by: Vec::new(),
        });
        app.message_menu_index = app
            .message_menu_actions()
            .iter()
            .position(|action| *action == MessageMenuAction::SendReadReceipt)
            .expect("send read receipt action");

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));

        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::None);
    }

    #[test]
    fn app_starts_with_app_event_subscription_effect() {
        let (_, effects) = App::new(false);
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], Effect::SubscribeAppEvents));
    }

    #[test]
    fn unread_count_increments_for_inactive_conversation_and_clears_on_switch() {
        let (mut app, _) = App::new(false);
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

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        )));

        assert!(matches!(
            effects.as_slice(),
            [Effect::SwitchConversation { conversation_id }] if conversation_id == "conv-2"
        ));
        assert_eq!(app.active_conversation_id.as_deref(), Some("conv-2"));
        assert_eq!(app.unread_counts.get("conv-2"), None);
    }

    #[test]
    fn conversations_loaded_keeps_pending_new_active_conversation_until_it_appears() {
        let (mut app, _) = App::new(false);
        app.conversations = vec![xmtp_ipc::ConversationItem {
            id: "conv-1".into(),
            kind: "dm".into(),
            name: Some("one".into()),
            dm_peer_inbox_id: Some("peer-1".into()),
            last_message_ns: Some(10),
        }];
        app.selected_conversation = 0;
        app.active_conversation_id = Some("conv-2".into());
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "conv-1".into(),
            kind: "dm".into(),
            name: Some("one".into()),
            dm_peer_inbox_id: Some("peer-1".into()),
            last_message_ns: Some(10),
        });

        let effects = app.handle_event(crate::event::AppEvent::ConversationsLoaded(vec![
            xmtp_ipc::ConversationItem {
                id: "conv-1".into(),
                kind: "dm".into(),
                name: Some("one".into()),
                dm_peer_inbox_id: Some("peer-1".into()),
                last_message_ns: Some(10),
            },
        ]));

        assert!(effects.is_empty());
        assert_eq!(app.selected_conversation, 0);
        assert_eq!(app.active_conversation_id.as_deref(), Some("conv-2"));
        assert_eq!(
            app.active_conversation.as_ref().map(|c| c.id.as_str()),
            Some("conv-1")
        );

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
                kind: "dm".into(),
                name: Some("two".into()),
                dm_peer_inbox_id: Some("peer-2".into()),
                last_message_ns: Some(20),
            },
        ]));

        assert!(effects.is_empty());
        assert_eq!(app.selected_conversation, 1);
        assert_eq!(app.active_conversation_id.as_deref(), Some("conv-2"));
        assert_eq!(
            app.active_conversation.as_ref().map(|c| c.id.as_str()),
            Some("conv-2")
        );
    }

    #[test]
    fn sent_action_optimistically_appends_message_to_active_conversation() {
        let (mut app, _) = App::new(false);
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
        app.pending_status = Some("Sending...".into());

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
        assert!(app.pending_status.is_none());
    }

    #[test]
    fn history_event_keeps_selection_when_not_at_end_even_if_messages_panel_is_unfocused() {
        let (mut app, _) = App::new(false);
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
                read_by: Vec::new(),
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
                read_by: Vec::new(),
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
                read_by: Vec::new(),
            },
        });

        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn history_event_auto_scrolls_when_selection_was_already_at_end() {
        let (mut app, _) = App::new(false);
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
                read_by: Vec::new(),
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
                read_by: Vec::new(),
            },
        ];
        app.selected_message = 1;

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
                read_by: Vec::new(),
            },
        });

        assert_eq!(app.selected_message, 2);
    }

    #[test]
    fn history_event_does_not_auto_scroll_when_messages_panel_is_focused() {
        let (mut app, _) = App::new(false);
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
                read_by: Vec::new(),
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
                read_by: Vec::new(),
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
                read_by: Vec::new(),
            },
        });

        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn conversation_updated_event_updates_list_and_active_name() {
        let (mut app, _) = App::new(false);
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
            app.active_conversation
                .as_ref()
                .and_then(|item| item.name.as_deref()),
            Some("new-name")
        );
    }

    #[test]
    fn group_members_updated_event_refreshes_active_group_members() {
        let (mut app, _) = App::new(false);
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
    fn group_members_loaded_updates_self_permission_level() {
        let (mut app, _) = App::new(false);
        app.status = Some(
            serde_json::from_value(serde_json::json!({
                "daemon_state": "running",
                "connection_state": "connected",
                "inbox_id": "self-1",
                "installation_id": null
            }))
            .expect("build status response"),
        );

        let effects = app.handle_event(crate::event::AppEvent::GroupMembersLoaded(vec![
            xmtp_ipc::GroupMemberItem {
                inbox_id: "self-1".into(),
                permission_level: "admin".into(),
                consent_state: "unknown".into(),
                account_identifiers: Vec::new(),
                installation_count: 1,
            },
            xmtp_ipc::GroupMemberItem {
                inbox_id: "peer-1".into(),
                permission_level: "member".into(),
                consent_state: "unknown".into(),
                account_identifiers: Vec::new(),
                installation_count: 1,
            },
        ]));

        assert!(effects.is_empty());
        assert_eq!(
            app.group_management.self_permission_level.as_deref(),
            Some("admin")
        );
    }

    #[test]
    fn add_members_precheck_blocks_member_without_permission() {
        let (mut app, _) = App::new(false);
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "grp-1".into(),
            kind: "group".into(),
            name: Some("team".into()),
            dm_peer_inbox_id: None,
            last_message_ns: None,
        });
        app.active_conversation_id = Some("grp-1".into());
        app.modal = Modal::GroupManagement;
        app.group_management.menu_index = 2;
        app.group_management.permissions = Some(xmtp_ipc::GroupPermissionsResponse {
            preset: "custom".into(),
            add_member: "admin_only".into(),
            remove_member: "everyone".into(),
            add_admin: "super_admin_only".into(),
            remove_admin: "super_admin_only".into(),
            update_group_name: "admin_only".into(),
            update_group_description: "admin_only".into(),
            update_group_image: "admin_only".into(),
            update_app_data: "admin_only".into(),
        });
        app.group_management.self_permission_level = Some("member".into());

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )));

        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::None);
        assert_eq!(
            app.last_error.as_deref(),
            Some("You don't have permission to perform this action")
        );
    }

    #[test]
    fn esc_from_input_returns_to_conversations_without_quitting() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        )));
        assert!(effects.is_empty());
        assert_eq!(app.focus, Focus::Conversations);
        assert!(!app.should_quit);
        assert!(!app.exit_armed);
    }

    #[test]
    fn esc_in_input_clears_reply_state_before_leaving_input() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Input;
        app.reply_to_message_id = Some("msg-1".into());

        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        )));

        assert!(effects.is_empty());
        assert_eq!(app.focus, Focus::Input);
        assert!(app.reply_to_message_id.is_none());
        assert!(!app.should_quit);
        assert!(!app.exit_armed);
    }

    #[test]
    fn esc_twice_in_conversations_quits() {
        let (mut app, _) = App::new(false);
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
        let (mut app, _) = App::new(false);
        app.modal = Modal::CreateDm;
        let effects = app.handle_event(crate::event::AppEvent::Terminal(Event::Key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        )));
        assert!(effects.is_empty());
        assert_eq!(app.modal, Modal::None);
        assert!(!app.exit_armed);
        assert!(!app.should_quit);
    }

    #[test]
    fn switching_conversation_preserves_draft_and_clears_reply_state() {
        let (mut app, _) = App::new(false);
        app.active_conversation_id = Some("conv-1".into());
        app.active_conversation = Some(xmtp_ipc::ConversationItem {
            id: "conv-1".into(),
            kind: "dm".into(),
            name: Some("first".into()),
            dm_peer_inbox_id: Some("peer-a".into()),
            last_message_ns: Some(10),
        });
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
        assert_eq!(
            app.drafts.get("conv-1").map(String::as_str),
            Some("draft message")
        );

        app.input = "other draft".into();
        let effects = app.activate_conversation(xmtp_ipc::ConversationItem {
            id: "conv-1".into(),
            kind: "dm".into(),
            name: Some("first".into()),
            dm_peer_inbox_id: Some("peer-a".into()),
            last_message_ns: Some(10),
        });

        assert!(matches!(
            effects.as_slice(),
            [Effect::SwitchConversation { conversation_id }] if conversation_id == "conv-1"
        ));
        assert_eq!(app.input, "draft message");
        assert!(app.reply_to_message_id.is_none());
    }

    #[test]
    fn switching_to_different_conversation_resets_selected_message_to_end() {
        let (mut app, _) = App::new(false);
        app.active_conversation_id = Some("conv-1".into());
        app.active_conversation = Some(ConversationItem {
            id: "conv-1".into(),
            kind: "dm".into(),
            name: Some("first".into()),
            dm_peer_inbox_id: Some("peer-a".into()),
            last_message_ns: Some(10),
        });
        app.messages = vec![
            sample_history_item("msg-1", "one"),
            sample_history_item("msg-2", "two"),
            sample_history_item("msg-3", "three"),
        ];
        app.selected_message = 0;

        let effects = app.activate_conversation(ConversationItem {
            id: "conv-2".into(),
            kind: "dm".into(),
            name: Some("second".into()),
            dm_peer_inbox_id: Some("peer-b".into()),
            last_message_ns: Some(20),
        });

        assert!(matches!(
            effects.as_slice(),
            [Effect::SwitchConversation { conversation_id }] if conversation_id == "conv-2"
        ));
        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn leaving_messages_focus_resets_selected_message_to_end() {
        let (mut app, _) = App::new(false);
        app.focus = Focus::Messages;
        app.messages = vec![
            sample_history_item("msg-1", "one"),
            sample_history_item("msg-2", "two"),
            sample_history_item("msg-3", "three"),
        ];
        app.selected_message = 0;

        let _ = app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        assert_eq!(app.focus, Focus::Input);
        assert_eq!(app.selected_message, 2);
    }

    #[test]
    fn reactivating_same_conversation_keeps_selected_message_position() {
        let (mut app, _) = App::new(false);
        app.active_conversation_id = Some("conv-1".into());
        app.active_conversation = Some(ConversationItem {
            id: "conv-1".into(),
            kind: "dm".into(),
            name: Some("first".into()),
            dm_peer_inbox_id: Some("peer-a".into()),
            last_message_ns: Some(10),
        });
        app.messages = vec![
            sample_history_item("msg-1", "one"),
            sample_history_item("msg-2", "two"),
            sample_history_item("msg-3", "three"),
        ];
        app.selected_message = 1;

        let effects = app.activate_conversation(ConversationItem {
            id: "conv-1".into(),
            kind: "dm".into(),
            name: Some("first updated".into()),
            dm_peer_inbox_id: Some("peer-a".into()),
            last_message_ns: Some(11),
        });

        assert!(effects.is_empty());
        assert_eq!(app.selected_message, 1);
        assert_eq!(
            app.active_conversation
                .as_ref()
                .and_then(|c| c.name.as_deref()),
            Some("first updated")
        );
    }
}
