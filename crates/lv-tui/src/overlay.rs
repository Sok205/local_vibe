use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::AppCommand;

/// What the caller should do after handing a key event to an overlay.
pub enum OverlayAction {
    /// Key consumed, overlay stays.
    None,
    /// Close this overlay.
    Dismiss,
    /// Dismiss and forward a command to the CLI handler task.
    RunCommand(AppCommand),
}

/// Any screen that takes over the whole TUI (centered popup) and handles its
/// own input. Only one overlay is open at a time in v1.
pub trait Overlay: Send {
    fn draw(&mut self, frame: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: KeyEvent) -> OverlayAction;
}

/// Centered rectangle at `width_pct` x `height_pct` of `area`, used by every
/// overlay so they share a consistent shape.
pub fn centered(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
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
