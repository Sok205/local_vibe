use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Minimal single-line text editor for use inside overlays (e.g. the picker's
/// DB-name field). The chat's `InputBuffer` is too coupled to its own action
/// enum to reuse directly, so we take a ~50 LOC duplication.
pub struct TextInput {
    buf: Vec<char>,
    cursor: usize,
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

impl TextInput {
    pub fn new() -> Self {
        Self { buf: Vec::new(), cursor: 0 }
    }

    pub fn with_value(value: &str) -> Self {
        let buf: Vec<char> = value.chars().collect();
        let cursor = buf.len();
        Self { buf, cursor }
    }

    pub fn as_str(&self) -> String {
        self.buf.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
    }

    /// Returns true if the key was consumed by the editor.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
                self.buf.insert(self.cursor, c);
                self.cursor += 1;
                true
            }
            (KeyCode::Backspace, _) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.buf.remove(self.cursor);
                }
                true
            }
            (KeyCode::Delete, _) => {
                if self.cursor < self.buf.len() {
                    self.buf.remove(self.cursor);
                }
                true
            }
            (KeyCode::Left, _) => {
                self.cursor = self.cursor.saturating_sub(1);
                true
            }
            (KeyCode::Right, _) => {
                if self.cursor < self.buf.len() {
                    self.cursor += 1;
                }
                true
            }
            (KeyCode::Home, _) => {
                self.cursor = 0;
                true
            }
            (KeyCode::End, _) => {
                self.cursor = self.buf.len();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }
    fn code(k: KeyCode) -> KeyEvent {
        KeyEvent::new(k, KeyModifiers::empty())
    }

    #[test]
    fn type_and_read() {
        let mut ti = TextInput::new();
        assert!(ti.handle_key(ch('h')));
        assert!(ti.handle_key(ch('i')));
        assert_eq!(ti.as_str(), "hi");
        assert_eq!(ti.cursor(), 2);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut ti = TextInput::new();
        assert!(ti.handle_key(code(KeyCode::Backspace)));
        assert_eq!(ti.as_str(), "");
    }

    #[test]
    fn left_right_clamp() {
        let mut ti = TextInput::with_value("ab");
        assert!(ti.handle_key(code(KeyCode::Left)));
        assert!(ti.handle_key(code(KeyCode::Left)));
        assert!(ti.handle_key(code(KeyCode::Left)));
        assert_eq!(ti.cursor(), 0);
        assert!(ti.handle_key(code(KeyCode::Right)));
        assert!(ti.handle_key(code(KeyCode::Right)));
        assert!(ti.handle_key(code(KeyCode::Right)));
        assert_eq!(ti.cursor(), 2);
    }

    #[test]
    fn insert_mid_string() {
        let mut ti = TextInput::with_value("ac");
        ti.handle_key(code(KeyCode::Left));
        ti.handle_key(ch('b'));
        assert_eq!(ti.as_str(), "abc");
    }
}
