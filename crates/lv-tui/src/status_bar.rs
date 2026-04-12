use lv_core::types::{ModelTier, StoreStats};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::IndexingProgress;

pub fn draw_status_bar(
    frame: &mut Frame,
    area: Rect,
    model_tier: ModelTier,
    model_name: &str,
    stats: Option<&StoreStats>,
    current_db: &str,
    indexing: Option<&IndexingProgress>,
) {
    let tier_label = match model_tier {
        ModelTier::Fast => "Fast",
        ModelTier::Medium => "Medium",
        ModelTier::Strong => "Strong",
        ModelTier::Cloud => "Cloud",
    };

    let indexed = stats
        .map(|s| s.unique_files.to_string())
        .unwrap_or_else(|| "?".to_string());

    let mut spans = vec![
        Span::styled(" local-vibe v0.1  ", Style::default().fg(Color::White)),
        Span::styled(
            format!("[{tier_label}: {model_name}]"),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[db: {current_db}]"),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{indexed} indexed]"),
            Style::default().fg(Color::Green),
        ),
    ];

    if let Some(p) = indexing {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("[indexing {}/{}: {}]", p.done, p.total, p.current),
            Style::default().fg(Color::Magenta),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
