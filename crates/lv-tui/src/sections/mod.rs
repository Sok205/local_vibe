pub mod chat;
pub mod databases;
pub mod models;
pub mod placeholder;

use crate::app::AppCommand;

/// A top-level section in the left sidebar. Ordering matches the `[1]..[5]`
/// hotkeys rendered in the sidebar.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Section {
    Chat,
    Models,
    Databases,
    Index,
    Settings,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Chat,
        Section::Models,
        Section::Databases,
        Section::Index,
        Section::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Chat => "Chat",
            Section::Models => "Models",
            Section::Databases => "Databases",
            Section::Index => "Index",
            Section::Settings => "Settings",
        }
    }

    pub fn hotkey(self) -> char {
        match self {
            Section::Chat => '1',
            Section::Models => '2',
            Section::Databases => '3',
            Section::Index => '4',
            Section::Settings => '5',
        }
    }

    pub fn from_digit(c: char) -> Option<Self> {
        match c {
            '1' => Some(Section::Chat),
            '2' => Some(Section::Models),
            '3' => Some(Section::Databases),
            '4' => Some(Section::Index),
            '5' => Some(Section::Settings),
            _ => None,
        }
    }
}

/// What a section reports after handling a key.
pub enum SectionOutcome {
    /// Key consumed; app should do nothing further.
    Consumed,
    /// Section didn't use this key — let app handle globals (like `?`).
    Unhandled,
    /// User submitted text (Chat only). App wraps into `AppCommand::Ask`.
    Submit(String),
    /// Section wants the app to run a command.
    RunCommand(AppCommand),
}
