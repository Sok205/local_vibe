use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};

use crate::app::{AppCommand, IndexingProgress};
use crate::input_complete::complete_path;
use crate::widgets::text_input::TextInput;

use super::SectionOutcome;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Focus {
    Path,
    Db,
}

/// Index section: pick a path + target DB, hit Enter, watch progress.
pub struct IndexSection {
    path: TextInput,
    db: TextInput,
    focus: Focus,
    footer_msg: Option<String>,
    submitting: bool,
}

impl Default for IndexSection {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexSection {
    pub fn new() -> Self {
        Self {
            path: TextInput::new(),
            db: TextInput::new(),
            focus: Focus::Path,
            footer_msg: None,
            submitting: false,
        }
    }

    /// Called when the user enters the section; prefills the DB field with
    /// the currently active DB so hitting Enter "just works" for the common
    /// case of extending the active knowledge base.
    pub fn prefill_db(&mut self, active_db: &str) {
        if self.db.is_empty() {
            self.db = TextInput::with_value(active_db);
        }
    }

    pub fn on_index_done(&mut self, indexed: usize, skipped: usize, failed: usize) {
        self.submitting = false;
        self.footer_msg = Some(format!(
            "indexed {indexed}, skipped {skipped}, failed {failed}"
        ));
        self.path = TextInput::new();
        self.focus = Focus::Path;
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, progress: Option<&IndexingProgress>) {
        let outer = Block::default()
            .borders(Borders::ALL)
            .title(" Index ")
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(inner);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " Pick a directory and a destination DB. Enter starts indexing.",
                Style::default().fg(Color::DarkGray),
            ))),
            rows[0],
        );

        self.draw_field(frame, rows[1], "Path", &self.path, self.focus == Focus::Path);
        self.draw_field(frame, rows[2], "Into", &self.db, self.focus == Focus::Db);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  Enter to start  ·  Tab complete/focus  ·  ↑↓ cycle fields",
                Style::default().fg(Color::DarkGray),
            ))),
            rows[3],
        );

        self.draw_progress(frame, rows[4], progress);
        self.draw_footer(frame, rows[5]);
    }

    fn draw_field(
        &self,
        frame: &mut Frame,
        area: Rect,
        label: &str,
        input: &TextInput,
        focused: bool,
    ) {
        let border = if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        let title = Line::from(vec![
            Span::styled(
                format!(" {label} "),
                Style::default()
                    .fg(if focused { Color::Cyan } else { Color::Yellow })
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        let widget = Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::raw(input.as_str()),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(border)),
        );
        frame.render_widget(widget, area);

        if focused {
            let cursor_x = area.x + 2 + input.cursor() as u16;
            let cursor_y = area.y + 1;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    fn draw_progress(&self, frame: &mut Frame, area: Rect, progress: Option<&IndexingProgress>) {
        match progress {
            Some(p) if p.total > 0 => {
                let ratio = (p.done as f64) / (p.total as f64);
                let label = format!(
                    " {}/{}  {}",
                    p.done,
                    p.total,
                    truncate(&p.current, area.width.saturating_sub(20) as usize),
                );
                let gauge = Gauge::default()
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Progress ")
                            .border_style(Style::default().fg(Color::Magenta)),
                    )
                    .gauge_style(Style::default().fg(Color::Magenta))
                    .ratio(ratio.clamp(0.0, 1.0))
                    .label(label);
                frame.render_widget(gauge, area);
            }
            _ => {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        " (no run in progress)",
                        Style::default().fg(Color::DarkGray),
                    )))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Progress ")
                            .border_style(Style::default().fg(Color::DarkGray)),
                    ),
                    area,
                );
            }
        }
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        if let Some(msg) = &self.footer_msg {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {msg}"),
                    Style::default().fg(Color::Green),
                ))),
                area,
            );
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SectionOutcome {
        self.footer_msg = None;

        // Tab: in Path → try path completion first; on no-op, cycle focus.
        if matches!(key.code, KeyCode::Tab) {
            if self.focus == Focus::Path
                && let Some(completed) = complete_path(&self.path.as_str())
            {
                self.path = TextInput::with_value(&completed);
                return SectionOutcome::Consumed;
            }
            self.focus = match self.focus {
                Focus::Path => Focus::Db,
                Focus::Db => Focus::Path,
            };
            return SectionOutcome::Consumed;
        }

        // Up/Down cycle focus unconditionally.
        if matches!(key.code, KeyCode::Up | KeyCode::Down) {
            self.focus = match self.focus {
                Focus::Path => Focus::Db,
                Focus::Db => Focus::Path,
            };
            return SectionOutcome::Consumed;
        }

        if matches!(key.code, KeyCode::Enter) {
            return self.submit();
        }

        let consumed = match self.focus {
            Focus::Path => self.path.handle_key(key),
            Focus::Db => self.db.handle_key(key),
        };
        if consumed {
            SectionOutcome::Consumed
        } else {
            SectionOutcome::Unhandled
        }
    }

    fn submit(&mut self) -> SectionOutcome {
        if self.submitting {
            self.footer_msg = Some("indexing already in progress…".to_string());
            return SectionOutcome::Consumed;
        }
        let path = self.path.as_str().trim().to_string();
        let db = self.db.as_str().trim().to_string();
        if path.is_empty() {
            self.footer_msg = Some("path is empty".to_string());
            self.focus = Focus::Path;
            return SectionOutcome::Consumed;
        }
        if db.is_empty() {
            self.footer_msg = Some("DB name is empty".to_string());
            self.focus = Focus::Db;
            return SectionOutcome::Consumed;
        }
        self.submitting = true;
        SectionOutcome::RunCommand(AppCommand::Index {
            path,
            db: Some(db),
        })
    }

    pub fn keyhints(&self) -> &'static str {
        "Enter start  ·  Tab complete/focus  ·  ↑↓ cycle fields  ·  Ctrl+1..5 sections"
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn ch(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }
    fn code(k: KeyCode) -> KeyEvent {
        KeyEvent::new(k, KeyModifiers::empty())
    }

    #[test]
    fn submit_empty_path_is_rejected() {
        let mut s = IndexSection::new();
        match s.handle_key(code(KeyCode::Enter)) {
            SectionOutcome::Consumed => {}
            _ => panic!("expected Consumed"),
        }
        assert!(s.footer_msg.is_some());
        assert_eq!(s.focus, Focus::Path);
    }

    #[test]
    fn up_down_cycles_focus() {
        let mut s = IndexSection::new();
        assert_eq!(s.focus, Focus::Path);
        s.handle_key(code(KeyCode::Down));
        assert_eq!(s.focus, Focus::Db);
        s.handle_key(code(KeyCode::Up));
        assert_eq!(s.focus, Focus::Path);
    }

    #[test]
    fn submit_with_both_fields_emits_index_command() {
        let mut s = IndexSection::new();
        s.handle_key(ch('/'));
        s.handle_key(ch('t'));
        s.handle_key(ch('m'));
        s.handle_key(ch('p'));
        s.handle_key(code(KeyCode::Down)); // focus Db
        s.handle_key(ch('d'));
        s.handle_key(ch('b'));
        match s.handle_key(code(KeyCode::Enter)) {
            SectionOutcome::RunCommand(AppCommand::Index { path, db }) => {
                assert_eq!(path, "/tmp");
                assert_eq!(db.as_deref(), Some("db"));
            }
            _ => panic!("expected RunCommand Index"),
        }
    }
}
