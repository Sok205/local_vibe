use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent};
use lv_core::types::ModelTier;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::overlay::centered;

struct Item {
    tier: ModelTier,
    label: &'static str,
    desc: &'static str,
    enabled: bool,
}

pub struct StartupOverlay {
    items: Vec<Item>,
    cursor: usize,
}

impl Default for StartupOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupOverlay {
    pub fn new() -> Self {
        Self {
            items: vec![
                Item {
                    tier: ModelTier::Fast,
                    label: "Fast  ",
                    desc: "small / quick responses",
                    enabled: true,
                },
                Item {
                    tier: ModelTier::Medium,
                    label: "Medium",
                    desc: "balanced quality + speed",
                    enabled: true,
                },
                Item {
                    tier: ModelTier::Strong,
                    label: "Strong",
                    desc: "best quality, heavy model",
                    enabled: true,
                },
            ],
            cursor: 0,
        }
    }

    /// Returns the set of disabled tiers after the user confirms.
    pub fn confirm(&self) -> HashSet<ModelTier> {
        self.items
            .iter()
            .filter(|i| !i.enabled)
            .map(|i| i.tier)
            .collect()
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let popup = centered(area, 56, 50);
        frame.render_widget(Clear, popup);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Which chat models do you need this session?",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));

        for (i, item) in self.items.iter().enumerate() {
            let is_cursor = i == self.cursor;
            let check = if item.enabled { "◉" } else { "○" };
            let check_color = if item.enabled {
                Color::Green
            } else {
                Color::DarkGray
            };
            let label_style = if is_cursor {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if item.enabled {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let cursor_indicator = if is_cursor { "▶ " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {cursor_indicator}"),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(check.to_string(), Style::default().fg(check_color)),
                Span::raw("  "),
                Span::styled(item.label.to_string(), label_style),
                Span::raw("  "),
                Span::styled(item.desc.to_string(), Style::default().fg(Color::DarkGray)),
            ]));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  ◉ Embedding  always available (lightweight, used for search)",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Space toggle  ·  ↑↓ move  ·  Enter confirm  ·  Esc enable all",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::raw(""));

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" local-vibe — session setup ")
            .border_style(Style::default().fg(Color::Cyan));

        let widget = Paragraph::new(lines).block(block);
        frame.render_widget(widget, popup);
    }

    /// Returns `true` when the overlay should be dismissed (Enter or Esc).
    /// Esc = enable all before dismissing.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                false
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor + 1 < self.items.len() {
                    self.cursor += 1;
                }
                false
            }
            KeyCode::Char(' ') => {
                self.items[self.cursor].enabled = !self.items[self.cursor].enabled;
                false
            }
            KeyCode::Enter => true,
            KeyCode::Esc => {
                for item in &mut self.items {
                    item.enabled = true;
                }
                true
            }
            _ => false,
        }
    }
}
