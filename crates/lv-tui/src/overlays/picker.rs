use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::AppCommand;
use crate::overlay::{centered, Overlay, OverlayAction};
use crate::widgets::path_tree::{EntryKind, PathEntry, PathTree};
use crate::widgets::selectable_list::KeyOutcome;
use crate::widgets::text_input::TextInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Tree,
    NameField,
}

pub struct PickerOverlay {
    tree: PathTree,
    name: TextInput,
    focus: Focus,
    footer_msg: Option<String>,
}

impl PickerOverlay {
    pub fn new(start: &std::path::Path) -> std::io::Result<Self> {
        Ok(Self {
            tree: PathTree::new(start)?,
            name: TextInput::new(),
            focus: Focus::Tree,
            footer_msg: None,
        })
    }

    fn commit_path(&self) -> PathBuf {
        match self.tree.selected() {
            Some(PathEntry { kind: EntryKind::Dir(p), .. }) => p.clone(),
            _ => self.tree.current().to_path_buf(),
        }
    }

    fn build_commit_command(&self) -> AppCommand {
        let path = self.commit_path().to_string_lossy().into_owned();
        let db_raw = self.name.as_str();
        let db_trim = db_raw.trim();
        let db = if db_trim.is_empty() { None } else { Some(db_trim.to_string()) };
        AppCommand::Index { path, db }
    }
}

impl Overlay for PickerOverlay {
    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let outer = centered(area, 65, 75);
        frame.render_widget(
            Paragraph::new("").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Index directory — Esc to close "),
            ),
            outer,
        );

        let inner = Rect {
            x: outer.x + 1,
            y: outer.y + 1,
            width: outer.width.saturating_sub(2),
            height: outer.height.saturating_sub(2),
        };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(3),
                Constraint::Length(4),
            ])
            .split(inner);

        // Current path header
        let path_line = Line::from(vec![
            Span::styled("path: ", Style::default().fg(Color::Yellow)),
            Span::raw(self.tree.current().to_string_lossy().into_owned()),
        ]);
        frame.render_widget(
            Paragraph::new(path_line).block(Block::default().borders(Borders::BOTTOM)),
            rows[0],
        );

        // Tree
        let tree_block = Block::default().borders(Borders::NONE);
        self.tree.draw(frame, rows[1], tree_block);

        // Footer: name field + hints
        let footer_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
            .split(rows[2]);

        let name_text = self.name.as_str();
        let name_style = if self.focus == Focus::NameField {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let name_line = Line::from(vec![
            Span::styled("DB name: ", Style::default().fg(Color::Yellow)),
            Span::styled(name_text.clone(), name_style),
            if self.focus == Focus::NameField {
                Span::styled("▏", Style::default().fg(Color::Cyan))
            } else {
                Span::raw("")
            },
            if name_text.is_empty() {
                Span::styled("  (blank = default)", Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ]);
        frame.render_widget(Paragraph::new(name_line), footer_rows[0]);

        let hint = if let Some(msg) = &self.footer_msg {
            Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Red)))
        } else if self.focus == Focus::Tree {
            Line::from(Span::styled(
                "Enter: descend · i: index selected · Tab: name field · Esc close",
                Style::default().fg(Color::DarkGray),
            ))
        } else {
            Line::from(Span::styled(
                "Enter: go · Tab: back to tree · Esc close",
                Style::default().fg(Color::DarkGray),
            ))
        };
        frame.render_widget(Paragraph::new(hint), footer_rows[1]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> OverlayAction {
        self.footer_msg = None;
        match self.focus {
            Focus::Tree => self.handle_key_tree(key),
            Focus::NameField => self.handle_key_name(key),
        }
    }
}

impl PickerOverlay {
    fn handle_key_tree(&mut self, key: KeyEvent) -> OverlayAction {
        // Tab toggles focus before the list sees the key.
        if matches!(key.code, KeyCode::Tab) {
            self.focus = Focus::NameField;
            return OverlayAction::None;
        }
        if matches!(key.code, KeyCode::Char('i')) {
            return OverlayAction::RunCommand(self.build_commit_command());
        }
        let outcome = self.tree.list_mut().handle_key(key);
        match outcome {
            KeyOutcome::Consumed | KeyOutcome::Unhandled => OverlayAction::None,
            KeyOutcome::Escape => OverlayAction::Dismiss,
            KeyOutcome::Activate(_) => {
                if let Err(e) = self.tree.activate() {
                    self.footer_msg = Some(format!("navigate: {e}"));
                }
                OverlayAction::None
            }
            KeyOutcome::Key(_) => OverlayAction::None,
        }
    }

    fn handle_key_name(&mut self, key: KeyEvent) -> OverlayAction {
        match key.code {
            KeyCode::Tab => {
                self.focus = Focus::Tree;
                OverlayAction::None
            }
            KeyCode::Esc => OverlayAction::Dismiss,
            KeyCode::Enter => OverlayAction::RunCommand(self.build_commit_command()),
            _ => {
                self.name.handle_key(key);
                OverlayAction::None
            }
        }
    }
}
