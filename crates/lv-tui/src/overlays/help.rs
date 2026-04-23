use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::overlay::{centered, Overlay, OverlayAction};

struct Entry {
    command: &'static str,
    description: &'static str,
}

const COMMANDS: &[Entry] = &[
    Entry { command: "/help, /?",        description: "show this help" },
    Entry { command: "/status",          description: "models, DBs, runtime — Enter on a DB drills into browse" },
    Entry { command: "/models",          description: "load / unload / activate models" },
    Entry { command: "/browse [db]",     description: "browse files inside a DB" },
    Entry { command: "/dbs",             description: "list indexed DBs" },
    Entry { command: "/db <name>",       description: "switch current DB" },
    Entry { command: "/index [path]",    description: "index a directory (no args = picker)" },
    Entry { command: "/quit",            description: "exit" },
];

const KEYS: &[Entry] = &[
    Entry { command: "Enter",            description: "submit input (or activate in overlays)" },
    Entry { command: "Up / Down, j / k", description: "scroll chat / navigate list" },
    Entry { command: "/",                description: "start fuzzy filter inside a list" },
    Entry { command: "Tab",              description: "toggle context panel (or complete /index path)" },
    Entry { command: "Ctrl+C, Ctrl+Q",   description: "quit" },
    Entry { command: "Esc / q",          description: "dismiss overlay" },
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

    draw_section(frame, body[0], " Slash commands ", COMMANDS);
    draw_section(frame, body[1], " Keys ", KEYS);
}

fn draw_section(frame: &mut Frame, area: Rect, title: &str, entries: &[Entry]) {
    let col_width = entries.iter().map(|e| e.command.len()).max().unwrap_or(0) + 2;

    let lines: Vec<Line> = entries
        .iter()
        .map(|e| {
            Line::from(vec![
                Span::styled(
                    format!("  {:<width$}", e.command, width = col_width),
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
            .block(Block::default().borders(Borders::ALL).title(title.to_string())),
        area,
    );
}
