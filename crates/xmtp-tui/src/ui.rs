use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, prelude::Alignment};
use textwrap::wrap;

use crate::app::{App, Focus, GroupDialogField, GroupManagementAction, Modal, reaction_choices};
use crate::format::{format_clock, format_day_tag, short_display_id};
use crate::markdown::render_markdown;
use xmtp_ipc::GroupPermissionsResponse;

enum MessageRowKind {
    DateSeparator,
    ReplyContext,
    Reactions,
    Message(usize),
}

struct MessageRow<'a> {
    kind: MessageRowKind,
    item: ListItem<'a>,
}

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let input_height = input_panel_height(app, frame.area().width);
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(input_height),
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
        Modal::Help => render_help(frame),
        Modal::MessageMenu => render_message_menu(frame, app),
        Modal::MessageDetail => render_message_detail(frame, app),
        Modal::ReactionPicker => render_reaction_picker(frame, app),
        Modal::CreateDm => render_create_dm(frame, app),
        Modal::CreateGroup => render_create_group(frame, app),
        Modal::GroupManagement => render_group_management(frame, app),
        Modal::GroupInfo => render_group_info(frame, app),
        Modal::GroupMembers => render_group_members(frame, app),
        Modal::GroupPermissions => render_group_permissions(frame, app),
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
            let kind = if conversation.kind == "group" {
                "grp"
            } else {
                "dm"
            };
            let label = if conversation.kind == "dm" {
                conversation
                    .dm_peer_inbox_id
                    .as_deref()
                    .map(short_display_id)
                    .unwrap_or_else(|| short_display_id(&conversation.id))
            } else {
                conversation
                    .name
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| short_display_id(&conversation.id))
            };
            let unread = app
                .unread_counts
                .get(&conversation.id)
                .copied()
                .unwrap_or_default();
            let unread_suffix = if unread > 0 {
                format!(" [{}]", unread)
            } else {
                String::new()
            };
            let text = format!(
                "{} {} ({}){}",
                marker,
                truncate(&label, 18),
                kind,
                unread_suffix
            );
            let style = if unread > 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(text, style)))
        })
        .collect();
    let block = titled_block("Conversations", app.focus == Focus::Conversations);
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_messages(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let title = message_panel_title(app);
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
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "Loading…",
            Style::default().dark_gray(),
        )))
        .block(titled_block(&title, app.focus == Focus::Messages))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    if app.messages.is_empty() {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "No messages yet",
            Style::default().dark_gray(),
        )))
        .block(titled_block(&title, app.focus == Focus::Messages))
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    let rows = build_message_rows(app, area.width.saturating_sub(2));
    let selected_row = rows.iter().position(|row| match row.kind {
        MessageRowKind::Message(index) => index == app.selected_message,
        MessageRowKind::DateSeparator
        | MessageRowKind::ReplyContext
        | MessageRowKind::Reactions => false,
    });
    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        items.push(row.item.clone());
    }

    let mut state = ListState::default();
    if let Some(selected_row_idx) = selected_row {
        let end_row_idx = trailing_row_end_index(&rows, selected_row_idx);
        let visible_height = area.height.saturating_sub(2) as usize;
        *state.offset_mut() = list_offset_for_visible_window(&items, end_row_idx, visible_height);
    }

    let list = List::new(items).block(titled_block(&title, app.focus == Focus::Messages));
    frame.render_stateful_widget(list, area, &mut state);
}

