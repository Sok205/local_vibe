use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::sections::Section;

/// Fixed-width left column listing every section with its `[n]` hotkey.
/// The currently-active section is highlighted in cyan; others are dimmed.
pub fn draw_sidebar(frame: &mut Frame, area: Rect, active: Section) {
    let mut lines: Vec<Line> = Vec::with_capacity(Section::ALL.len() + 2);
    lines.push(Line::raw(""));

    for section in Section::ALL {
        let is_active = section == active;
        let marker = if is_active { "▸ " } else { "  " };
        let style = if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(format!("[{}] ", section.hotkey()), style),
            Span::styled(section.label(), style),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  Ctrl+Q quit",
        Style::default().fg(Color::DarkGray),
    )));

    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(widget, area);
}
