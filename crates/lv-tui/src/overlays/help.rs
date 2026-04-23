use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::commands::COMMANDS;
use crate::overlay::{centered, Overlay, OverlayAction};

struct KeyEntry {
    key: &'static str,
    description: &'static str,
}

const KEYS: &[KeyEntry] = &[
    KeyEntry { key: "/",                description: "open the command palette; filters as you type" },
    KeyEntry { key: "Enter",            description: "submit input (or run selected command / activate in overlays)" },
    KeyEntry { key: "Up / Down, j / k", description: "scroll chat / navigate palette / navigate lists" },
    KeyEntry { key: "Tab after /index ",description: "complete the partial path" },
    KeyEntry { key: "Tab (chat)",       description: "toggle the context panel" },
    KeyEntry { key: "1–9 in /browse",   description: "filter by language pill; 0 clears" },
    KeyEntry { key: "Ctrl+C, Ctrl+Q",   description: "quit" },
    KeyEntry { key: "Esc / q",          description: "dismiss overlay / palette" },
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
            KeyCode::Esc | KeyCode::Char('q') => OverlayAction::Dismiss,
            _ => OverlayAction::None,
        }
    }
}

fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    let outer = centered(area, 70, 70);

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
            Constraint::Length(COMMANDS.len() as u16 + 2),
            Constraint::Length(KEYS.len() as u16 + 2),
            Constraint::Min(0),
        ])
        .split(inner);

    draw_commands(frame, body[0]);
    draw_keys(frame, body[1]);
}

fn draw_commands(frame: &mut Frame, area: Rect) {
    let col_width = COMMANDS
        .iter()
        .map(|c| c.name.len() + if c.takes_args { 6 } else { 0 })
        .max()
        .unwrap_or(0)
        + 2;

    let lines: Vec<Line> = COMMANDS
        .iter()
        .map(|c| {
            let label = if c.takes_args {
                format!("{} <arg>", c.name)
            } else {
                c.name.to_string()
            };
            Line::from(vec![
                Span::styled(
                    format!("  {:<width$}", label, width = col_width),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(c.description),
            ])
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Slash commands ")),
        area,
    );
}

fn draw_keys(frame: &mut Frame, area: Rect) {
    let col_width = KEYS.iter().map(|e| e.key.len()).max().unwrap_or(0) + 2;
    let lines: Vec<Line> = KEYS
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
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Keys ")),
        area,
    );
}
