use lv_core::types::SearchResult;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub struct ContextPanel {
    pub repo_map: Option<String>,
    pub search_results: Vec<SearchResult>,
    pub visible: bool,
}

impl Default for ContextPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextPanel {
    pub fn new() -> Self {
        Self {
            repo_map: None,
            search_results: Vec::new(),
            visible: true,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        if let Some(repo_map) = &self.repo_map {
            lines.push(Line::styled(
                "── Repo Map ──",
                Style::default().fg(Color::Yellow),
            ));
            for line in repo_map.lines().take(20) {
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Gray),
                )));
            }
            lines.push(Line::raw(""));
        }

        if !self.search_results.is_empty() {
            lines.push(Line::styled(
                "── Sources ──",
                Style::default().fg(Color::Yellow),
            ));
            for result in &self.search_results {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:.2} ", result.score),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(
                        result.file_name.clone(),
                        Style::default().fg(Color::Cyan),
                    ),
                ]));
                let preview: String = result.text.lines().next().unwrap_or("").chars().take(50).collect();
                lines.push(Line::from(Span::styled(
                    format!("  {preview}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        if lines.is_empty() {
            lines.push(Line::styled(
                "No context yet.",
                Style::default().fg(Color::DarkGray),
            ));
        }

        let widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Context"))
            .wrap(Wrap { trim: true });

        frame.render_widget(widget, area);
    }
}
