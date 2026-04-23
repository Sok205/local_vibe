use crossterm::event::{KeyCode, KeyEvent};
use lv_core::types::ModelTier;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::AppCommand;
use crate::overlay::{centered, Overlay, OverlayAction};
use crate::widgets::selectable_list::{Item, KeyOutcome, SelectableList};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadState {
    Cold,
    Loading,
    Warm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotId {
    Chat(ModelTier),
    Embedding,
}

#[derive(Debug, Clone)]
pub struct ModelRow {
    pub slot: SlotId,
    pub name: String,
    pub backend: String,
    pub state: LoadState,
    pub active: bool,
}

pub struct ModelsOverlay {
    list: SelectableList<ModelRow>,
    footer_msg: Option<String>,
}

impl ModelsOverlay {
    pub fn new(rows: Vec<ModelRow>) -> Self {
        let items = rows.into_iter().map(Self::make_item).collect();
        Self {
            list: SelectableList::new(items),
            footer_msg: None,
        }
    }

    pub fn update(&mut self, rows: Vec<ModelRow>) {
        let items = rows.into_iter().map(Self::make_item).collect();
        self.list.replace_items(items);
    }

    pub fn set_footer(&mut self, msg: impl Into<String>) {
        self.footer_msg = Some(msg.into());
    }

    fn make_item(row: ModelRow) -> Item<ModelRow> {
        let label = match row.slot {
            SlotId::Chat(ModelTier::Fast) => "fast    ",
            SlotId::Chat(ModelTier::Medium) => "medium  ",
            SlotId::Chat(ModelTier::Strong) => "strong  ",
            SlotId::Chat(ModelTier::Cloud) => "cloud   ",
            SlotId::Embedding => "embed   ",
        };
        let (state_text, state_color) = match row.state {
            LoadState::Cold => ("cold", Color::Gray),
            LoadState::Loading => ("loading…", Color::Yellow),
            LoadState::Warm => ("warm", Color::Green),
        };
        let active_suffix = if row.active { "*" } else { "" };
        let active_style = if row.active {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let display = Line::from(vec![
            Span::styled(label, Style::default().fg(Color::Yellow)),
            Span::styled(format!("{:22}", row.name), active_style),
            Span::styled(
                format!("{:8}", row.backend),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(state_text.to_string(), Style::default().fg(state_color)),
            Span::styled(active_suffix.to_string(), Style::default().fg(Color::Cyan)),
        ]);
        Item::new(display, row)
    }

    fn selected(&self) -> Option<&ModelRow> {
        self.list.selected_meta()
    }
}

impl Overlay for ModelsOverlay {
    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let outer = centered(area, 60, 60);
        frame.render_widget(
            Paragraph::new("").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Models — Esc to close "),
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
            .constraints([Constraint::Min(3), Constraint::Length(3)])
            .split(inner);

        self.list
            .draw(frame, rows[0], Block::default().borders(Borders::NONE));

        let footer_line = if let Some(msg) = &self.footer_msg {
            Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Red),
            ))
        } else {
            Line::from(vec![
                Span::styled(
                    "↑/↓ · Enter: load+activate · u: unload · a: set active · Esc close",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        };
        frame.render_widget(
            Paragraph::new(footer_line).block(Block::default().borders(Borders::TOP)),
            rows[1],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> OverlayAction {
        self.footer_msg = None;
        let outcome = self.list.handle_key(key);
        match outcome {
            KeyOutcome::Consumed | KeyOutcome::Unhandled => OverlayAction::None,
            KeyOutcome::Escape => OverlayAction::Dismiss,
            KeyOutcome::Activate(_) => {
                let row = self.selected().cloned();
                handle_enter(row.as_ref())
            }
            KeyOutcome::Key(k) => match k.code {
                KeyCode::Char('u') => {
                    let row = self.selected().cloned();
                    let (action, footer) = handle_unload(row.as_ref());
                    if let Some(f) = footer {
                        self.footer_msg = Some(f);
                    }
                    action
                }
                KeyCode::Char('a') => {
                    let row = self.selected().cloned();
                    let (action, footer) = handle_set_active(row.as_ref());
                    if let Some(f) = footer {
                        self.footer_msg = Some(f);
                    }
                    action
                }
                _ => OverlayAction::None,
            },
        }
    }
}

fn handle_enter(selected: Option<&ModelRow>) -> OverlayAction {
    let Some(row) = selected else {
        return OverlayAction::None;
    };
    match row.slot {
        SlotId::Chat(tier) => match row.state {
            LoadState::Cold => OverlayAction::RunCommand(AppCommand::LoadAndActivate(tier)),
            LoadState::Warm => OverlayAction::RunCommand(AppCommand::SetActiveTier(tier)),
            LoadState::Loading => OverlayAction::None,
        },
        SlotId::Embedding => OverlayAction::None,
    }
}

fn handle_unload(selected: Option<&ModelRow>) -> (OverlayAction, Option<String>) {
    let Some(row) = selected else {
        return (OverlayAction::None, None);
    };
    if matches!(row.state, LoadState::Cold) {
        return (OverlayAction::None, None);
    }
    match row.slot {
        SlotId::Chat(tier) => {
            if row.active {
                return (
                    OverlayAction::None,
                    Some("can't unload active tier — press `a` on another warm tier first".to_string()),
                );
            }
            (OverlayAction::RunCommand(AppCommand::UnloadModel(tier)), None)
        }
        SlotId::Embedding => (
            OverlayAction::None,
            Some("unloading embedding isn't supported in v1".to_string()),
        ),
    }
}

fn handle_set_active(selected: Option<&ModelRow>) -> (OverlayAction, Option<String>) {
    let Some(row) = selected else {
        return (OverlayAction::None, None);
    };
    match row.slot {
        SlotId::Chat(tier) => match row.state {
            LoadState::Warm => (OverlayAction::RunCommand(AppCommand::SetActiveTier(tier)), None),
            LoadState::Cold => (
                OverlayAction::None,
                Some("load this tier first (Enter)".to_string()),
            ),
            LoadState::Loading => (OverlayAction::None, None),
        },
        SlotId::Embedding => (OverlayAction::None, None),
    }
}
