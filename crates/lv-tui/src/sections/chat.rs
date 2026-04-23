use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::chat_view::ChatView;
use crate::context_panel::ContextPanel;
use crate::input::{InputAction, InputBuffer};

use super::SectionOutcome;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Focus {
    Input,
    Context,
}

/// The Chat section: conversation + input on the left, retrieved context on
/// the right. Context is always visible — it's the thing that makes this app
/// different from a plain chat TUI.
pub struct ChatSection {
    pub chat: ChatView,
    pub context: ContextPanel,
    pub input: InputBuffer,
    focus: Focus,
}

impl Default for ChatSection {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatSection {
    pub fn new() -> Self {
        Self {
            chat: ChatView::new(),
            context: ContextPanel::new(),
            input: InputBuffer::new(),
            focus: Focus::Input,
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        let left_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(3)])
            .split(cols[0]);

        self.chat.draw(frame, left_rows[0]);
        self.draw_input(frame, left_rows[1]);
        self.context.draw(frame, cols[1]);
    }

    fn draw_input(&self, frame: &mut Frame, area: Rect) {
        let input_text: String = self.input.as_str();
        let border = if self.focus == Focus::Input {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        let widget = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Yellow)),
            Span::raw(input_text),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border)),
        );
        frame.render_widget(widget, area);

        if self.focus == Focus::Input {
            let cursor_x = area.x + 1 + 2 + self.input.cursor as u16;
            let cursor_y = area.y + 1;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SectionOutcome {
        if matches!(key.code, KeyCode::Tab) {
            self.focus = match self.focus {
                Focus::Input => Focus::Context,
                Focus::Context => Focus::Input,
            };
            return SectionOutcome::Consumed;
        }

        match self.focus {
            Focus::Input => match self.input.handle_key(key) {
                InputAction::Submit(text) => SectionOutcome::Submit(text),
                InputAction::ScrollUp => {
                    self.chat.scroll_up();
                    SectionOutcome::Consumed
                }
                InputAction::ScrollDown => {
                    self.chat.scroll_down();
                    SectionOutcome::Consumed
                }
                InputAction::Quit => SectionOutcome::Unhandled,
                InputAction::ToggleContext | InputAction::None => SectionOutcome::Consumed,
            },
            Focus::Context => match key.code {
                KeyCode::Esc => {
                    self.focus = Focus::Input;
                    SectionOutcome::Consumed
                }
                KeyCode::Up | KeyCode::Down => SectionOutcome::Consumed,
                _ => SectionOutcome::Unhandled,
            },
        }
    }

    pub fn keyhints(&self) -> &'static str {
        match self.focus {
            Focus::Input => "Enter send  ·  Tab → Context  ·  ↑↓ scroll  ·  Ctrl+1..5 sections  ·  Ctrl+Q quit",
            Focus::Context => "Tab → Chat  ·  Esc back  ·  (source peek coming soon)",
        }
    }
}