fn build_message_rows<'a>(app: &'a App, width: u16) -> Vec<MessageRow<'a>> {
    const THRESHOLD: usize = 4;

    let mut rows = Vec::new();
    let mut last_day: Option<String> = None;
    let wrap_width = width.max(1) as usize;
    let selected_style = Style::default()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let unselected_style = Style::default().bg(Color::Reset);

    for (index, item) in app.messages.iter().enumerate() {
        let day_tag = format_day_tag(item.sent_at_ns);
        if last_day.as_deref() != Some(day_tag.as_str()) {
            last_day = Some(day_tag.clone());
            rows.push(MessageRow {
                kind: MessageRowKind::DateSeparator,
                item: ListItem::new(Line::from(Span::styled(
                    format!("----- {} ----", day_tag),
                    Style::default().dark_gray().bg(Color::Reset),
                )))
                .style(Style::default().bg(Color::Reset)),
            });
        }

        if let Some(reply_target_id) = item.reply_target_message_id.as_deref() {
            let reply_line = if let Some(target) = app
                .messages
                .iter()
                .find(|candidate| candidate.message_id == reply_target_id)
            {
                let preview = truncate(&target.content.replace('\n', " "), 20);
                format!(
                    "  ↩ [{}]: {}",
                    short_display_id(&target.sender_inbox_id),
                    preview
                )
            } else {
                format!("  ↩ [{}]", short_display_id(reply_target_id))
            };
            rows.push(MessageRow {
                kind: MessageRowKind::ReplyContext,
                item: ListItem::new(Line::from(Span::styled(
                    reply_line,
                    Style::default().fg(Color::Gray).bg(Color::Reset),
                )))
                .style(Style::default().bg(Color::Reset)),
            });
        }

        let message_style = if index == app.selected_message {
            selected_style
        } else {
            unselected_style
        };
        let content = item.content.replace('\n', " ");
        let sender_display = if app.self_inbox_id() == Some(item.sender_inbox_id.as_str()) {
            "You".to_owned()
        } else {
            short_display_id(&item.sender_inbox_id)
        };
        let header = format!("{} [{}]", format_clock(item.sent_at_ns), sender_display);
        let mut header_spans = vec![Span::styled(
            header,
            Style::default()
                .fg(app.color_for_message(item))
                .bg(Color::Reset),
        )];
        if app.self_inbox_id() == Some(item.sender_inbox_id.as_str()) && !item.read_by.is_empty() {
            header_spans.push(Span::styled(
                " ✓",
                Style::default().fg(Color::DarkGray).bg(Color::Reset),
            ));
        }
        rows.push(MessageRow {
            kind: MessageRowKind::Message(index),
            item: ListItem::new(Line::from(header_spans)).style(message_style),
        });

        let mut content_lines = if item.content_kind == "markdown" {
            let rendered =
                app.cached_markdown_lines(&item.message_id, &item.content, wrap_width.max(1));
            if rendered.iter().all(|line| {
                line.spans
                    .iter()
                    .all(|span| span.content.as_ref().trim().is_empty())
            }) {
                wrap_text_lines(&content, wrap_width)
                    .into_iter()
                    .map(|segment| {
                        Line::from(Span::styled(
                            segment,
                            Style::default()
                                .fg(app.color_for_message(item))
                                .bg(Color::Reset),
                        ))
                    })
                    .collect::<Vec<_>>()
            } else {
                rendered
                    .into_iter()
                    .map(|line| line.style(Style::default().bg(Color::Reset)))
                    .collect::<Vec<_>>()
            }
        } else {
            wrap_text_lines(&content, wrap_width)
                .into_iter()
                .map(|segment| {
                    Line::from(Span::styled(
                        segment,
                        Style::default()
                            .fg(app.color_for_message(item))
                            .bg(Color::Reset),
                    ))
                })
                .collect::<Vec<_>>()
        };

        if content_lines.is_empty() {
            content_lines.push(Line::from(Span::styled(
                content.clone(),
                Style::default()
                    .fg(app.color_for_message(item))
                    .bg(Color::Reset),
            )));
        }

        let is_collapsed = content_lines.len() > THRESHOLD;
        let preview_lines = if is_collapsed {
            content_lines
                .into_iter()
                .take(THRESHOLD)
                .collect::<Vec<_>>()
        } else {
            content_lines
        };
        rows.extend(preview_lines.into_iter().map(|line| MessageRow {
            kind: MessageRowKind::Message(index),
            item: ListItem::new(line).style(message_style),
        }));
        if is_collapsed {
            rows.push(MessageRow {
                kind: MessageRowKind::Message(index),
                item: ListItem::new(Line::from(Span::styled(
                    " ... (Enter: view full)",
                    Style::default().fg(Color::Yellow),
                )))
                .style(message_style),
            });
        }

        if let Some(reactions_line) = format_reactions_line(item) {
            rows.push(MessageRow {
                kind: MessageRowKind::Reactions,
                item: ListItem::new(Line::from(Span::styled(
                    format!("  reactions: {reactions_line}"),
                    Style::default().fg(Color::Gray).bg(Color::Reset),
                )))
                .style(Style::default().bg(Color::Reset)),
            });
        }
    }

    rows
}

fn message_panel_title(app: &App) -> String {
    conversation_display_name(app).unwrap_or_else(|| "Messages".to_owned())
}

