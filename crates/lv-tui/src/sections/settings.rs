use crossterm::event::KeyEvent;
use lv_core::status::StatusSnapshot;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::sys_memory::{MemorySample, fmt_bytes};

use super::SectionOutcome;

/// Read-only Settings + Help. Mirrors what users used to dig out of
/// `/status` and `/help`, now surfaced ambiently.
pub struct SettingsSection {
    snapshot: Option<StatusSnapshot>,
    memory: MemorySample,
}

impl Default for SettingsSection {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsSection {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            memory: MemorySample::default(),
        }
    }

    pub fn update(&mut self, snapshot: StatusSnapshot) {
        self.snapshot = Some(snapshot);
    }

    pub fn set_memory(&mut self, sample: MemorySample) {
        self.memory = sample;
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        self.draw_info(frame, cols[0]);
        self.draw_keybinds(frame, cols[1]);
    }

    fn draw_info(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        lines.push(kv("Version", env!("CARGO_PKG_VERSION").to_string()));

        // Memory panel — always present, updates live from the poller.
        if self.memory.rss_bytes > 0 {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                "  Memory",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(kv("  RSS", fmt_bytes(self.memory.rss_bytes)));
            lines.push(kv("  Virtual", fmt_bytes(self.memory.vsize_bytes)));
            if self.memory.swap_total_bytes > 0 {
                let pct = (self.memory.swap_used_bytes as f64
                    / self.memory.swap_total_bytes as f64)
                    * 100.0;
                lines.push(kv(
                    "  Swap (system)",
                    format!(
                        "{} / {} ({:.0}%)",
                        fmt_bytes(self.memory.swap_used_bytes),
                        fmt_bytes(self.memory.swap_total_bytes),
                        pct,
                    ),
                ));
            }
            lines.push(Line::raw(""));
        }

        if let Some(snap) = &self.snapshot {
            if let Some(cp) = &snap.config_path {
                lines.push(kv("Config", cp.display().to_string()));
            } else {
                lines.push(kv("Config", "(none found)".into()));
            }
            if let Some(root) = &snap.db_root {
                lines.push(kv("DB root", root.display().to_string()));
            }
            if let Some(rt) = &snap.runtime {
                lines.push(kv("Process", format!("pid {}", rt.pid)));
                let warm_models = if rt.warm_models.is_empty() {
                    "(none)".into()
                } else {
                    rt.warm_models.join(", ")
                };
                lines.push(kv("Warm models", warm_models));
                let warm_dbs = if rt.warm_dbs.is_empty() {
                    "(none)".into()
                } else {
                    rt.warm_dbs.join(", ")
                };
                lines.push(kv("Warm DBs", warm_dbs));
                if let Some(sid) = &rt.session_id {
                    lines.push(kv("Session", sid.clone()));
                }
            }
        } else {
            lines.push(Line::from(Span::styled(
                "  loading runtime info…",
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  MCP server is a separate subcommand — run `lv serve` in another shell.",
            Style::default().fg(Color::DarkGray),
        )));

        let widget = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Settings ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(widget, area);
    }

    fn draw_keybinds(&self, frame: &mut Frame, area: Rect) {
        let entries: &[(&str, &str)] = &[
            ("F1..F5",       "switch section"),
            ("Ctrl+1..5",    "same (fallback — Ctrl+1..3 silent on most macOS terms)"),
            ("Ctrl+C, Ctrl+Q", "quit"),
            ("Tab",          "cycle focus inside section"),
            ("Esc",          "back out / dismiss peek"),
            ("?",            "open this help (where not typing)"),
            ("",             ""),
            ("Chat — Enter", "send message"),
            ("Chat — ↑↓",    "scroll history"),
            ("Models — Enter", "load + activate tier"),
            ("Models — l/u/a", "load / unload / set active"),
            ("DBs — Enter",  "activate the selected DB"),
            ("DBs — b",      "browse files in the DB"),
            ("Index — Tab",  "path completion, then focus cycle"),
            ("Index — Enter", "start indexing"),
        ];

        let col_width = entries
            .iter()
            .map(|(k, _)| k.len())
            .max()
            .unwrap_or(0)
            + 2;
        let lines: Vec<Line> = entries
            .iter()
            .map(|(k, v)| {
                if k.is_empty() {
                    Line::raw("")
                } else {
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<width$}", k, width = col_width),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(v.to_string(), Style::default().fg(Color::Gray)),
                    ])
                }
            })
            .collect();

        let widget = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Keybinds ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(widget, area);
    }

    pub fn handle_key(&self, _key: KeyEvent) -> SectionOutcome {
        SectionOutcome::Unhandled
    }

    pub fn keyhints(&self) -> &'static str {
        "read-only  ·  F1..F5 switch section  ·  Ctrl+Q quit"
    }
}

fn kv(key: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<14}"),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(value, Style::default().fg(Color::White)),
    ])
}
