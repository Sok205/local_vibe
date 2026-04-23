use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::overlay::{Overlay, OverlayAction, centered};

struct KeyEntry {
    key: &'static str,
    description: &'static str,
}

const GLOBAL_KEYS: &[KeyEntry] = &[
    KeyEntry { key: "F1..F5",        description: "switch section (Chat / Models / Databases / Index / Settings)" },
    KeyEntry { key: "Ctrl+1..5",     description: "same (fallback — many macOS terms eat Ctrl+1..3)" },
    KeyEntry { key: "Ctrl+C, Ctrl+Q", description: "quit" },
    KeyEntry { key: "Esc",           description: "back out of a focused sub-pane or overlay" },
    KeyEntry { key: "?",             description: "toggle this help (when not typing in chat)" },
];

const CHAT_KEYS: &[KeyEntry] = &[
    KeyEntry { key: "Enter",  description: "send the current message" },
    KeyEntry { key: "Tab",    description: "toggle focus between chat input and context pane" },
    KeyEntry { key: "↑ / ↓",  description: "scroll chat history (input focus) / move selection (context focus)" },
];

pub struct HelpOverlay;

impl Default for HelpOverlay {
    fn default() -> Self {
        Self
    }
}

impl Overlay for HelpOverlay {
    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        draw_help_overlay(frame, area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> OverlayAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => OverlayAction::Dismiss,
            _ => OverlayAction::None,
        }
    }
}

fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    let outer = centered(area, 70, 60);

    frame.render_widget(
        Paragraph::new("").block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help — press Esc to close "),
        ),
        outer,
    );

    let inner = Rect {
        x: outer.x + 1,
        y: outer.y + 1,
        width: outer.width.saturating_sub(2),
        height: outer.height.saturating_sub(2),
    };

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(GLOBAL_KEYS.len() as u16 + 2),
            Constraint::Length(CHAT_KEYS.len() as u16 + 2),
            Constraint::Min(0),
        ])
        .split(inner);

    draw_keys(frame, body[0], " Global ", GLOBAL_KEYS);
    draw_keys(frame, body[1], " Chat ", CHAT_KEYS);
}

fn draw_keys(frame: &mut Frame, area: Rect, title: &str, entries: &[KeyEntry]) {
    let col_width = entries.iter().map(|e| e.key.len()).max().unwrap_or(0) + 2;
    let lines: Vec<Line> = entries
        .iter()
        .map(|e| {
            Line::from(vec![
                Span::styled(
                    format!("  {:<width$}", e.key, width = col_width),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(e.description),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}