fn conversation_display_name(app: &App) -> Option<String> {
    match app.active_conversation.as_ref() {
        Some(conversation) if conversation.kind == "dm" => conversation
            .dm_peer_inbox_id
            .as_deref()
            .map(short_display_id)
            .or_else(|| conversation.name.clone())
            .or(Some("DM".to_owned())),
        Some(conversation) if conversation.kind == "group" => {
            conversation.name.clone().or(Some("Group".to_owned()))
        }
        Some(conversation) => conversation.name.clone(),
        None => None,
    }
}

fn render_input(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if let Some(reply_to) = &app.reply_to_message_id {
        lines.push(Line::from(format!(
            "reply -> {}",
            short_display_id(reply_to)
        )));
    }
    lines.extend(render_input_lines(app, app.focus == Focus::Input));
    let paragraph = Paragraph::new(lines)
        .block(titled_block("Input", app.focus == Focus::Input))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn input_panel_height(app: &App, total_width: u16) -> u16 {
    let usable_width = total_width.saturating_sub(2).max(1) as usize;
    let input_lines = input_visual_line_count(app, usable_width).clamp(1, 5) as u16;
    let reply_lines = u16::from(app.reply_to_message_id.is_some());
    input_lines + reply_lines + 2
}

fn input_visual_line_count(app: &App, usable_width: usize) -> usize {
    if app.input.is_empty() {
        return 1;
    }

    app.input
        .split('\n')
        .map(|segment| {
            let wrapped = wrap(segment, usable_width.max(1));
            wrapped.len().max(1)
        })
        .sum::<usize>()
        .max(1)
}

fn render_input_lines(app: &App, focused: bool) -> Vec<Line<'static>> {
    if app.input.is_empty() {
        let mut spans = Vec::new();
        if focused {
            spans.push(Span::styled(
                " ",
                Style::default().add_modifier(Modifier::REVERSED),
            ));
        }
        spans.push(Span::styled("Type message", Style::default().dark_gray()));
        return vec![Line::from(spans)];
    }

    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let chars: Vec<char> = app.input.chars().collect();

    for (index, ch) in chars.iter().enumerate() {
        if index == app.cursor && *ch == '\n' {
            if focused {
                spans.push(Span::styled(
                    " ",
                    Style::default().add_modifier(Modifier::REVERSED),
                ));
            }
            lines.push(Line::from(std::mem::take(&mut spans)));
            continue;
        }

        if *ch == '\n' {
            lines.push(Line::from(std::mem::take(&mut spans)));
            continue;
        }

        let span = if focused && index == app.cursor {
            Span::styled(
                ch.to_string(),
                Style::default().add_modifier(Modifier::REVERSED),
            )
        } else {
            Span::raw(ch.to_string())
        };
        spans.push(span);
    }

    if focused && app.cursor == chars.len() {
        spans.push(Span::styled(
            " ",
            Style::default().add_modifier(Modifier::REVERSED),
        ));
    }

    lines.push(Line::from(spans));
    lines
}

