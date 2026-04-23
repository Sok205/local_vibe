use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use lv_core::status::StatusSnapshot;
use lv_core::types::{FileSummary, Message, ModelTier, Role, SearchResult, StoreStats};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use tokio::sync::mpsc;
use tracing::error;

use crate::{
    chat_view::ChatView,
    commands::is_palette_prefix,
    context_panel::ContextPanel,
    input::{InputAction, InputBuffer},
    overlay::{Overlay, OverlayAction},
    overlays::{HelpOverlay, StatusOverlay},
    status_bar::{draw_status_bar, StatusBarView},
    widgets::palette::CommandPalette,
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

/// Parse one submitted input line into an `AppCommand`.
///
/// Slash commands:
/// - `/quit`                — `Quit`
/// - `/dbs`                 — `ListDbs`
/// - `/db <name>`           — `SwitchDb`
/// - `/index <path> [name]` — `Index { path, db }`
/// - `/status`              — `Status`
/// - `/help`, `/?`          — `Help`
///
/// Anything else becomes `Ask`.
pub fn parse_input(line: &str) -> AppCommand {
    let trimmed = line.trim();
    if trimmed == "/quit" {
        return AppCommand::Quit;
    }
    if trimmed == "/dbs" {
        return AppCommand::ListDbs;
    }
    if trimmed == "/status" {
        return AppCommand::Status;
    }
    if trimmed == "/models" {
        return AppCommand::Models;
    }
    if trimmed == "/browse" {
        return AppCommand::Browse(String::new());
    }
    if let Some(rest) = trimmed.strip_prefix("/browse ") {
        return AppCommand::Browse(rest.trim().to_string());
    }
    if trimmed == "/help" || trimmed == "/?" {
        return AppCommand::Help;
    }
    if let Some(rest) = trimmed.strip_prefix("/db ") {
        return AppCommand::SwitchDb(rest.trim().to_string());
    }
    if trimmed == "/index" {
        return AppCommand::OpenPicker;
    }
    if let Some(rest) = trimmed.strip_prefix("/index ") {
        let mut parts = rest.split_whitespace();
        let path = parts.next().unwrap_or("").to_string();
        let db = parts.next().map(|s| s.to_string());
        return AppCommand::Index { path, db };
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
    chat: ChatView,
    context: ContextPanel,
    input: InputBuffer,
    model_tier: ModelTier,
    model_name: String,
    store_stats: Option<StoreStats>,
    indexing: Option<IndexingProgress>,
    current_db: String,
    overlay: Option<Box<dyn Overlay>>,
    warm_count: usize,
    active_loading: bool,
    palette: CommandPalette,
}

impl AppState {
    fn new() -> Self {
        Self {
            chat: ChatView::new(),
            context: ContextPanel::new(),
            input: InputBuffer::new(),
            model_tier: ModelTier::Fast,
            model_name: "unknown".to_string(),
            store_stats: None,
            indexing: None,
            current_db: "default".to_string(),
            overlay: None,
            warm_count: 0,
            active_loading: false,
            palette: CommandPalette::new(),
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
        // Drain all pending backend events without blocking
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
                    Constraint::Length(3),
                ])
                .split(size);

            // Status bar
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

            // Main area: chat + optional context panel
            let main_area = rows[1];
            if state.context.visible {
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                    .split(main_area);

                state.chat.draw(frame, cols[0]);
                state.context.draw(frame, cols[1]);
            } else {
                state.chat.draw(frame, main_area);
            }

            // Input area
            let input_text: String = state.input.as_str();
            let input_widget = Paragraph::new(Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Yellow)),
                Span::raw(input_text),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title_top(
                        Line::from(Span::styled(
                            " /? for help ",
                            Style::default().fg(Color::DarkGray),
                        ))
                        .right_aligned(),
                    ),
            );
            frame.render_widget(input_widget, rows[2]);

            // Position cursor inside input box
            let cursor_x = rows[2].x + 1 + 2 + state.input.cursor as u16;
            let cursor_y = rows[2].y + 1;
            frame.set_cursor_position((cursor_x, cursor_y));

            // Command palette floats above the input when in palette mode.
            let current_input = state.input.as_str();
            if state.palette.visible(&current_input) && state.overlay.is_none() {
                state.palette.draw(frame, rows[2], &current_input);
            }

            if let Some(overlay) = state.overlay.as_mut() {
                overlay.draw(frame, size);
            }
        })?;

        if event::poll(poll_interval)? && let Event::Key(key) = event::read()? {
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
            // Tab completion on /index paths, before InputBuffer sees it.
            if matches!(key.code, KeyCode::Tab) {
                let current = state.input.as_str();
                if current.starts_with("/index ")
                    && let Some(completed) = crate::input_complete::complete_index_line(&current)
                {
                    state.input.set_from(&completed);
                    continue;
                }
            }

            // Command palette: intercepts nav + Enter while the input is a
            // slash-prefix with no whitespace. Any other key flows through to
            // the input buffer as normal.
            {
                let current = state.input.as_str();
                if is_palette_prefix(&current) {
                    match key.code {
                        KeyCode::Up => {
                            state.palette.move_up(&current);
                            continue;
                        }
                        KeyCode::Down | KeyCode::Tab => {
                            state.palette.move_down(&current);
                            continue;
                        }
                        KeyCode::Esc => {
                            state.input.clear();
                            state.palette.reset_selection();
                            continue;
                        }
                        KeyCode::Enter => {
                            if let Some(spec) = state.palette.selected_spec(&current) {
                                if spec.takes_args {
                                    state.input.set_from(&format!("{} ", spec.name));
                                    state.palette.reset_selection();
                                    continue;
                                }
                                // No-arg command: run via normal submit path.
                                let cmd = parse_input(spec.name);
                                state.input.clear();
                                state.palette.reset_selection();
                                match &cmd {
                                    AppCommand::Quit => {
                                        let _ = command_tx.send(AppCommand::Quit).await;
                                        break;
                                    }
                                    AppCommand::Help => {
                                        state.overlay = Some(Box::new(HelpOverlay));
                                        continue;
                                    }
                                    AppCommand::OpenPicker => {
                                        let start = std::env::current_dir()
                                            .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                        match crate::overlays::PickerOverlay::new(&start) {
                                            Ok(o) => state.overlay = Some(Box::new(o)),
                                            Err(e) => state.chat.push_message(Message {
                                                role: Role::System,
                                                content: format!("picker: {e}"),
                                            }),
                                        }
                                        continue;
                                    }
                                    _ => {}
                                }
                                let _ = command_tx.send(cmd).await;
                                continue;
                            }
                            // No matches: fall through so Enter submits the raw text.
                        }
                        _ => {}
                    }
                }
            }
            let action = state.input.handle_key(key);
            match action {
                InputAction::Quit => {
                    let _ = command_tx.send(AppCommand::Quit).await;
                    break;
                }
                InputAction::Submit(text) => {
                    let cmd = parse_input(&text);
                    match &cmd {
                        AppCommand::Quit => {
                            let _ = command_tx.send(AppCommand::Quit).await;
                            break;
                        }
                        AppCommand::Help => {
                            state.overlay = Some(Box::new(HelpOverlay));
                            continue;
                        }
                        AppCommand::OpenPicker => {
                            let start = std::env::current_dir()
                                .unwrap_or_else(|_| std::path::PathBuf::from("."));
                            match crate::overlays::PickerOverlay::new(&start) {
                                Ok(o) => state.overlay = Some(Box::new(o)),
                                Err(e) => state.chat.push_message(Message {
                                    role: Role::System,
                                    content: format!("picker: {e}"),
                                }),
                            }
                            continue;
                        }
                        AppCommand::Ask { query } => {
                            state.chat.push_message(Message {
                                role: Role::User,
                                content: query.clone(),
                            });
                        }
                        _ => {}
                    }
                    let _ = command_tx.send(cmd).await;
                }
                InputAction::ScrollUp => state.chat.scroll_up(),
                InputAction::ScrollDown => state.chat.scroll_down(),
                InputAction::ToggleContext => state.context.toggle(),
                InputAction::None => {}
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

fn handle_app_event(event: AppEvent, state: &mut AppState) {
    match event {
        AppEvent::StreamToken(token) => {
            state.chat.push_token(&token);
        }
        AppEvent::StreamDone => {
            state.chat.finish_stream();
        }
        AppEvent::SearchResults(results) => {
            state.context.search_results = results;
        }
        AppEvent::RepoMap(map) => {
            state.context.repo_map = Some(map);
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
            state.chat.push_message(Message {
                role: Role::System,
                content: format!("Error: {msg}"),
            });
        }
        AppEvent::IndexProgress { done, total, current } => {
            state.indexing = Some(IndexingProgress { done, total, current });
        }
        AppEvent::IndexDone { indexed, skipped, failed } => {
            state.indexing = None;
            state.chat.push_message(Message {
                role: Role::System,
                content: format!("Indexed {indexed}, skipped {skipped}, failed {failed}."),
            });
        }
        AppEvent::DbListing(names) => {
            let list = if names.is_empty() {
                "(no DBs — index one with `/index <path> <name>`)".to_string()
            } else {
                names.join(", ")
            };
            state.chat.push_message(Message {
                role: Role::System,
                content: format!("DBs: {list}"),
            });
        }
        AppEvent::DbSwitched(name) => {
            state.current_db = name.clone();
            state.chat.push_message(Message {
                role: Role::System,
                content: format!("Switched to DB '{name}'."),
            });
        }
        AppEvent::Status(snapshot) => {
            state.overlay = Some(Box::new(StatusOverlay::new(*snapshot)));
        }
        AppEvent::ModelsSnapshot(rows) => {
            use crate::overlays::ModelsOverlay;
            state.overlay = Some(Box::new(ModelsOverlay::new(rows)));
        }
        AppEvent::ModelLoading(_) | AppEvent::ModelLoaded(_) | AppEvent::ModelUnloaded(_) => {
            // Handled via follow-up ModelsSnapshot / WarmCountChanged events.
        }
        AppEvent::ModelLoadFailed(tier, err) => {
            state.chat.push_message(Message {
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
    fn parse_dbs() {
        assert!(matches!(parse_input("/dbs"), AppCommand::ListDbs));
    }

    #[test]
    fn parse_db_switch() {
        match parse_input("/db code") {
            AppCommand::SwitchDb(name) => assert_eq!(name, "code"),
            _ => panic!("expected SwitchDb"),
        }
    }

    #[test]
    fn parse_index_without_db() {
        match parse_input("/index /tmp/foo") {
            AppCommand::Index { path, db } => {
                assert_eq!(path, "/tmp/foo");
                assert!(db.is_none());
            }
            _ => panic!("expected Index"),
        }
    }

    #[test]
    fn parse_index_with_db() {
        match parse_input("/index /tmp/foo code") {
            AppCommand::Index { path, db } => {
                assert_eq!(path, "/tmp/foo");
                assert_eq!(db.as_deref(), Some("code"));
            }
            _ => panic!("expected Index"),
        }
    }

    #[test]
    fn parse_plain_question_becomes_ask() {
        match parse_input("what is this?") {
            AppCommand::Ask { query } => assert_eq!(query, "what is this?"),
            _ => panic!("expected Ask"),
        }
    }

    #[test]
    fn parse_status() {
        assert!(matches!(parse_input("/status"), AppCommand::Status));
        assert!(matches!(parse_input("  /status  "), AppCommand::Status));
    }

    #[test]
    fn parse_help_variants() {
        assert!(matches!(parse_input("/help"), AppCommand::Help));
        assert!(matches!(parse_input("/?"), AppCommand::Help));
        assert!(matches!(parse_input("  /help  "), AppCommand::Help));
    }
}
