use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use super::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let chips = app.filters.active_chips();

    if chips.is_empty() {
        let line = Line::from(Span::styled(
            " Filters: (none active)",
            Style::default().fg(theme.muted),
        ));
        frame.render_widget(Paragraph::new(line).style(Style::default().bg(theme.header_bg)), area);
        return;
    }

    let mut spans = vec![
        Span::styled(" Filters: ", Style::default().fg(theme.muted)),
    ];

    for chip in &chips {
        spans.push(Span::styled(
            format!("[{}]", chip),
            Style::default()
                .bg(theme.chip_bg)
                .fg(theme.chip_fg)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.header_bg)),
        area,
    );
}
