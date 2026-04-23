use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum InputAction {
    Submit(String),
    Quit,
    ScrollUp,
    ScrollDown,
    ToggleContext,
    None,
}

pub struct InputBuffer {
    pub buf: Vec<char>,
    pub cursor: usize,
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl InputBuffer {
    pub fn new() -> Self {
        Self { buf: Vec::new(), cursor: 0 }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> InputAction {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => InputAction::Quit,
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => InputAction::Quit,
            (KeyCode::Enter, _) => {
                let text: String = self.buf.iter().collect();
                self.buf.clear();
                self.cursor = 0;
                if text.trim().is_empty() {
                    InputAction::None
                } else {
                    InputAction::Submit(text)
                }
            }
            (KeyCode::Backspace, _) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.buf.remove(self.cursor);
                }
                InputAction::None
            }
            (KeyCode::Left, _) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                InputAction::None
            }
            (KeyCode::Right, _) => {
                if self.cursor < self.buf.len() {
                    self.cursor += 1;
                }
                InputAction::None
            }
            (KeyCode::Up, _) => InputAction::ScrollUp,
            (KeyCode::Down, _) => InputAction::ScrollDown,
            (KeyCode::Tab, _) => InputAction::ToggleContext,
            (KeyCode::Char(c), _) => {
                self.buf.insert(self.cursor, c);
                self.cursor += 1;
                InputAction::None
            }
            _ => InputAction::None,
        }
    }

    pub fn as_str(&self) -> String {
        self.buf.iter().collect()
    }

    /// Overwrite the buffer with `s`, putting the cursor at the end.
    pub fn set_from(&mut self, s: &str) {
        self.buf = s.chars().collect();
        self.cursor = self.buf.len();
    }
}
