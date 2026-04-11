use lv_core::types::{ModelTier, StoreStats};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn draw_status_bar(
    frame: &mut Frame,
    area: Rect,
    model_tier: ModelTier,
    model_name: &str,
    stats: Option<&StoreStats>,
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

    let line = Line::from(vec![
        Span::styled(" local-vibe v0.1  ", Style::default().fg(Color::White)),
        Span::styled(
            format!("[{tier_label}: {model_name}]"),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{indexed} indexed]"),
            Style::default().fg(Color::Green),
        ),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}
