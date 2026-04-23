use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use lv_core::status::StatusSnapshot;
use lv_core::types::{FileSummary, Message, ModelTier, Role, SearchResult, StoreStats};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use tokio::sync::mpsc;
use tracing::error;

use crate::{
    overlay::{Overlay, OverlayAction},
    overlays::HelpOverlay,
    sections::{
        Section, SectionOutcome, chat::ChatSection, databases::DatabasesSection,
        index::IndexSection, models::ModelsSection, settings::SettingsSection,
    },
    status_bar::{StatusBarView, draw_status_bar},
    widgets::sidebar::draw_sidebar,
};

pub enum AppEvent {
    StreamToken(String),
    StreamDone,
    SearchResults(Vec<SearchResult>),
    RepoMap(String),
    ModelChanged(ModelTier, String),
    StoreStats(StoreStats),
    Error(String),
    IndexProgress { done: usize, total: usize, current: String },
    IndexDone { indexed: usize, skipped: usize, failed: usize },
    DbListing(Vec<String>),
    DbSwitched(String),
    Status(Box<StatusSnapshot>),
    ModelsSnapshot(Vec<crate::overlays::ModelRow>),
    ModelLoading(ModelTier),
    ModelLoaded(ModelTier),
    ModelLoadFailed(ModelTier, String),
    ModelUnloaded(ModelTier),
    ActiveTierChanged(ModelTier, String),
    WarmCountChanged(usize, bool),
    BrowseData { db: String, files: Vec<FileSummary>, total_chunks: usize },
}

pub enum AppCommand {
    Ask { query: String },
    Index { path: String, db: Option<String> },
    ListDbs,
    SwitchDb(String),
    Status,
    Models,
    Browse(String),
    OpenPicker,
    LoadAndActivate(ModelTier),
    LoadModel(ModelTier),
    UnloadModel(ModelTier),
    SetActiveTier(ModelTier),
    Help,
    Quit,
}

/// Parse one submitted chat line. In TUI 3.0 the slash palette is gone — the
/// only command recognised at the prompt is `/quit`; anything else is asked
/// of the model. Non-Chat actions live inside their sections.
pub fn parse_input(line: &str) -> AppCommand {
    let trimmed = line.trim();
    if trimmed == "/quit" {
        return AppCommand::Quit;
    }
    AppCommand::Ask {
        query: line.to_string(),
    }
}

pub struct IndexingProgress {
    pub done: usize,
    pub total: usize,
    pub current: String,
}

struct AppState {
    active_section: Section,
    chat_section: ChatSection,
    models_section: ModelsSection,
    databases_section: DatabasesSection,
    index_section: IndexSection,
    settings_section: SettingsSection,
    model_tier: ModelTier,
    model_name: String,
    store_stats: Option<StoreStats>,
    indexing: Option<IndexingProgress>,
    current_db: String,
    overlay: Option<Box<dyn Overlay>>,
    warm_count: usize,
    active_loading: bool,
}

impl AppState {
    fn new() -> Self {
        Self {
            active_section: Section::Chat,
            chat_section: ChatSection::new(),
            models_section: ModelsSection::new(),
            databases_section: DatabasesSection::new(),
            index_section: IndexSection::new(),
            settings_section: SettingsSection::new(),
            model_tier: ModelTier::Fast,
            model_name: "unknown".to_string(),
            store_stats: None,
            indexing: None,
            current_db: "default".to_string(),
            overlay: None,
            warm_count: 0,
            active_loading: false,
        }
    }

    fn draw_section(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        match self.active_section {
            Section::Chat => self.chat_section.draw(frame, area),
            Section::Models => self.models_section.draw(frame, area),
            Section::Databases => self.databases_section.draw(frame, area),
            Section::Index => self.index_section.draw(frame, area, self.indexing.as_ref()),
            Section::Settings => self.settings_section.draw(frame, area),
        }
    }