fn wrap_text_lines(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        let wrapped = wrap(raw_line, width.max(1));
        if wrapped.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrapped.into_iter().map(|line| line.into_owned()));
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn trailing_row_end_index(rows: &[MessageRow<'_>], selected_row_idx: usize) -> usize {
    let mut end = selected_row_idx;
    let selected_message_index = match rows.get(selected_row_idx) {
        Some(MessageRow {
            kind: MessageRowKind::Message(index),
            ..
        }) => Some(*index),
        Some(MessageRow {
            kind: MessageRowKind::DateSeparator
                | MessageRowKind::ReplyContext
                | MessageRowKind::Reactions,
            ..
        })
        | None => None,
    };
    for (index, row) in rows.iter().enumerate().skip(selected_row_idx + 1) {
        match row.kind {
            MessageRowKind::Message(message_index)
                if Some(message_index) != selected_message_index =>
            {
                break;
            }
            MessageRowKind::Message(_) => {
                end = index;
            }
            MessageRowKind::DateSeparator
            | MessageRowKind::ReplyContext
            | MessageRowKind::Reactions => {
                end = index;
            }
        }
    }
    end
}

fn list_offset_for_visible_window(
    items: &[ListItem<'_>],
    end_row_idx: usize,
    visible_height: usize,
) -> usize {
    if items.is_empty() || visible_height == 0 {
        return 0;
    }

    let mut offset = end_row_idx;
    let mut used_height = 0usize;
    loop {
        let item_height = items[offset].height();
        if used_height + item_height > visible_height {
            return (offset + 1).min(end_row_idx);
        }
        used_height += item_height;
        if offset == 0 {
            return 0;
        }
        offset -= 1;
    }
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
    let xmtp_env = app.xmtp_env.as_deref().unwrap_or("unknown");
    let selected_message_id = app
        .messages
        .get(app.selected_message)
        .map(|item| short_display_id(&item.message_id))
        .unwrap_or_else(|| "-".to_owned());
    let current_name = conversation_display_name(app).unwrap_or_else(|| "-".to_owned());
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
    let current_detail = format!(
        "current {} | {} | {} | msg {}",
        current_kind, current_name, current_id, selected_message_id
    );
    let connection_style = if online == "connected" {
        Style::default().fg(Color::Green)
    } else if online == "disconnected" || online.contains("error") {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let daemon_style = if daemon == "running" {
        Style::default().fg(Color::Green)
    } else if daemon == "stopped" {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let mut runtime_spans = vec![
        Span::raw(format!("me {} | {} | ", me, xmtp_env)),
        Span::styled(online.clone(), connection_style),
        Span::raw(" | daemon "),
        Span::styled(daemon.clone(), daemon_style),
    ];
    if let Some(error) = &app.last_error {
        runtime_spans.push(Span::raw(" | "));
        runtime_spans.push(Span::raw(truncate(error, 48)));
    } else if let Some(status) = &app.pending_status {
        runtime_spans.push(Span::raw(" | "));
        runtime_spans.push(Span::styled(
            truncate(status, 48),
            Style::default().dark_gray(),
        ));
    }
    let lines = vec![Line::from(current_detail), Line::from(runtime_spans)];
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
    let items: Vec<ListItem<'_>> = app
        .message_menu_actions()
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

fn render_message_detail(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(88, 84, frame.area());
    frame.render_widget(Clear, area);
    let wrap_width = area.width.saturating_sub(4).max(1) as usize;
    let visible_height = area.height.saturating_sub(2) as usize;
    app.last_detail_wrap_width.set(wrap_width);
    app.last_detail_visible_height.set(visible_height);
    let Some(message) = app.detail_message() else {
        let paragraph = Paragraph::new("Message not found")
            .block(
                Block::default()
                    .title("Message Detail")
                    .borders(Borders::ALL),
            )
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    };

    let mut lines = vec![
        Line::from(format!(
            "{} [{}]",
            format_clock(message.sent_at_ns),
            if app.self_inbox_id() == Some(message.sender_inbox_id.as_str()) {
                "You".to_owned()
            } else {
                short_display_id(&message.sender_inbox_id)
            }
        )),
        Line::from(""),
    ];
    if message.content_kind == "markdown" {
        lines.extend(render_markdown(&message.content, wrap_width));
    } else {
        lines.extend(
            wrap_text_lines(&message.content, wrap_width)
                .into_iter()
                .map(Line::from),
        );
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑↓ 滚动  Esc 关闭",
        Style::default().dark_gray(),
    )));

    let max_scroll = lines.len().saturating_sub(visible_height);
    let scroll = app.detail_scroll.min(max_scroll) as u16;

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Message Detail")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}

fn render_help(frame: &mut Frame<'_>) {
    let area = centered_rect(64, 44, frame.area());
    frame.render_widget(Clear, area);
    let text = vec![
        Line::from("Keyboard Help"),
        Line::from(""),
        Line::from("Tab            switch panel"),
        Line::from("Up/Down        navigate"),
        Line::from("Enter          select / send"),
        Line::from("Enter on group group management"),
        Line::from("Left/Right     move cursor"),
        Line::from("Ctrl/Alt+←/→   jump word"),
        Line::from("Bksp/Del       delete char"),
        Line::from("Ctrl+A/E       line start/end"),
        Line::from("Ctrl+K         delete to end"),
        Line::from("Ctrl+W         delete word"),
        Line::from("Ctrl+U         delete to start"),
        Line::from("Alt+Enter      newline"),
        Line::from("Ctrl+N         create direct-message"),
        Line::from("g              create group"),
        Line::from("r              quick reply"),
        Line::from("q / Esc Esc    quit"),
        Line::from("?              show help"),
        Line::from(""),
        Line::from("Esc closes this help"),
    ];
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_reaction_picker(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(28, 24, frame.area());
    frame.render_widget(Clear, area);
    let items: Vec<ListItem<'_>> = reaction_choices().into_iter().map(ListItem::new).collect();
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
    let name_marker = if app.group_dialog.field == Some(GroupDialogField::Name) {
        ">"
    } else {
        " "
    };
    let members_marker = if app.group_dialog.field == Some(GroupDialogField::Members) {
        ">"
    } else {
        " "
    };
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
        .enumerate()
        .map(|(index, action)| {
            let allowed = app.can_manage_group_members(action);
            let label = if matches!(
                action,
                GroupManagementAction::AddMembers | GroupManagementAction::RemoveMembers
            ) && !allowed
            {
                format!("{}. {} (no permission)", index + 1, action.label())
            } else {
                format!("{}. {}", index + 1, action.label())
            };
            let style = if matches!(
                action,
                GroupManagementAction::AddMembers | GroupManagementAction::RemoveMembers
            ) && !allowed
            {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
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

fn render_group_permissions(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(68, 18, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title("Group Permissions")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(inner);

    if app.group_management.permissions_loading {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "loading...",
                Style::default().dark_gray(),
            )))
            .wrap(Wrap { trim: false }),
            chunks[0],
        );
    } else if let Some(info) = &app.group_management.permissions {
        let mut rows = Vec::new();
        rows.push(ListItem::new(render_group_permission_row(
            "Preset:",
            &info.preset,
            false,
        )));
        for (index, (label, value)) in editable_group_permission_rows(info).into_iter().enumerate()
        {
            let style = if index == app.group_management.permissions_cursor {
                Style::default().reversed()
            } else {
                Style::default()
            };
            rows.push(ListItem::new(render_group_permission_row(label, value, true)).style(style));
        }
        let mut state =
            ListState::default().with_selected(Some(app.group_management.permissions_cursor + 1));
        let list = List::new(rows).highlight_style(Style::default().reversed());
        frame.render_stateful_widget(list, chunks[0], &mut state);
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no permission data",
                Style::default().dark_gray(),
            )))
            .wrap(Wrap { trim: false }),
            chunks[0],
        );
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓ 移动  ← → 切换  Enter 保存  Esc 取消",
            Style::default().dark_gray(),
        ))),
        chunks[1],
    );
}

