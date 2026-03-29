use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
pub fn render_markdown(md: &str, wrap_width: usize) -> Vec<Line<'static>> {
    let width = wrap_width.max(1);
    if md.trim().is_empty() {
        return vec![Line::from("")];
    }
    let parser = Parser::new_ext(md, Options::all());
    let mut renderer = MarkdownRenderer::new(width);
    for event in parser {
        renderer.push_event(event);
    }
    renderer.finish()
}

struct MarkdownRenderer {
    wrap_width: usize,
    lines: Vec<Line<'static>>,
    inline_spans: Vec<Span<'static>>,
    strong_depth: usize,
    emphasis_depth: usize,
    heading_level: Option<HeadingLevel>,
    list_state: Option<ListState>,
    in_item: bool,
    in_code_block: bool,
    code_block_lines: Vec<String>,
}

struct ListState {
    ordered: bool,
    next_number: u64,
}

impl MarkdownRenderer {
    fn new(wrap_width: usize) -> Self {
        Self {
            wrap_width,
            lines: Vec::new(),
            inline_spans: Vec::new(),
            strong_depth: 0,
            emphasis_depth: 0,
            heading_level: None,
            list_state: None,
            in_item: false,
            in_code_block: false,
            code_block_lines: Vec::new(),
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_inline(false);
        if self.lines.is_empty() {
            vec![Line::from("")]
        } else {
            self.lines
        }
    }

    fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.push_text(text.as_ref()),
            Event::Code(code) => self.push_inline_code(code.as_ref()),
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => {
                self.flush_inline(false);
            }
            Event::Rule => {
                self.flush_inline(true);
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(self.wrap_width.max(3)),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) => {}
            Event::TaskListMarker(checked) => {
                self.push_text(if checked { "[x] " } else { "[ ] " });
            }
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                if self.in_item {
                    self.push_list_prefix();
                }
            }
            Tag::Heading { level, .. } => {
                self.flush_inline(true);
                self.heading_level = Some(level);
                self.inline_spans.push(Span::styled(
                    format!("{} ", heading_prefix(level)),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Tag::Strong => self.strong_depth += 1,
            Tag::Emphasis => self.emphasis_depth += 1,
            Tag::CodeBlock(kind) => {
                self.flush_inline(true);
                self.in_code_block = true;
                self.code_block_lines.clear();
                if let CodeBlockKind::Fenced(lang) = kind {
                    let lang = lang.trim();
                    if !lang.is_empty() {
                        self.code_block_lines.push(format!("  ```{lang}"));
                    }
                }
            }
            Tag::List(start) => {
                self.flush_inline(true);
                self.list_state = Some(ListState {
                    ordered: start.is_some(),
                    next_number: start.unwrap_or(1),
                });
            }
            Tag::Item => {
                self.flush_inline(false);
                self.in_item = true;
                self.push_list_prefix();
            }
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush_inline(true),
            TagEnd::Heading(_) => {
                self.flush_inline(true);
                self.heading_level = None;
            }
            TagEnd::Strong => {
                self.strong_depth = self.strong_depth.saturating_sub(1);
            }
            TagEnd::Emphasis => {
                self.emphasis_depth = self.emphasis_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                self.flush_code_block();
            }
            TagEnd::Item => {
                self.flush_inline(false);
                self.in_item = false;
                if let Some(list) = self.list_state.as_mut()
                    && list.ordered
                {
                    list.next_number += 1;
                }
            }
            TagEnd::List(_) => {
                self.flush_inline(true);
                self.list_state = None;
            }
            _ => {}
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.in_code_block {
            for line in text.lines() {
                self.code_block_lines.push(format!("  {line}"));
            }
            if text.ends_with('\n') && !text.is_empty() {
                self.code_block_lines.push("  ".to_owned());
            }
            return;
        }
        let style = self.current_inline_style();
        self.inline_spans.push(Span::styled(text.to_owned(), style));
    }

    fn push_inline_code(&mut self, code: &str) {
        self.inline_spans.push(Span::styled(
            format!(" {code} "),
            Style::default().fg(Color::White).bg(Color::DarkGray),
        ));
    }

    fn current_inline_style(&self) -> Style {
        let mut style = if self.heading_level.is_some() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };
        if self.heading_level.is_some() || self.strong_depth > 0 {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.emphasis_depth > 0 {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }

    fn push_list_prefix(&mut self) {
        if !self.inline_spans.is_empty() {
            return;
        }
        if let Some(list) = &self.list_state {
            let prefix = if list.ordered {
                format!("{}. ", list.next_number)
            } else {
                "• ".to_owned()
            };
            self.inline_spans.push(Span::raw(prefix));
        }
    }

    fn flush_inline(&mut self, blank_line_after: bool) {
        if !self.inline_spans.is_empty() {
            let wrapped = wrap_styled_spans(&self.inline_spans, self.wrap_width);
            self.lines.extend(wrapped);
            self.inline_spans.clear();
        }
        if blank_line_after && self.lines.last().is_some_and(|line| !line.spans.is_empty()) {
            self.lines.push(Line::from(""));
        }
    }

    fn flush_code_block(&mut self) {
        self.in_code_block = false;
        if self.code_block_lines.is_empty() {
            self.lines.push(Line::from(Span::styled(
                "  ",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for line in self.code_block_lines.drain(..) {
                self.lines.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
        self.lines.push(Line::from(""));
    }
}

fn heading_prefix(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "#",
        HeadingLevel::H2 => "##",
        HeadingLevel::H3 => "###",
        HeadingLevel::H4 => "####",
        HeadingLevel::H5 => "#####",
        HeadingLevel::H6 => "######",
    }
}

fn wrap_styled_spans(spans: &[Span<'static>], width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0usize;

    for span in spans {
        let text = span.content.as_ref();
        let style = span.style;
        for segment in
            split_wrapped_segments(text, width, &mut current_width, &mut lines, &mut current)
        {
            if !segment.is_empty() {
                current.push(Span::styled(segment, style));
            }
        }
    }

    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    if lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines
}

fn split_wrapped_segments(
    text: &str,
    width: usize,
    current_width: &mut usize,
    lines: &mut Vec<Line<'static>>,
    current: &mut Vec<Span<'static>>,
) -> Vec<String> {
    let mut segments = Vec::new();
    let mut buffer = String::new();

    for ch in text.chars() {
        if ch == '\n' {
            if !buffer.is_empty() {
                segments.push(std::mem::take(&mut buffer));
            }
            lines.push(Line::from(std::mem::take(current)));
            *current_width = 0;
            continue;
        }

        if *current_width >= width {
            if !buffer.is_empty() {
                segments.push(std::mem::take(&mut buffer));
            }
            lines.push(Line::from(std::mem::take(current)));
            *current_width = 0;
        }

        buffer.push(ch);
        *current_width += 1;
    }

    if !buffer.is_empty() {
        segments.push(buffer);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::render_markdown;

    fn line_text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn renders_heading_with_prefix() {
        let lines = render_markdown("# Title", 40);
        assert_eq!(line_text(&lines[0]), "# Title");
    }

    #[test]
    fn renders_rule_as_horizontal_line() {
        let lines = render_markdown("---", 8);
        assert_eq!(line_text(&lines[0]), "────────");
    }

    #[test]
    fn renders_unordered_list_prefix() {
        let lines = render_markdown("- one\n- two", 40);
        assert_eq!(line_text(&lines[0]), "• one");
        assert_eq!(line_text(&lines[1]), "• two");
    }

    #[test]
    fn renders_inline_code_text() {
        let lines = render_markdown("hello `code`", 40);
        assert_eq!(line_text(&lines[0]), "hello  code ");
    }

    #[test]
    fn renders_plain_text_as_non_empty_lines() {
        let lines = render_markdown("plain text only", 40);
        assert!(!lines.is_empty());
        assert_eq!(line_text(&lines[0]), "plain text only");
    }
}
