use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::{Frame, prelude::Alignment};

use crate::app::{
    App, Focus, GroupDialogField, GroupManagementAction, MessageMenuAction, Modal,
    reaction_choices,
};
use crate::format::{format_clock, format_day_tag, short_display_id};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(32), Constraint::Min(20)])
        .split(areas[0]);

    render_conversations(frame, app, main[0]);
    render_messages(frame, app, main[1]);
    render_input(frame, app, areas[1]);
    render_status(frame, app, areas[2]);

    match app.modal {
        Modal::None => {}
        Modal::MessageMenu => render_message_menu(frame, app),
        Modal::ReactionPicker => render_reaction_picker(frame, app),
        Modal::CreateDm => render_create_dm(frame, app),
        Modal::CreateGroup => render_create_group(frame, app),
        Modal::GroupManagement => render_group_management(frame, app),
        Modal::GroupInfo => render_group_info(frame, app),
        Modal::GroupAddMembers => render_group_add_members(frame, app),
        Modal::GroupRemoveMembers => render_group_remove_members(frame, app),
        Modal::GroupRename => render_group_rename(frame, app),
        Modal::GroupLeaveConfirm => render_group_leave_confirm(frame, app),
    }
}

fn render_conversations(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mut state = ListState::default().with_selected(if app.conversations.is_empty() {
        None
    } else {
        Some(app.selected_conversation)
    });
    let items: Vec<ListItem<'_>> = app
        .conversations
        .iter()
        .map(|conversation| {
            let active = app.active_conversation_id.as_deref() == Some(conversation.id.as_str());
            let marker = if active { "●" } else { " " };
            let label = conversation
                .name
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| short_display_id(&conversation.id));
            let kind = if conversation.kind == "group" { "grp" } else { "dm" };
            let text = format!("{} {} ({})", marker, truncate(&label, 22), kind);
            ListItem::new(Line::from(Span::styled(text, Style::default())))
        })
        .collect();
    let block = titled_block("Conversations", app.focus == Focus::Conversations);
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().reversed().add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_messages(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if app.messages.is_empty() && app.active_conversation.is_none() {
        let message = app
            .last_error
            .clone()
            .unwrap_or_else(|| "No conversations loaded.".to_owned());
        let paragraph = Paragraph::new(message)
            .block(titled_block("Messages", app.focus == Focus::Messages))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
        return;
    }

    if app.active_history_loading && app.messages.is_empty() {
        let title = app
            .active_conversation
            .as_ref()
            .and_then(|conversation| conversation.name.clone())
            .unwrap_or_else(|| "Messages".to_owned());
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "loading...",
            Style::default().dark_gray(),
        )))
        .block(titled_block(&title, app.focus == Focus::Messages))
        .alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut state = ListState::default().with_selected(if app.messages.is_empty() {
        None
    } else {
        Some(app.selected_message)
    });

    let mut last_day: Option<String> = None;
    let items: Vec<ListItem<'_>> = app
        .messages
        .iter()
        .map(|item| {
            let day_tag = format_day_tag(item.sent_at_ns);
            let mut lines = Vec::new();
            if last_day.as_deref() != Some(day_tag.as_str()) {
                last_day = Some(day_tag.clone());
                lines.push(Line::from(Span::styled(
                    format!("----- {} ----", day_tag),
                    Style::default().dark_gray(),
                )));
            }

            let content = item.content.replace('\n', " ");
            let message_line = format!(
                "{} [{}] {}",
                format_clock(item.sent_at_ns),
                short_display_id(&item.sender_inbox_id),
                content,
            );
            lines.push(Line::from(Span::styled(
                message_line,
                Style::default().fg(app.color_for_message(item)),
            )));
            if let Some(reactions_line) = format_reactions_line(item) {
                lines.push(Line::from(Span::styled(
                    format!("  reactions: {reactions_line}"),
                    Style::default().dark_gray(),
                )));
            }
            ListItem::new(lines)
        })
        .collect();

    let title = app
        .active_conversation
        .as_ref()
        .and_then(|conversation| conversation.name.clone())
        .unwrap_or_else(|| "Messages".to_owned());
    let list = List::new(items)
        .block(titled_block(&title, app.focus == Focus::Messages))
        .highlight_style(Style::default().reversed().add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_input(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if let Some(reply_to) = &app.reply_to_message_id {
        lines.push(Line::from(format!("reply -> {}", short_display_id(reply_to))));
    }
    if app.input.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type message",
            Style::default().dark_gray(),
        )));
    } else {
        lines.extend(app.input.lines().map(Line::from));
    }
    let paragraph = Paragraph::new(lines)
        .block(titled_block("Input", app.focus == Focus::Input))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let me = app
        .self_inbox_id()
        .map(short_display_id)
        .unwrap_or_else(|| "-".to_owned());
    let online = app
        .status
        .as_ref()
        .map(|status| status.connection_state.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let daemon = app
        .status
        .as_ref()
        .map(|status| status.daemon_state.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    let selected_message_id = app
        .messages
        .get(app.selected_message)
        .map(|item| short_display_id(&item.message_id))
        .unwrap_or_else(|| "-".to_owned());
    let current_name = app
        .active_conversation
        .as_ref()
        .and_then(|conversation| conversation.name.clone())
        .unwrap_or_else(|| "-".to_owned());
    let current_id = app
        .active_conversation_id
        .as_deref()
        .map(short_display_id)
        .unwrap_or_else(|| "-".to_owned());
    let current_kind = app
        .active_conversation
        .as_ref()
        .map(|conversation| conversation.kind.as_str())
        .unwrap_or("-");
    let current_detail = format!("current {} | {} | {}", current_kind, current_name, current_id);
    let mut runtime_detail = format!(
        "me {} | {} | daemon {} | msg {}",
        me, online, daemon, selected_message_id
    );
    if let Some(error) = &app.last_error {
        runtime_detail.push_str(" | ");
        runtime_detail.push_str(&truncate(error, 48));
    }
    let lines = vec![
        Line::from(current_detail),
        Line::from(runtime_detail),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Left), area);
}

