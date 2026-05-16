use crossterm::event::{KeyCode, KeyEvent};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, List, ListItem, ListState},
};

/// A selectable item with a renderable display line plus typed metadata.
pub struct Item<T> {
    pub display: Line<'static>,
    pub plain: String,
    pub meta: T,
}

impl<T> Item<T> {
    pub fn new(display: Line<'static>, meta: T) -> Self {
        let plain = display
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        Self {
            display,
            plain,
            meta,
        }
    }
}

/// Generic selectable list with an optional fuzzy filter.
///
/// Keybinds (via `handle_key`):
/// - `↑/↓`, `j/k`: move selection (wrap at boundaries)
/// - `g` / `G`: jump to first / last
/// - `/`: enter filter mode (caller should render the filter string)
/// - typing while in filter mode: updates the filter
/// - `Esc` in filter mode: clears filter + exits filter mode (returns `KeyOutcome::Consumed`)
/// - `Esc` outside filter mode: returns `KeyOutcome::Escape` — container decides what to do
/// - `Enter` outside filter mode: returns `KeyOutcome::Activate(index_in_items)`
pub struct SelectableList<T> {
    items: Vec<Item<T>>,
    filtered: Vec<usize>,
    state: ListState,
    filter: Option<String>,
    matcher: Matcher,
}

pub enum KeyOutcome {
    Consumed,
    Unhandled,
    Escape,
    Activate(usize),
    Key(KeyEvent),
}

