use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Modal};
use super::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let hints = match app.modal {
        Modal::Search => vec![
            hint("Enter", "Search"),
            hint("↑↓", "History"),
            hint("Esc", "Cancel"),
        ],
        Modal::Filter => vec![
            hint("Tab", "Next field"),
            hint("Enter", "Apply"),
            hint("Space", "Toggle"),
            hint("Esc", "Cancel"),
        ],
        Modal::Export => vec![
            hint("↑↓", "Format"),
            hint("Enter", "Export"),
            hint("Esc", "Cancel"),
        ],
        Modal::Help => vec![hint("Esc", "Close")],
        Modal::Tag => vec![hint("Enter", "Add tag"), hint("Esc", "Cancel")],
        Modal::None => vec![
            hint("/", "Search"),
            hint("f", "Filters"),
            hint("e", "Export"),
            hint("r", "Refresh"),
            hint("?", "Help"),
            hint("q", "Quit"),
        ],
    };

    let mut spans: Vec<Span> = Vec::new();
    // Status message on the left
    if !app.status_message.is_empty() {
        spans.push(Span::styled(
            format!(" {} ", app.status_message),
            Style::default().fg(theme.fg),
        ));
        spans.push(Span::styled(" │ ", Style::default().fg(theme.muted)));
    }
    for (key, action) in hints {
        spans.push(Span::styled(
            format!("[{}]", key),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}  ", action),
            Style::default().fg(theme.muted),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.status_bg)),
        area,
    );
}

fn hint(key: &'static str, action: &'static str) -> (&'static str, &'static str) {
    (key, action)
}
