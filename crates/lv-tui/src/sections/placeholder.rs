use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::SectionOutcome;

/// A stub section used during the rollout: renders a short description of
/// what will live here once its phase lands. Each placeholder is stateless.
pub struct PlaceholderSection {
    title: &'static str,
    phase: u8,
    summary: &'static str,
}

impl PlaceholderSection {
    pub const fn models() -> Self {
        Self {
            title: "Models",
            phase: 2,
            summary: "Load, unload, and activate chat-tier models here. \
                      For now use `lv models` / `lv load` from the shell.",
        }
    }

    pub const fn databases() -> Self {
        Self {
            title: "Databases",
            phase: 3,
            summary: "Browse indexed knowledge bases, see per-DB stats, \
                      and activate a DB for chat.",
        }
    }

    pub const fn index() -> Self {
        Self {
            title: "Index",
            phase: 4,
            summary: "Pick a directory, pick a DB, and run indexing with live \
                      progress. For now use `lv index <path> <db>`.",
        }
    }

    pub const fn settings() -> Self {
        Self {
            title: "Settings",
            phase: 5,
            summary: "Config paths, cache locations, version, and a compact \
                      keybind reference. Read-only.",
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::raw(""),
            Line::from(Span::styled(
                self.title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            Line::raw(""),
            Line::from(Span::styled(
                format!("— arriving in Phase {} —", self.phase),
                Style::default().fg(Color::DarkGray),
            ))
            .alignment(Alignment::Center),
            Line::raw(""),
            Line::from(Span::styled(
                self.summary,
                Style::default().fg(Color::Gray),
            ))
            .alignment(Alignment::Center),
        ];
        let widget = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(format!(" {} ", self.title)),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(widget, area);
    }

    pub fn handle_key(&self, _key: KeyEvent) -> SectionOutcome {
        SectionOutcome::Unhandled
    }

    pub fn keyhints(&self) -> &'static str {
        "Ctrl+1..5 switch section  ·  Ctrl+Q quit"
    }
}
