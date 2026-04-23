use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

struct Entry {
    command: &'static str,
    description: &'static str,
}

const COMMANDS: &[Entry] = &[
    Entry { command: "/help, /?",        description: "show this help" },
    Entry { command: "/status",          description: "full snapshot: models, DBs, runtime state" },
    Entry { command: "/dbs",             description: "list indexed DBs" },
    Entry { command: "/db <name>",       description: "switch current DB" },
    Entry { command: "/index <path> [name]", description: "index a directory (optional DB name)" },
    Entry { command: "/quit",            description: "exit" },
];

const KEYS: &[Entry] = &[
    Entry { command: "Enter",            description: "submit input" },
    Entry { command: "Up / Down",        description: "scroll chat" },
    Entry { command: "Tab",              description: "toggle context panel" },
    Entry { command: "Ctrl+C, Ctrl+Q",   description: "quit" },
    Entry { command: "Esc / q",          description: "dismiss overlay (help / status)" },
];

pub fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    let outer = centered(area, 60, 60);

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
    let col_width = entries
        .iter()
        .map(|e| e.command.len())
        .max()
        .unwrap_or(0)
        + 2;

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
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title.to_string())),
        area,
    );
}

fn centered(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(popup_layout[1])[1]
}