impl<T> SelectableList<T> {
    pub fn new(items: Vec<Item<T>>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        let filtered = (0..items.len()).collect();
        Self {
            items,
            filtered,
            state,
            filter: None,
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    pub fn items(&self) -> &[Item<T>] {
        &self.items
    }

    pub fn selected_meta(&self) -> Option<&T> {
        let idx = self.selected_item_index()?;
        self.items.get(idx).map(|i| &i.meta)
    }

    pub fn selected_item_index(&self) -> Option<usize> {
        let vi = self.state.selected()?;
        self.filtered.get(vi).copied()
    }

    pub fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    pub fn in_filter_mode(&self) -> bool {
        self.filter.is_some()
    }

    pub fn replace_items(&mut self, items: Vec<Item<T>>) {
        self.items = items;
        self.recompute_filter();
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, block: Block<'_>) {
        let list_items: Vec<ListItem> = self
            .filtered
            .iter()
            .filter_map(|i| self.items.get(*i))
            .map(|it| ListItem::new(it.display.clone()))
            .collect();
        let list = List::new(list_items)
            .block(block)
            .highlight_style(Style::default().fg(Color::Cyan))
            .highlight_symbol("▸ ");
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        if let Some(filter) = self.filter.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    self.filter = None;
                    self.recompute_filter();
                    return KeyOutcome::Consumed;
                }
                KeyCode::Enter => {
                    self.filter = None;
                    self.recompute_filter();
                    if let Some(idx) = self.selected_item_index() {
                        return KeyOutcome::Activate(idx);
                    }
                    return KeyOutcome::Consumed;
                }
                KeyCode::Backspace => {
                    filter.pop();
                    self.recompute_filter();
                    return KeyOutcome::Consumed;
                }
                KeyCode::Char(c) => {
                    filter.push(c);
                    self.recompute_filter();
                    return KeyOutcome::Consumed;
                }
                KeyCode::Up => {
                    self.move_selection(-1);
                    return KeyOutcome::Consumed;
                }
                KeyCode::Down => {
                    self.move_selection(1);
                    return KeyOutcome::Consumed;
                }
                _ => return KeyOutcome::Consumed,
            }
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                KeyOutcome::Consumed
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                KeyOutcome::Consumed
            }
            KeyCode::Char('g') => {
                if !self.filtered.is_empty() {
                    self.state.select(Some(0));
                }
                KeyOutcome::Consumed
            }
            KeyCode::Char('G') => {
                if !self.filtered.is_empty() {
                    self.state.select(Some(self.filtered.len() - 1));
                }
                KeyOutcome::Consumed
            }
            KeyCode::Char('/') => {
                self.filter = Some(String::new());
                KeyOutcome::Consumed
            }
            KeyCode::Enter => match self.selected_item_index() {
                Some(idx) => KeyOutcome::Activate(idx),
                None => KeyOutcome::Consumed,
            },
            KeyCode::Esc => KeyOutcome::Escape,
            _ => KeyOutcome::Key(key),
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            self.state.select(None);
            return;
        }
        let len = self.filtered.len() as i32;
        let cur = self.state.selected().unwrap_or(0) as i32;
        let next = ((cur + delta).rem_euclid(len)) as usize;
        self.state.select(Some(next));
    }

    fn recompute_filter(&mut self) {
        let previous_item_idx = self.selected_item_index();
        match self.filter.as_deref() {
            None | Some("") => {
                self.filtered = (0..self.items.len()).collect();
            }
            Some(needle) => {
                let mut haystack_buf = Vec::new();
                let mut needle_buf = Vec::new();
                let needle_utf32 = Utf32Str::new(needle, &mut needle_buf);
                let mut scored: Vec<(usize, u16)> = Vec::new();
                for (i, it) in self.items.iter().enumerate() {
                    haystack_buf.clear();
                    let hay = Utf32Str::new(&it.plain, &mut haystack_buf);
                    if let Some(score) = self.matcher.fuzzy_match(hay, needle_utf32) {
                        scored.push((i, score));
                    }
                }
                scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
                self.filtered = scored.into_iter().map(|(i, _)| i).collect();
            }
        }
        if self.filtered.is_empty() {
            self.state.select(None);
        } else {
            let new_idx = previous_item_idx
                .and_then(|old| self.filtered.iter().position(|i| *i == old))
                .unwrap_or(0);
            self.state.select(Some(new_idx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use ratatui::text::Line;

    fn items(names: &[&str]) -> Vec<Item<String>> {
        names
            .iter()
            .map(|n| Item::new(Line::from(n.to_string()), n.to_string()))
            .collect()
    }
    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }
    fn code(k: KeyCode) -> KeyEvent {
        KeyEvent::new(k, KeyModifiers::empty())
    }

    #[test]
    fn navigation_wraps() {
        let mut list = SelectableList::new(items(&["a", "b", "c"]));
        assert_eq!(list.selected_meta().unwrap(), "a");
        let _ = list.handle_key(code(KeyCode::Down));
        assert_eq!(list.selected_meta().unwrap(), "b");
        let _ = list.handle_key(code(KeyCode::Down));
        let _ = list.handle_key(code(KeyCode::Down));
        assert_eq!(list.selected_meta().unwrap(), "a");
        let _ = list.handle_key(code(KeyCode::Up));
        assert_eq!(list.selected_meta().unwrap(), "c");
    }

    #[test]
    fn g_and_shift_g_jump_ends() {
        let mut list = SelectableList::new(items(&["a", "b", "c"]));
        let _ = list.handle_key(key('G'));
        assert_eq!(list.selected_meta().unwrap(), "c");
        let _ = list.handle_key(key('g'));
        assert_eq!(list.selected_meta().unwrap(), "a");
    }

    #[test]
    fn filter_narrows_and_clears() {
        let mut list = SelectableList::new(items(&["alpha", "beta", "gamma", "delta"]));
        let _ = list.handle_key(key('/'));
        assert!(list.in_filter_mode());
        let _ = list.handle_key(key('l'));
        let _ = list.handle_key(key('t'));
        // "delta" has l then t in order; nothing else matches that subsequence
        let filtered = list.filtered.clone();
        let plains: Vec<String> = filtered
            .iter()
            .map(|i| list.items[*i].plain.clone())
            .collect();
        assert!(plains.contains(&"delta".to_string()));
        assert!(!plains.contains(&"alpha".to_string()));
        assert!(!plains.contains(&"gamma".to_string()));
        assert!(!plains.contains(&"beta".to_string()));
        let _ = list.handle_key(code(KeyCode::Esc));
        assert!(!list.in_filter_mode());
        assert_eq!(list.filtered.len(), 4);
    }

    #[test]
    fn enter_outside_filter_returns_activate() {
        let mut list = SelectableList::new(items(&["a", "b"]));
        let _ = list.handle_key(code(KeyCode::Down));
        match list.handle_key(code(KeyCode::Enter)) {
            KeyOutcome::Activate(i) => assert_eq!(list.items[i].plain, "b"),
            _ => panic!("expected Activate"),
        }
    }

    #[test]
    fn esc_outside_filter_escapes() {
        let mut list = SelectableList::<String>::new(items(&["a"]));
        match list.handle_key(code(KeyCode::Esc)) {
            KeyOutcome::Escape => {}
            _ => panic!("expected Escape"),
        }
    }
}
