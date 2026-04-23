use crossterm::event::{KeyCode, KeyEvent};
use lv_core::status::language_histogram_by_files;
use lv_core::types::FileSummary;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::overlay::{centered, Overlay, OverlayAction};
use crate::widgets::selectable_list::{Item, KeyOutcome, SelectableList};

pub struct BrowseOverlay {
    db: String,
    total_chunks: usize,
    all_files: Vec<FileSummary>,
    languages: Vec<(String, usize)>,
    list: SelectableList<FileSummary>,
    lang_filter: Option<String>,
}

impl BrowseOverlay {
    pub fn new(db: String, files: Vec<FileSummary>, total_chunks: usize) -> Self {
        let languages = language_histogram_by_files(&files);
        let list = Self::build_list(&files);
        Self {
            db,
            total_chunks,
            all_files: files,
            languages,
            list,
            lang_filter: None,
        }
    }

    fn build_list(files: &[FileSummary]) -> SelectableList<FileSummary> {
        let items: Vec<Item<FileSummary>> = files
            .iter()
            .map(|f| {
                let lang = f.language.clone().unwrap_or_else(|| "?".into());
                let line = Line::from(vec![
                    Span::styled(
                        format!("[{lang:<8}] "),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(f.file_path.clone()),
                    Span::styled(
                        format!("  ({} chunk{})", f.chunk_count, if f.chunk_count == 1 { "" } else { "s" }),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                Item::new(line, f.clone())
            })
            .collect();
        SelectableList::new(items)
    }

    fn apply_language_filter(&mut self) {
        let files: Vec<FileSummary> = match &self.lang_filter {
            None => self.all_files.clone(),
            Some(lang) => self
                .all_files
                .iter()
                .filter(|f| f.language.as_deref() == Some(lang.as_str()))
                .cloned()
                .collect(),
        };
        self.list = Self::build_list(&files);
    }
}

impl Overlay for BrowseOverlay {
    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let outer = centered(area, 75, 80);

        let header = format!(
            " Browse: {} — {} files · {} chunks ",
            self.db,
            self.all_files.len(),
            self.total_chunks,
        );
        let title_suffix = match &self.lang_filter {
            Some(lang) => format!(" · filtering: {lang} "),
            None => String::new(),
        };

        frame.render_widget(
            Paragraph::new("").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("{header}{title_suffix}")),
            ),
            outer,
        );

        let inner = Rect {
            x: outer.x + 1,
            y: outer.y + 1,
            width: outer.width.saturating_sub(2),
            height: outer.height.saturating_sub(2),
        };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(3), Constraint::Length(3)])
            .split(inner);

        // Filter line
        let filter_line: Line = match self.list.filter() {
            Some(f) => Line::from(vec![
                Span::styled("filter: ", Style::default().fg(Color::Yellow)),
                Span::raw(f.to_string()),
                Span::styled("▏", Style::default().fg(Color::Yellow)),
            ]),
            None => Line::from(Span::styled(
                "  (press `/` to filter)",
                Style::default().fg(Color::DarkGray),
            )),
        };
        frame.render_widget(
            Paragraph::new(filter_line).block(Block::default().borders(Borders::BOTTOM)),
            rows[0],
        );

        // List
        self.list
            .draw(frame, rows[1], Block::default().borders(Borders::NONE));

        // Footer: language pills + hints
        let mut pills: Vec<Span> = Vec::new();
        for (i, (lang, count)) in self.languages.iter().take(9).enumerate() {
            let is_active = self.lang_filter.as_deref() == Some(lang.as_str());
            let pill_style = if is_active {
                Style::default().fg(Color::Black).bg(Color::Green)
            } else {
                Style::default().fg(Color::Green)
            };
            pills.push(Span::styled(format!(" [{}] ", i + 1), Style::default().fg(Color::Yellow)));
            pills.push(Span::styled(format!("{lang}:{count}"), pill_style));
        }
        let hint = Line::from(Span::styled(
            "↑/↓ · / filter · 1-9 lang filter · 0 clear · Esc close",
            Style::default().fg(Color::DarkGray),
        ));
        let footer = if pills.is_empty() {
            vec![hint]
        } else {
            vec![Line::from(pills), hint]
        };
        frame.render_widget(
            Paragraph::new(footer).block(Block::default().borders(Borders::TOP)),
            rows[2],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> OverlayAction {
        let outcome = self.list.handle_key(key);
        match outcome {
            KeyOutcome::Consumed | KeyOutcome::Unhandled | KeyOutcome::Activate(_) => OverlayAction::None,
            KeyOutcome::Escape => OverlayAction::Dismiss,
            KeyOutcome::Key(k) => match k.code {
                KeyCode::Char('0') => {
                    self.lang_filter = None;
                    self.apply_language_filter();
                    OverlayAction::None
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    let idx = (c as u8 - b'0') as usize;
                    if idx >= 1 && idx <= self.languages.len() {
                        self.lang_filter = Some(self.languages[idx - 1].0.clone());
                        self.apply_language_filter();
                    }
                    OverlayAction::None
                }
                _ => OverlayAction::None,
            },
        }
    }
}
