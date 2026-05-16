use crossterm::event::{KeyCode, KeyEvent};
use lv_core::status::DbStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::AppCommand;
use crate::widgets::selectable_list::{Item, KeyOutcome, SelectableList};

use super::SectionOutcome;

/// Databases section: left list of DBs with an active-marker, right detail
/// panel describing the selected DB. Enter activates a DB (and drops the
/// user back into Chat); `b` opens a file browser peek.
pub struct DatabasesSection {
    dbs: SelectableList<DbStatus>,
    loaded_once: bool,
    footer_msg: Option<String>,
}

impl Default for DatabasesSection {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabasesSection {
    pub fn new() -> Self {
        Self {
            dbs: SelectableList::new(Vec::new()),
            loaded_once: false,
            footer_msg: None,
        }
    }

    pub fn update(&mut self, dbs: Vec<DbStatus>) {
        let items = dbs.into_iter().map(Self::make_item).collect();
        self.dbs.replace_items(items);
        self.loaded_once = true;
    }

    fn make_item(db: DbStatus) -> Item<DbStatus> {
        let marker = if db.is_current { "▸" } else { " " };
        let name_style = if db.is_current {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let suffix = if db.is_current {
            Span::styled(" *", Style::default().fg(Color::Cyan))
        } else {
            Span::raw("")
        };
        let display = Line::from(vec![
            Span::styled(format!("{marker} "), name_style),
            Span::styled(db.name.clone(), name_style),
            suffix,
        ]);
        Item::new(display, db)
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)])
            .split(area);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(26), Constraint::Min(20)])
            .split(rows[0]);

        self.dbs.draw(
            frame,
            cols[0],
            Block::default()
                .borders(Borders::ALL)
                .title(" Databases ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        self.draw_detail(frame, cols[1]);

        let footer_line = if let Some(msg) = &self.footer_msg {
            Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Red)))
        } else if !self.loaded_once {
            Line::from(Span::styled(
                " loading…",
                Style::default().fg(Color::DarkGray),
            ))
        } else if self.dbs.items().is_empty() {
            Line::from(Span::styled(
                " no databases yet — go to [4] Index to create one",
                Style::default().fg(Color::DarkGray),
            ))
        } else {
            Line::raw("")
        };
        frame.render_widget(Paragraph::new(footer_line), rows[1]);
    }

    fn draw_detail(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        let title;
        if let Some(db) = self.dbs.selected_meta() {
            title = format!(" Detail · {} ", db.name);
            lines.push(kv_line("Path", db.path.display().to_string()));
            lines.push(kv_line(
                "Indexed at",
                db.indexed_at.clone().unwrap_or_else(|| "-".into()),
            ));
            lines.push(kv_line("Files", db.unique_files.to_string()));
            lines.push(kv_line("Chunks", db.total_chunks.to_string()));
            let langs = if db.languages.is_empty() {
                "(none)".to_string()
            } else {
                db.languages
                    .iter()
                    .take(5)
                    .map(|(k, v)| format!("{k}:{v}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            };
            lines.push(kv_line("Languages", langs));
            if let Some(err) = &db.error {
                lines.push(Line::raw(""));
                lines.push(Line::from(vec![
                    Span::styled(
                        "error: ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(err.clone(), Style::default().fg(Color::Red)),
                ]));
            }
        } else {
            title = " Detail ".to_string();
            lines.push(Line::from(Span::styled(
                "no databases",
                Style::default().fg(Color::DarkGray),
            )));
        }

        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SectionOutcome {
        self.footer_msg = None;
        match self.dbs.handle_key(key) {
            KeyOutcome::Consumed => SectionOutcome::Consumed,
            KeyOutcome::Unhandled => SectionOutcome::Unhandled,
            KeyOutcome::Escape => SectionOutcome::Consumed,
            KeyOutcome::Activate(_) => match self.dbs.selected_meta() {
                Some(db) => SectionOutcome::RunCommand(AppCommand::SwitchDb(db.name.clone())),
                None => SectionOutcome::Consumed,
            },
            KeyOutcome::Key(k) => match k.code {
                KeyCode::Char('b') => match self.dbs.selected_meta() {
                    Some(db) => SectionOutcome::RunCommand(AppCommand::Browse(db.name.clone())),
                    None => SectionOutcome::Consumed,
                },
                _ => SectionOutcome::Unhandled,
            },
        }
    }

    pub fn keyhints(&self) -> &'static str {
        "↑↓ select  ·  Enter activate  ·  b browse files  ·  F4 index into DB  ·  F1..F5 sections"
    }
}

fn kv_line(key: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<11} "), Style::default().fg(Color::Yellow)),
        Span::styled(value, Style::default().fg(Color::White)),
    ])
}
