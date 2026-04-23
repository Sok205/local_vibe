use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

use crate::commands::{filter_commands, CommandSpec};

/// Popup rendered above the input frame when the user is typing a slash
/// command. All state is derived from the current input text on each draw —
/// the only persistent state is the selected row index.
pub struct CommandPalette {
    selected: usize,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    pub fn visible(&self, input: &str) -> bool {
        crate::commands::is_palette_prefix(input)
    }

    pub fn matches(&self, input: &str) -> Vec<&'static CommandSpec> {
        filter_commands(input)
    }

    /// Clamp the selected index against the current filter and return the
    /// matches so the caller can act on them.
    pub fn resolve(&mut self, input: &str) -> Vec<&'static CommandSpec> {
        let matches = self.matches(input);
        if matches.is_empty() {
            self.selected = 0;
        } else if self.selected >= matches.len() {
            self.selected = matches.len() - 1;
        }
        matches
    }

    pub fn selected_spec(&mut self, input: &str) -> Option<&'static CommandSpec> {
        let matches = self.resolve(input);
        matches.get(self.selected).copied()
    }

    pub fn move_up(&mut self, input: &str) {
        let matches = self.resolve(input);
        if matches.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = matches.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self, input: &str) {
        let matches = self.resolve(input);
        if matches.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % matches.len();
    }

    pub fn reset_selection(&mut self) {
        self.selected = 0;
    }

    /// Draw the palette above the input. `anchor` is the *input frame's* area;
    /// the palette hovers immediately above it, left-aligned to the same
    /// left edge.
    pub fn draw(&mut self, frame: &mut Frame, anchor: Rect, input: &str) {
        let matches = self.resolve(input);
        if matches.is_empty() {
            return;
        }

        let max_rows = 8usize;
        let height = (matches.len().min(max_rows) + 2) as u16;
        let width = anchor.width.min(70);
        let y = anchor.y.saturating_sub(height);
        let x = anchor.x;
        let area = Rect { x, y, width, height };

        let name_w = matches
            .iter()
            .map(|c| c.name.len())
            .max()
            .unwrap_or(0)
            + 2;

        let items: Vec<ListItem> = matches
            .iter()
            .map(|c| {
                let mut name = format!("  {:<width$}", c.name, width = name_w);
                if c.takes_args {
                    name.push_str("<arg> ");
                } else {
                    name.push_str("      ");
                }
                ListItem::new(Line::from(vec![
                    Span::styled(
                        name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(c.description, Style::default().fg(Color::Gray)),
                ]))
            })
            .collect();

        let mut state = ListState::default();
        state.select(Some(self.selected.min(matches.len() - 1)));

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Commands "),
            )
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
            .highlight_symbol("▸ ");

        frame.render_stateful_widget(list, area, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_only_for_palette_prefix() {
        let p = CommandPalette::new();
        assert!(p.visible("/"));
        assert!(p.visible("/s"));
        assert!(!p.visible("hello"));
        assert!(!p.visible("/index foo"));
    }

    #[test]
    fn move_down_wraps() {
        let mut p = CommandPalette::new();
        let input = "/d";
        let matches = p.resolve(input);
        assert!(matches.len() >= 2); // /db and /dbs at least
        p.move_down(input);
        assert_eq!(p.selected, 1);
        // wrap around
        for _ in 0..matches.len() {
            p.move_down(input);
        }
        assert_eq!(p.selected, 1);
    }

    #[test]
    fn empty_matches_clamps_selected() {
        let mut p = CommandPalette::new();
        p.selected = 99;
        let matches = p.resolve("/zzzzz");
        assert!(matches.is_empty());
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn selected_spec_tracks_selection() {
        let mut p = CommandPalette::new();
        let input = "/d";
        p.move_down(input);
        let spec = p.selected_spec(input).unwrap();
        assert!(spec.name == "/dbs" || spec.name == "/db");
    }
}
