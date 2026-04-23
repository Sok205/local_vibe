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
use crate::widgets::selectable_list::{Item, KeyOutcome, SelectableList};

use super::SectionOutcome;

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

/// Models section: one row per slot (chat tiers + embedding). Shows load
/// state, active tier, and lets the user load / unload / switch active.
pub struct ModelsSection {
    list: SelectableList<ModelRow>,
    footer_msg: Option<String>,
    loaded_once: bool,
}

impl Default for ModelsSection {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelsSection {
    pub fn new() -> Self {
        Self {
            list: SelectableList::new(Vec::new()),
            footer_msg: None,
            loaded_once: false,
        }
    }

    pub fn update(&mut self, rows: Vec<ModelRow>) {
        let items = rows.into_iter().map(Self::make_item).collect();
        self.list.replace_items(items);
        self.loaded_once = true;
    }

    pub fn needs_initial_load(&self) -> bool {
        !self.loaded_once
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
        let active_suffix = if row.active { "*" } else { " " };
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
            Span::raw("  "),
            Span::styled(active_suffix.to_string(), Style::default().fg(Color::Cyan)),
        ]);
        Item::new(display, row)
    }

    fn selected(&self) -> Option<&ModelRow> {
        self.list.selected_meta()
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)])
            .split(area);

        self.list.draw(
            frame,
            rows[0],
            Block::default()
                .borders(Borders::ALL)
                .title(" Models ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        let footer_line = if let Some(msg) = &self.footer_msg {
            Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Red),
            ))
        } else if self.list.items().is_empty() {
            Line::from(Span::styled(
                " loading…",
                Style::default().fg(Color::DarkGray),
            ))
        } else {
            Line::raw("")
        };
        frame.render_widget(Paragraph::new(footer_line), rows[1]);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SectionOutcome {
        self.footer_msg = None;
        let outcome = self.list.handle_key(key);
        match outcome {
            KeyOutcome::Consumed => SectionOutcome::Consumed,
            KeyOutcome::Unhandled => SectionOutcome::Unhandled,
            KeyOutcome::Escape => SectionOutcome::Consumed,
            KeyOutcome::Activate(_) => {
                let row = self.selected().cloned();
                match enter_cmd(row.as_ref()) {
                    Some(cmd) => SectionOutcome::RunCommand(cmd),
                    None => SectionOutcome::Consumed,
                }
            }
            KeyOutcome::Key(k) => match k.code {
                KeyCode::Char('l') => {
                    let row = self.selected().cloned();
                    match load_cmd(row.as_ref()) {
                        Ok(Some(cmd)) => SectionOutcome::RunCommand(cmd),
                        Ok(None) => SectionOutcome::Consumed,
                        Err(msg) => {
                            self.footer_msg = Some(msg);
                            SectionOutcome::Consumed
                        }
                    }
                }
                KeyCode::Char('u') => {
                    let row = self.selected().cloned();
                    match unload_cmd(row.as_ref()) {
                        Ok(Some(cmd)) => SectionOutcome::RunCommand(cmd),
                        Ok(None) => SectionOutcome::Consumed,
                        Err(msg) => {
                            self.footer_msg = Some(msg);
                            SectionOutcome::Consumed
                        }
                    }
                }
                KeyCode::Char('a') => {
                    let row = self.selected().cloned();
                    match set_active_cmd(row.as_ref()) {
                        Ok(Some(cmd)) => SectionOutcome::RunCommand(cmd),
                        Ok(None) => SectionOutcome::Consumed,
                        Err(msg) => {
                            self.footer_msg = Some(msg);
                            SectionOutcome::Consumed
                        }
                    }
                }
                _ => SectionOutcome::Unhandled,
            },
        }
    }

    pub fn keyhints(&self) -> &'static str {
        "↑↓ select  ·  Enter load+activate  ·  l load  ·  u unload  ·  a set active  ·  F1..F5 sections"
    }
}

fn enter_cmd(row: Option<&ModelRow>) -> Option<AppCommand> {
    let row = row?;
    match row.slot {
        SlotId::Chat(tier) => match row.state {
            LoadState::Cold => Some(AppCommand::LoadAndActivate(tier)),
            LoadState::Warm => Some(AppCommand::SetActiveTier(tier)),
            LoadState::Loading => None,
        },
        SlotId::Embedding => None,
    }
}

fn load_cmd(row: Option<&ModelRow>) -> Result<Option<AppCommand>, String> {
    let Some(row) = row else {
        return Ok(None);
    };
    if matches!(row.state, LoadState::Warm) {
        return Err("already warm".to_string());
    }
    match row.slot {
        SlotId::Chat(tier) => Ok(Some(AppCommand::LoadModel(tier))),
        SlotId::Embedding => Err("embedding loads on first use".to_string()),
    }
}

fn unload_cmd(row: Option<&ModelRow>) -> Result<Option<AppCommand>, String> {
    let Some(row) = row else {
        return Ok(None);
    };
    if matches!(row.state, LoadState::Cold) {
        return Ok(None);
    }
    match row.slot {
        SlotId::Chat(tier) => {
            if row.active {
                Err("can't unload active tier — press `a` on another warm tier first".to_string())
            } else {
                Ok(Some(AppCommand::UnloadModel(tier)))
            }
        }
        SlotId::Embedding => Err("unloading embedding isn't supported".to_string()),
    }
}

fn set_active_cmd(row: Option<&ModelRow>) -> Result<Option<AppCommand>, String> {
    let Some(row) = row else {
        return Ok(None);
    };
    match row.slot {
        SlotId::Chat(tier) => match row.state {
            LoadState::Warm => Ok(Some(AppCommand::SetActiveTier(tier))),
            LoadState::Cold => Err("load this tier first (Enter)".to_string()),
            LoadState::Loading => Ok(None),
        },
        SlotId::Embedding => Err("embedding has no active-tier semantics".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_cold_loads_and_activates() {
        let row = ModelRow {
            slot: SlotId::Chat(ModelTier::Fast),
            name: "x".into(),
            backend: "metal".into(),
            state: LoadState::Cold,
            active: false,
        };
        assert!(matches!(
            enter_cmd(Some(&row)),
            Some(AppCommand::LoadAndActivate(ModelTier::Fast))
        ));
    }

    #[test]
    fn enter_warm_just_switches_active() {
        let row = ModelRow {
            slot: SlotId::Chat(ModelTier::Medium),
            name: "x".into(),
            backend: "metal".into(),
            state: LoadState::Warm,
            active: false,
        };
        assert!(matches!(
            enter_cmd(Some(&row)),
            Some(AppCommand::SetActiveTier(ModelTier::Medium))
        ));
    }

    #[test]
    fn unload_active_rejects() {
        let row = ModelRow {
            slot: SlotId::Chat(ModelTier::Fast),
            name: "x".into(),
            backend: "metal".into(),
            state: LoadState::Warm,
            active: true,
        };
        assert!(unload_cmd(Some(&row)).is_err());
    }
}