    fn section_keyhints(&self) -> &'static str {
        match self.active_section {
            Section::Chat => self.chat_section.keyhints(),
            Section::Models => self.models_section.keyhints(),
            Section::Databases => self.databases_section.keyhints(),
            Section::Index => self.index_section.keyhints(),
            Section::Settings => self.settings_section.keyhints(),
        }
    }

    fn dispatch_key(&mut self, key: crossterm::event::KeyEvent) -> SectionOutcome {
        match self.active_section {
            Section::Chat => self.chat_section.handle_key(key),
            Section::Models => self.models_section.handle_key(key),
            Section::Databases => self.databases_section.handle_key(key),
            Section::Index => self.index_section.handle_key(key),
            Section::Settings => self.settings_section.handle_key(key),
        }
    }
}

pub async fn run_tui(
    mut event_rx: mpsc::Receiver<AppEvent>,
    command_tx: mpsc::Sender<AppCommand>,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new();
    let poll_interval = Duration::from_millis(50);

    loop {
        while let Ok(ev) = event_rx.try_recv() {
            handle_app_event(ev, &mut state);
        }

        terminal.draw(|frame| {
            let size = frame.area();
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(0),
                    Constraint::Length(1),
                ])
                .split(size);

            draw_status_bar(
                frame,
                rows[0],
                StatusBarView {
                    active_tier: state.model_tier,
                    model_name: &state.model_name,
                    stats: state.store_stats.as_ref(),
                    current_db: &state.current_db,
                    warm_count: state.warm_count,
                    active_loading: state.active_loading,
                    indexing: state.indexing.as_ref(),
                },
            );

            let main_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(16), Constraint::Min(0)])
                .split(rows[1]);

            draw_sidebar(frame, main_cols[0], state.active_section);
            state.draw_section(frame, main_cols[1]);

            draw_hint_line(frame, rows[2], state.section_keyhints());

            if let Some(overlay) = state.overlay.as_mut() {
                overlay.draw(frame, size);
            }
        })?;

        if event::poll(poll_interval)? && let Event::Key(key) = event::read()? {
            // Overlay takes priority.
            if let Some(overlay) = state.overlay.as_mut() {
                match overlay.handle_key(key) {
                    OverlayAction::None => {}
                    OverlayAction::Dismiss => state.overlay = None,
                    OverlayAction::RunCommand(cmd) => {
                        state.overlay = None;
                        let _ = command_tx.send(cmd).await;
                    }
                }
                continue;
            }

            // Global quit.
            if matches!(key.modifiers, KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q'))
            {
                let _ = command_tx.send(AppCommand::Quit).await;
                break;
            }

            // Global: F1..F5 jumps section (primary). Ctrl+digit is kept
            // as a fallback for terminals that deliver it, but on macOS
            // Terminal / iTerm2 only Ctrl+4..=Ctrl+5 actually register.
            let section_switch: Option<Section> = match key.code {
                KeyCode::F(n) => Section::from_function_key(n),
                KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Section::from_digit(c)
                }
                _ => None,
            };
            if let Some(section) = section_switch {
                state.active_section = section;
                // Refresh section-specific data on entry. Cheap operations;
                // the backend replies with an event that updates the section.
                match section {
                    Section::Models => {
                        let _ = command_tx.send(AppCommand::Models).await;
                    }
                    Section::Databases => {
                        let _ = command_tx.send(AppCommand::Status).await;
                    }
                    Section::Index => {
                        state.index_section.prefill_db(&state.current_db);
                    }
                    Section::Settings => {
                        let _ = command_tx.send(AppCommand::Status).await;
                    }
                    _ => {}
                }
                continue;
            }

            // Dispatch to active section.
            let outcome = state.dispatch_key(key);
            match outcome {
                SectionOutcome::Consumed => {}
                SectionOutcome::Submit(text) => {
                    let cmd = parse_input(&text);
                    match &cmd {
                        AppCommand::Quit => {
                            let _ = command_tx.send(AppCommand::Quit).await;
                            break;
                        }
                        AppCommand::Ask { query } => {
                            state.chat_section.chat.push_message(Message {
                                role: Role::User,
                                content: query.clone(),
                            });
                        }
                        _ => {}
                    }
                    let _ = command_tx.send(cmd).await;
                }
                SectionOutcome::RunCommand(cmd) => {
                    let _ = command_tx.send(cmd).await;
                }
                SectionOutcome::Unhandled => {
                    // Global fallbacks for keys the section didn't use.
                    if matches!(key.code, KeyCode::Char('?')) {
                        state.overlay = Some(Box::new(HelpOverlay));
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn draw_hint_line(frame: &mut ratatui::Frame, area: Rect, hints: &str) {
    let widget = Paragraph::new(Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(widget, area);
}

fn handle_app_event(event: AppEvent, state: &mut AppState) {
    match event {
        AppEvent::StreamToken(token) => {
            state.chat_section.chat.push_token(&token);
        }
        AppEvent::StreamDone => {
            state.chat_section.chat.finish_stream();
        }
        AppEvent::SearchResults(results) => {
            state.chat_section.context.search_results = results;
        }
        AppEvent::RepoMap(map) => {
            state.chat_section.context.repo_map = Some(map);
        }
        AppEvent::ModelChanged(tier, name) => {
            state.model_tier = tier;
            state.model_name = name;
        }
        AppEvent::StoreStats(stats) => {
            state.store_stats = Some(stats);
        }
        AppEvent::Error(msg) => {
            error!("TUI received error: {msg}");
            state.chat_section.chat.push_message(Message {
                role: Role::System,
                content: format!("Error: {msg}"),
            });
        }
        AppEvent::IndexProgress { done, total, current } => {
            state.indexing = Some(IndexingProgress { done, total, current });
        }
        AppEvent::IndexDone { indexed, skipped, failed } => {
            state.indexing = None;
            state.index_section.on_index_done(indexed, skipped, failed);
            state.chat_section.chat.push_message(Message {
                role: Role::System,
                content: format!("Indexed {indexed}, skipped {skipped}, failed {failed}."),
            });
        }
        AppEvent::DbListing(names) => {
            let list = if names.is_empty() {
                "(no DBs — index one from the CLI: `lv index <path> <name>`)".to_string()
            } else {
                names.join(", ")
            };
            state.chat_section.chat.push_message(Message {
                role: Role::System,
                content: format!("DBs: {list}"),
            });
        }
        AppEvent::DbSwitched(name) => {
            state.current_db = name.clone();
            state.chat_section.chat.push_message(Message {
                role: Role::System,
                content: format!("Switched to DB '{name}'."),
            });
        }
        AppEvent::Status(snapshot) => {
            state.databases_section.update(snapshot.databases.clone());
            state.settings_section.update(*snapshot);
        }
        AppEvent::ModelsSnapshot(rows) => {
            state.models_section.update(rows);
        }
        AppEvent::ModelLoading(_) | AppEvent::ModelLoaded(_) | AppEvent::ModelUnloaded(_) => {}
        AppEvent::ModelLoadFailed(tier, err) => {
            state.chat_section.chat.push_message(Message {
                role: Role::System,
                content: format!("failed to load {}: {err}", tier_label(tier)),
            });
        }
        AppEvent::ActiveTierChanged(tier, name) => {
            state.model_tier = tier;
            state.model_name = name;
        }
        AppEvent::WarmCountChanged(n, active_loading) => {
            state.warm_count = n;
            state.active_loading = active_loading;
        }
        AppEvent::BrowseData { db, files, total_chunks } => {
            use crate::overlays::BrowseOverlay;
            state.overlay = Some(Box::new(BrowseOverlay::new(db, files, total_chunks)));
        }
    }
}

fn tier_label(t: ModelTier) -> &'static str {
    match t {
        ModelTier::Fast => "fast",
        ModelTier::Medium => "medium",
        ModelTier::Strong => "strong",
        ModelTier::Cloud => "cloud",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quit() {
        assert!(matches!(parse_input("/quit"), AppCommand::Quit));
        assert!(matches!(parse_input("  /quit  "), AppCommand::Quit));
    }

    #[test]
    fn parse_plain_question_becomes_ask() {
        match parse_input("what is this?") {
            AppCommand::Ask { query } => assert_eq!(query, "what is this?"),
            _ => panic!("expected Ask"),
        }
    }

    #[test]
    fn parse_slash_non_quit_becomes_ask() {
        // Legacy slash commands are gone — they now flow to the model as
        // normal prose so nothing silently disappears.
        match parse_input("/index /tmp/foo") {
            AppCommand::Ask { query } => assert_eq!(query, "/index /tmp/foo"),
            _ => panic!("expected Ask"),
        }
    }
}
