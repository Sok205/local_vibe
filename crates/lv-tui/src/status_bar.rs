use lv_core::types::{ModelTier, StoreStats};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::IndexingProgress;
use crate::sys_memory::{MemorySample, fmt_bytes};

pub struct StatusBarView<'a> {
    pub active_tier: ModelTier,
    pub model_name: &'a str,
    pub stats: Option<&'a StoreStats>,
    pub current_db: &'a str,
    pub warm_count: usize,
    pub active_loading: bool,
    pub indexing: Option<&'a IndexingProgress>,
    pub memory: MemorySample,
}

pub fn draw_status_bar(frame: &mut Frame, area: Rect, view: StatusBarView) {
    let StatusBarView {
        active_tier,
        model_name,
        stats,
        current_db,
        warm_count,
        active_loading,
        indexing,
        memory,
    } = view;
    let tier_label = match active_tier {
        ModelTier::Fast => "fast",
        ModelTier::Medium => "medium",
        ModelTier::Strong => "strong",
        ModelTier::Cloud => "cloud",
    };

    let active_color = if active_loading {
        Color::Yellow
    } else if warm_count > 0 {
        Color::Green
    } else {
        Color::Gray
    };

    let sep = || Span::styled(" · ", Style::default().fg(Color::DarkGray));
    let mut spans: Vec<Span> = Vec::with_capacity(16);

    spans.push(Span::styled(
        " ◆ local-vibe ",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ));
    spans.push(sep());
    spans.push(Span::styled(
        tier_label.to_string(),
        Style::default().fg(active_color),
    ));
    spans.push(Span::styled(":", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(
        model_name.to_string(),
        Style::default().fg(Color::White),
    ));
    spans.push(sep());
    spans.push(Span::styled(
        format!("db:{current_db}"),
        Style::default().fg(Color::Cyan),
    ));

    if let Some(s) = stats
        && s.unique_files > 0
    {
        spans.push(sep());
        spans.push(Span::styled(
            format!("{} files", s.unique_files),
            Style::default().fg(Color::Green),
        ));
    }

    spans.push(sep());
    let warm_color = if warm_count > 0 {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    spans.push(Span::styled(
        format!("{warm_count} warm"),
        Style::default().fg(warm_color),
    ));

    if memory.rss_bytes > 0 {
        spans.push(sep());
        let rss_color = match memory.rss_bytes {
            n if n > 16 * 1024 * 1024 * 1024 => Color::Red,
            n if n > 8 * 1024 * 1024 * 1024 => Color::Yellow,
            _ => Color::Green,
        };
        spans.push(Span::styled(
            format!("RSS {}", fmt_bytes(memory.rss_bytes)),
            Style::default().fg(rss_color),
        ));
    }

    if memory.swap_total_bytes > 0 {
        let pct = (memory.swap_used_bytes as f64 / memory.swap_total_bytes as f64) * 100.0;
        if pct >= 10.0 {
            spans.push(sep());
            let color = if pct > 80.0 {
                Color::Red
            } else if pct > 50.0 {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            spans.push(Span::styled(
                format!("swap {pct:.0}%"),
                Style::default().fg(color),
            ));
        }
    }

    if let Some(p) = indexing {
        spans.push(sep());
        spans.push(Span::styled(
            format!("indexing {}/{}: {}", p.done, p.total, p.current),
            Style::default().fg(Color::Magenta),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
