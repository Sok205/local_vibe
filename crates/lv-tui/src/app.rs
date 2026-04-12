use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use lv_core::types::{Message, ModelTier, Role, SearchResult, StoreStats};
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
    context_panel::ContextPanel,
    input::{InputAction, InputBuffer},
    status_bar::draw_status_bar,
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
}

pub enum AppCommand {
    Ask { query: String },
    Index { path: String },
    Quit,
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
                state.model_tier,
                &state.model_name,
                state.store_stats.as_ref(),
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
            .block(Block::default().borders(Borders::ALL));
            frame.render_widget(input_widget, rows[2]);

            // Position cursor inside input box
            let cursor_x = rows[2].x + 1 + 2 + state.input.cursor as u16;
            let cursor_y = rows[2].y + 1;
            frame.set_cursor_position((cursor_x, cursor_y));
        })?;

        if event::poll(poll_interval)? && let Event::Key(key) = event::read()? {
            let action = state.input.handle_key(key);
            match action {
                InputAction::Quit => {
                    let _ = command_tx.send(AppCommand::Quit).await;
                    break;
                }
                InputAction::Submit(text) => {
                    if text.trim() == "/quit" {
                        let _ = command_tx.send(AppCommand::Quit).await;
                        break;
                    }
                    if let Some(path) = text.trim().strip_prefix("/index ") {
                        let _ = command_tx
                            .send(AppCommand::Index { path: path.to_string() })
                            .await;
                    } else {
                        state.chat.push_message(Message {
                            role: Role::User,
                            content: text.clone(),
                        });
                        let _ = command_tx.send(AppCommand::Ask { query: text }).await;
                    }
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
    }
}
