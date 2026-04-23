use crossterm::event::KeyEvent;
use lv_core::status::{DbStatus, Readiness, StatusSnapshot};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::AppCommand;
use crate::overlay::{centered, Overlay, OverlayAction};
use crate::widgets::selectable_list::{Item, KeyOutcome, SelectableList};

pub struct StatusOverlay {
    snapshot: StatusSnapshot,
    dbs: SelectableList<DbStatus>,
}

impl StatusOverlay {
    pub fn new(snapshot: StatusSnapshot) -> Self {
        let dbs = Self::build_db_list(&snapshot.databases);
        Self { snapshot, dbs }
    }

    fn build_db_list(dbs: &[DbStatus]) -> SelectableList<DbStatus> {
        let items: Vec<Item<DbStatus>> = dbs
            .iter()
            .map(|db| {
                let marker = if db.is_current { "*" } else { " " };
                let head = format!(
                    "{marker} {} — {} chunks, {} files, indexed {}",
                    db.name,
                    db.total_chunks,
                    db.unique_files,
                    db.indexed_at.as_deref().unwrap_or("-"),
                );
                let style = if db.is_current {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let mut spans = vec![Span::styled(head, style)];
                if let Some(err) = &db.error {
                    spans.push(Span::styled(
                        format!("  [err: {err}]"),
                        Style::default().fg(Color::Red),
                    ));
                } else if !db.languages.is_empty() {
                    let preview: Vec<String> = db
                        .languages
                        .iter()
                        .take(4)
                        .map(|(k, v)| format!("{k}:{v}"))
                        .collect();
                    spans.push(Span::styled(
                        format!("  — {}", preview.join(", ")),
                        Style::default().fg(Color::Green),
                    ));
                }
                Item::new(Line::from(spans), db.clone())
            })
            .collect();
        SelectableList::new(items)
    }
}

impl Overlay for StatusOverlay {
    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let outer = centered(area, 70, 80);

        frame.render_widget(
            Paragraph::new("").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Status — ↑/↓ to navigate DBs, Enter to browse, Esc to close "),
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
                Constraint::Length(models_lines(&self.snapshot) as u16 + 2),
                Constraint::Min(3),
                Constraint::Length(4),
            ])
            .split(inner);

        draw_models_block(frame, body[0], &self.snapshot);

        self.dbs.draw(
            frame,
            body[1],
            Block::default().borders(Borders::ALL).title(" Databases "),
        );

        draw_runtime_block(frame, body[2], &self.snapshot);
    }

    fn handle_key(&mut self, key: KeyEvent) -> OverlayAction {
        match self.dbs.handle_key(key) {
            KeyOutcome::Consumed | KeyOutcome::Unhandled | KeyOutcome::Key(_) => OverlayAction::None,
            KeyOutcome::Escape => OverlayAction::Dismiss,
            KeyOutcome::Activate(_) => match self.dbs.selected_meta() {
                Some(db) => OverlayAction::RunCommand(AppCommand::Browse(db.name.clone())),
                None => OverlayAction::None,
            },
        }
    }
}

fn models_lines(s: &StatusSnapshot) -> usize {
    4 + s.models.cloud.is_some() as usize
}

fn ready_span(r: &Readiness) -> Span<'static> {
    let (text, color) = match r {
        Readiness::Ready => ("ok", Color::Green),
        Readiness::MissingWeights => ("missing weights", Color::Red),
        Readiness::Unknown => ("unknown", Color::Gray),
    };
    Span::styled(text.to_string(), Style::default().fg(color))
}

fn draw_models_block(frame: &mut Frame, area: Rect, s: &StatusSnapshot) {
    let m = &s.models;
    let mut lines: Vec<Line> = Vec::new();
    for (label, slot) in [
        ("fast   ", &m.fast),
        ("medium ", &m.medium),
        ("strong ", &m.strong),
    ] {
        lines.push(Line::from(vec![
            Span::styled(label, Style::default().fg(Color::Yellow)),
            Span::raw(format!(" {} ({}) ", slot.name, slot.backend)),
            ready_span(&slot.ready),
        ]));
    }
    let emb_line = match &m.embedding {
        Some(slot) => Line::from(vec![
            Span::styled("embed  ", Style::default().fg(Color::Yellow)),
            Span::raw(format!(" {} ({}) ", slot.name, slot.backend)),
            ready_span(&slot.ready),
        ]),
        None => Line::from(vec![
            Span::styled("embed  ", Style::default().fg(Color::Yellow)),
            Span::styled(
                " (not configured — RAG disabled)",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    };
    lines.push(emb_line);
    if let Some(c) = &m.cloud {
        lines.push(Line::from(vec![
            Span::styled("cloud  ", Style::default().fg(Color::Yellow)),
            Span::raw(format!(" {} via {}", c.model, c.provider)),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Models ")),
        area,
    );
}

fn draw_runtime_block(frame: &mut Frame, area: Rect, s: &StatusSnapshot) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(root) = &s.db_root {
        lines.push(Line::from(format!("db_root: {}", root.display())));
    }
    if let Some(cp) = &s.config_path {
        lines.push(Line::from(format!("config:  {}", cp.display())));
    }
    if let Some(r) = &s.runtime {
        let warm_models = if r.warm_models.is_empty() {
            "(none)".to_string()
        } else {
            r.warm_models.join(", ")
        };
        let warm_dbs = if r.warm_dbs.is_empty() {
            "(none)".to_string()
        } else {
            r.warm_dbs.join(", ")
        };
        lines.push(Line::from(format!(
            "pid {} · warm models: {warm_models} · warm dbs: {warm_dbs}",
            r.pid
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(Block::default().borders(Borders::ALL).title(" Runtime ")),
        area,
    );
}