fn editable_group_permission_rows(info: &GroupPermissionsResponse) -> [(&'static str, &str); 8] {
    [
        ("Add members:", &info.add_member),
        ("Remove members:", &info.remove_member),
        ("Add admins:", &info.add_admin),
        ("Remove admins:", &info.remove_admin),
        ("Update name:", &info.update_group_name),
        ("Update description:", &info.update_group_description),
        ("Update image:", &info.update_group_image),
        ("Update app data:", &info.update_app_data),
    ]
}

fn render_group_permission_row(label: &str, value: &str, editable: bool) -> Line<'static> {
    let prefix = if editable { "  " } else { "" };
    Line::from(Span::raw(format!("{prefix}{:<20} {}", label, value)))
}

fn render_group_info(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(64, 22, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default().title("Group Info").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(1)])
        .split(inner);

    let info_lines = if let Some(info) = &app.group_management.info {
        let conversation_id = app
            .active_conversation
            .as_ref()
            .map(|conversation| conversation.id.as_str())
            .unwrap_or("-");
        vec![
            Line::from(format!(
                "name: {}",
                info.name.clone().unwrap_or_else(|| "-".to_owned())
            )),
            Line::from(format!(
                "creator: {}",
                if info.creator_inbox_id.is_empty() {
                    "-".to_owned()
                } else {
                    short_display_id(&info.creator_inbox_id)
                }
            )),
            Line::from(format!("type: {}", info.conversation_type)),
            Line::from(format!("members: {}", info.member_count)),
            Line::from(format!("conversation_id: {conversation_id}")),
        ]
    } else {
        vec![Line::from(Span::styled(
            "loading...",
            Style::default().dark_gray(),
        ))]
    };
    frame.render_widget(
        Paragraph::new(info_lines).wrap(Wrap { trim: false }),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Esc back to menu",
            Style::default().dark_gray(),
        ))),
        sections[1],
    );
}

