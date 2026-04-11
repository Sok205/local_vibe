use lv_core::types::{Message, Role};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub struct ChatView {
    pub messages: Vec<Message>,
    pub streaming_buf: String,
    pub scroll: u16,
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatView {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            streaming_buf: String::new(),
            scroll: 0,
        }
    }

    pub fn push_token(&mut self, token: &str) {
        self.streaming_buf.push_str(token);
    }

    pub fn finish_stream(&mut self) {
        if !self.streaming_buf.is_empty() {
            let content = std::mem::take(&mut self.streaming_buf);
            self.messages.push(Message {
                role: Role::Assistant,
                content,
            });
        }
        self.scroll_to_bottom();
    }

    pub fn push_message(&mut self, msg: Message) {
        self.messages.push(msg);
        self.scroll_to_bottom();
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll = u16::MAX;
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        for msg in &self.messages {
            let (prefix, color) = match msg.role {
                Role::User => ("You: ", Color::Green),
                Role::Assistant => ("AI:  ", Color::Cyan),
                Role::System => ("Sys: ", Color::DarkGray),
            };

            for (i, text_line) in msg.content.lines().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(color)),
                        Span::styled(text_line.to_string(), Style::default().fg(color)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(text_line.to_string(), Style::default().fg(color)),
                    ]));
                }
            }
            lines.push(Line::raw(""));
        }

        if !self.streaming_buf.is_empty() {
            for (i, text_line) in self.streaming_buf.lines().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![
                        Span::styled("AI:  ", Style::default().fg(Color::Cyan)),
                        Span::styled(text_line.to_string(), Style::default().fg(Color::Cyan)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(text_line.to_string(), Style::default().fg(Color::Cyan)),
                    ]));
                }
            }
        }

        let total = lines.len() as u16;
        let visible = area.height.saturating_sub(2);
        let scroll = if self.scroll == u16::MAX {
            total.saturating_sub(visible)
        } else {
            self.scroll.min(total.saturating_sub(visible))
        };

        let widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Chat"))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));

        frame.render_widget(widget, area);
    }
}