fn render_message_menu(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(36, 26, frame.area());
    frame.render_widget(Clear, area);
    let selected_message_id = app
        .messages
        .get(app.selected_message)
        .map(|item| short_display_id(&item.message_id))
        .unwrap_or_else(|| "-".to_owned());
    let items: Vec<ListItem<'_>> = MessageMenuAction::all()
        .into_iter()
        .map(|action| ListItem::new(action.label()))
        .collect();
    let mut state = ListState::default().with_selected(Some(app.message_menu_index));
    let list = List::new(items)
        .block(
            Block::default()
                .title(format!("Message {}", selected_message_id))
                .borders(Borders::ALL),
        )
        .highlight_style(Style::default().reversed());
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_reaction_picker(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(28, 24, frame.area());
    frame.render_widget(Clear, area);
    let items: Vec<ListItem<'_>> = reaction_choices()
        .into_iter()
        .map(ListItem::new)
        .collect();
    let mut state = ListState::default().with_selected(Some(app.reaction_picker_index));
    let list = List::new(items)
        .block(Block::default().title("Reaction").borders(Borders::ALL))
        .highlight_style(Style::default().reversed());
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_create_dm(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(60, 20, frame.area());
    frame.render_widget(Clear, area);
    let text = vec![
        Line::from("Create direct-message"),
        Line::from("recipient inbox/address:"),
        Line::from(app.dm_dialog.recipient.clone()),
    ];
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("New DM").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_create_group(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(72, 28, frame.area());
    frame.render_widget(Clear, area);
    let name_marker = if app.group_dialog.field == Some(GroupDialogField::Name) { ">" } else { " " };
    let members_marker = if app.group_dialog.field == Some(GroupDialogField::Members) { ">" } else { " " };
    let text = vec![
        Line::from("Create group"),
        Line::from(format!("{} name: {}", name_marker, app.group_dialog.name)),
        Line::from(format!(
            "{} members: {}",
            members_marker, app.group_dialog.members
        )),
        Line::from("members can be separated by comma or space"),
    ];
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("New Group").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_group_management(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(44, 34, frame.area());
    frame.render_widget(Clear, area);
    let items: Vec<ListItem<'_>> = GroupManagementAction::all()
        .into_iter()
        .map(|action| ListItem::new(action.label()))
        .collect();
    let mut state = ListState::default().with_selected(Some(app.group_management.menu_index));
    let title = app
        .active_conversation
        .as_ref()
        .and_then(|conversation| conversation.name.clone())
        .unwrap_or_else(|| "Group".to_owned());
    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().reversed());
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_group_info(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(64, 36, frame.area());
    frame.render_widget(Clear, area);
    let text = if let Some(info) = &app.group_management.info {
        vec![
            Line::from(format!(
                "name: {}",
                info.name.clone().unwrap_or_else(|| "-".to_owned())
            )),
            Line::from(format!("members: {}", info.member_count)),
            Line::from(format!(
                "creator: {}",
                if info.creator_inbox_id.is_empty() {
                    "-".to_owned()
                } else {
                    short_display_id(&info.creator_inbox_id)
                }
            )),
            Line::from(format!("type: {}", info.conversation_type)),
        ]
    } else {
        vec![Line::from(Span::styled(
            "loading...",
            Style::default().dark_gray(),
        ))]
    };
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("Group Info").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_group_add_members(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(70, 24, frame.area());
    frame.render_widget(Clear, area);
    let text = vec![
        Line::from("Add members"),
        Line::from("inbox_id list:"),
        Line::from(app.group_management.add_members_input.clone()),
        Line::from("members can be separated by comma or space"),
    ];
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("Add Members").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_group_remove_members(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(70, 42, frame.area());
    frame.render_widget(Clear, area);
    if app.group_management.members.is_empty() {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "loading...",
            Style::default().dark_gray(),
        )))
        .block(Block::default().title("Remove Members").borders(Borders::ALL));
        frame.render_widget(paragraph, area);
        return;
    }
    let items: Vec<ListItem<'_>> = app
        .group_management
        .members
        .iter()
        .map(|member| {
            ListItem::new(format!(
                "{} [{}]",
                short_display_id(&member.inbox_id),
                member.permission_level
            ))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.group_management.selected_member));
    let list = List::new(items)
        .block(Block::default().title("Remove Members").borders(Borders::ALL))
        .highlight_style(Style::default().reversed());
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_group_rename(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(64, 22, frame.area());
    frame.render_widget(Clear, area);
    let current_name = app
        .active_conversation
        .as_ref()
        .and_then(|conversation| conversation.name.clone())
        .unwrap_or_else(|| "-".to_owned());
    let text = vec![
        Line::from(format!("New name (current: {current_name}):")),
        Line::from(app.group_management.rename_input.clone()),
    ];
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("Rename").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_group_leave_confirm(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(54, 18, frame.area());
    frame.render_widget(Clear, area);
    let name = app
        .active_conversation
        .as_ref()
        .and_then(|conversation| conversation.name.clone())
        .unwrap_or_else(|| "group".to_owned());
    let text = vec![
        Line::from(format!("Leave {name}?")),
        Line::from("Leave group is not supported in this version."),
        Line::from("press y to acknowledge"),
        Line::from("press Esc to cancel"),
    ];
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("Leave Group").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn truncate(value: &str, max: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max {
        return value.to_owned();
    }
    chars[..max].iter().collect()
}

fn format_reactions_line(item: &xmtp_ipc::HistoryItem) -> Option<String> {
    if item.attached_reactions.is_empty() {
        return None;
    }

    let mut order = Vec::<String>::new();
    let mut counts = std::collections::BTreeMap::<String, i32>::new();
    for reaction in &item.attached_reactions {
        if !order.iter().any(|emoji| emoji == &reaction.emoji) {
            order.push(reaction.emoji.clone());
        }
        let entry = counts.entry(reaction.emoji.clone()).or_insert(0);
        match reaction.action.as_str() {
            "removed" => *entry -= 1,
            _ => *entry += 1,
        }
    }

    let parts: Vec<String> = order
        .into_iter()
        .filter_map(|emoji| {
            let count = counts.get(&emoji).copied().unwrap_or_default();
            if count > 0 {
                Some(format!("{emoji} x{count}"))
            } else {
                None
            }
        })
        .collect();

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn titled_block(title: &str, focused: bool) -> Block<'_> {
    let title_style = if focused {
        Style::default().yellow().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let border_style = if focused {
        Style::default().yellow().add_modifier(Modifier::BOLD)
    } else {
        Style::default().dark_gray()
    };
    Block::default()
        .title(Span::styled(title.to_owned(), title_style))
        .borders(Borders::ALL)
        .border_style(border_style)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