fn render_group_members(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(64, 36, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default().title("Members").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);
    if app.group_management.members.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("loading...", Style::default().dark_gray())),
                Line::from(""),
                Line::from(Span::styled(
                    "↑↓ scroll  Esc back to menu",
                    Style::default().dark_gray(),
                )),
            ]),
            inner,
        );
        return;
    }

    let self_inbox_id = app.self_inbox_id();
    let items: Vec<ListItem<'_>> = app
        .group_management
        .members
        .iter()
        .map(|member| {
            let permission_style = match member.permission_level.as_str() {
                "super_admin" | "admin" => Style::default().fg(Color::Yellow),
                "member" => Style::default().fg(Color::DarkGray),
                _ => Style::default(),
            };
            let mut spans = vec![
                Span::styled(short_display_id(&member.inbox_id), Style::default()),
                Span::raw("  "),
                Span::styled(
                    format_permission_level(&member.permission_level),
                    permission_style,
                ),
            ];
            if self_inbox_id == Some(member.inbox_id.as_str()) {
                spans.push(Span::raw("  "));
                spans.push(Span::styled("[you]", Style::default().fg(Color::Cyan)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state = ListState::default().with_offset(app.group_management.info_member_scroll);
    app.group_management
        .members_list_visible_rows
        .set(sections[0].height as usize);
    frame.render_stateful_widget(List::new(items), sections[0], &mut state);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓ scroll  Esc back to menu",
            Style::default().dark_gray(),
        ))),
        sections[1],
    );
}

fn render_group_add_members(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(70, 24, frame.area());
    frame.render_widget(Clear, area);
    let text = vec![
        Line::from("Add members"),
        Line::from("inbox_id list:"),
        Line::from(app.group_management.add_members_input.clone()),
        Line::from("members can be separated by comma or space"),
        Line::from(""),
        Line::from(Span::styled(
            "Enter confirm  Esc cancel",
            Style::default().dark_gray(),
        )),
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
        .block(
            Block::default()
                .title("Remove Members")
                .borders(Borders::ALL),
        );
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
        .block(
            Block::default()
                .title("Remove Members")
                .borders(Borders::ALL),
        )
        .highlight_style(Style::default().reversed());
    frame.render_stateful_widget(list, area, &mut state);
    let hint_area = Rect {
        x: area.x + 1,
        y: area.y + area.height.saturating_sub(2),
        width: area.width.saturating_sub(2),
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Enter remove  Esc cancel",
            Style::default().dark_gray(),
        ))),
        hint_area,
    );
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
        Line::from(""),
        Line::from(Span::styled(
            "Enter confirm  Esc cancel",
            Style::default().dark_gray(),
        )),
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
        Line::from("This will remove this conversation from the current account."),
        Line::from("press y to confirm"),
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

fn format_permission_level(value: &str) -> &str {
    match value {
        "super_admin" => "super_admin",
        "admin" => "admin",
        _ => "member",
    }
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

#[cfg(test)]
mod tests {
    use ratatui::text::Line;
    use ratatui::widgets::ListItem;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::app::{App, Focus};

    use super::{list_offset_for_visible_window, render};

    #[test]
    fn conversations_panel_shows_unread_badge() {
        let (mut app, _) = App::new();
        app.focus = Focus::Conversations;
        app.conversations = vec![
            xmtp_ipc::ConversationItem {
                id: "group-1".into(),
                kind: "group".into(),
                name: Some("Andelf".into()),
                dm_peer_inbox_id: None,
                last_message_ns: None,
            },
            xmtp_ipc::ConversationItem {
                id: "dm-1".into(),
                kind: "dm".into(),
                name: None,
                dm_peer_inbox_id: Some(
                    "461584b40048389e051f95c9f515d6ac39e1802abcdd0b3a9c62c178d329ac00".into(),
                ),
                last_message_ns: None,
            },
        ];
        app.active_conversation_id = Some("group-1".into());
        app.active_conversation = Some(app.conversations[0].clone());
        app.unread_counts.insert("dm-1".into(), 3);

        let backend = TestBackend::new(140, 40);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("render frame");

        let backend = terminal.backend();
        let mut rendered = String::new();
        for y in 0..40 {
            for x in 0..32 {
                rendered.push_str(backend.buffer()[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(
            rendered.contains("4615....ac00"),
            "rendered output:\n{rendered}"
        );
        assert!(
            rendered.contains("(dm) [3]"),
            "rendered output:\n{rendered}"
        );
        println!("{rendered}");
    }

    #[test]
    fn list_offset_keeps_top_item_to_avoid_blank_line_at_bottom() {
        let items = [2usize, 2, 5, 1, 1]
            .into_iter()
            .map(|height| ListItem::new(vec![Line::from("x"); height]))
            .collect::<Vec<_>>();

        let offset = list_offset_for_visible_window(&items, 4, 10);

        assert_eq!(offset, 1);
    }
}
