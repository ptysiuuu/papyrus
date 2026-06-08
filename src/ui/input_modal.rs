use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, FilterFieldType};
use super::{centered_rect, Theme};

const EXPORT_FORMATS: &[&str] = &["JSON (.json)", "CSV (.csv)", "BibTeX (.bib)"];
const EXPORT_SCOPES: &[&str] = &["All results", "BibTeX buffer only"];

pub fn render_search(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let modal = centered_rect(60, 7, area);
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(
            " Search Papers ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.bg));

    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let input_display = format!("{}_", app.modal_input);
    let lines = vec![
        Line::from(Span::styled("Query:", Style::default().fg(theme.muted))),
        Line::from(Span::styled(
            input_display,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        Line::default(),
        Line::from(Span::styled(
            "Press Enter to search, Esc to cancel, ↑/↓ for history",
            Style::default().fg(theme.muted),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.bg)),
        inner,
    );
}

pub fn render_filter(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let modal = centered_rect(70, (app.filter_fields.len() as u16 + 6).min(area.height - 4), area);
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(
            " Filters ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.bg));

    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let items: Vec<ListItem> = app
        .filter_fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let is_active = i == app.filter_field_idx;
            let label_style = if is_active {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };
            let value_style = if is_active {
                Style::default().fg(theme.selected_fg).bg(theme.selected_bg)
            } else {
                Style::default().fg(theme.fg)
            };

            let value_display = match &field.field_type {
                FilterFieldType::Toggle(v) => {
                    if *v { "● ON ".to_string() } else { "○ off".to_string() }
                }
                _ => {
                    if is_active {
                        format!("{}_", field.value)
                    } else {
                        field.value.clone()
                    }
                }
            };

            let line = Line::from(vec![
                Span::styled(format!("{:<16}", field.label), label_style),
                Span::styled(value_display, value_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(theme.selected_bg))
        .style(Style::default().bg(theme.bg));

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.filter_field_idx));

    frame.render_stateful_widget(list, inner, &mut state);
}

pub fn render_export(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let modal = centered_rect(60, 14, area);
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(
            " Export ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.bg));

    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // label
            Constraint::Length(3), // format list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // scope label
            Constraint::Length(2), // scope list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // path label
            Constraint::Length(1), // path input
            Constraint::Min(0),    // hints
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("Format:", Style::default().fg(theme.muted)))),
        chunks[0],
    );

    let fmt_items: Vec<ListItem> = EXPORT_FORMATS
        .iter()
        .enumerate()
        .map(|(i, &fmt)| {
            let style = if i == app.export_format_idx {
                Style::default().fg(theme.selected_fg).bg(theme.selected_bg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            ListItem::new(Line::from(Span::styled(format!("  {}", fmt), style)))
        })
        .collect();
    let mut fmt_state = ratatui::widgets::ListState::default();
    fmt_state.select(Some(app.export_format_idx));
    frame.render_stateful_widget(List::new(fmt_items), chunks[1], &mut fmt_state);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("Scope:", Style::default().fg(theme.muted)))),
        chunks[3],
    );

    let scope_items: Vec<ListItem> = EXPORT_SCOPES
        .iter()
        .enumerate()
        .map(|(i, &scope)| {
            let style = if i == app.export_scope_idx {
                Style::default().fg(theme.selected_fg).bg(theme.selected_bg)
            } else {
                Style::default().fg(theme.fg)
            };
            ListItem::new(Line::from(Span::styled(format!("  {}", scope), style)))
        })
        .collect();
    let mut scope_state = ratatui::widgets::ListState::default();
    scope_state.select(Some(app.export_scope_idx));
    frame.render_stateful_widget(List::new(scope_items), chunks[4], &mut scope_state);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("Output path:", Style::default().fg(theme.muted)))),
        chunks[6],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{}_", app.export_path_input),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )))
        .style(Style::default().bg(theme.selected_bg)),
        chunks[7],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓ Format/Scope  Tab Switch  Enter Export  Esc Cancel",
            Style::default().fg(theme.muted),
        ))),
        chunks[8],
    );
}

pub fn render_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let modal = centered_rect(70, 34, area);
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(
            " Keybindings — papyrus ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.bg));

    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let rows: &[(&str, &str)] = &[
        ("/", "Open search input modal"),
        ("f", "Open filter modal"),
        ("e", "Open export modal"),
        ("r", "Re-run current search"),
        ("q", "Quit"),
        ("?", "This help screen"),
        ("j / ↓", "Move down in results"),
        ("k / ↑", "Move up in results"),
        ("J / Shift+↓", "Scroll detail panel down"),
        ("K / Shift+↑", "Scroll detail panel up"),
        ("g", "Jump to first result"),
        ("G", "Jump to last result"),
        ("Tab", "Switch focus: results ↔ detail"),
        ("Enter", "Open paper URL in browser"),
        ("p", "Open PDF URL in browser"),
        ("c", "Open code repository URL"),
        ("d", "Copy DOI to clipboard"),
        ("y", "Copy paper title to clipboard"),
        ("b", "Add paper to BibTeX buffer"),
        ("t", "Add/edit tag on current paper"),
        ("i", "Save paper to local library"),
        ("Ctrl+F", "Fuzzy-filter current results"),
        ("n", "Fetch next page of results"),
        ("N", "Fetch previous page"),
        ("Ctrl+C / q", "Force quit"),
        ("Esc", "Close modal / cancel"),
    ];

    let items: Vec<ListItem> = rows
        .iter()
        .map(|(key, desc)| {
            let line = Line::from(vec![
                Span::styled(
                    format!("{:<18}", key),
                    Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(*desc, Style::default().fg(theme.fg)),
            ]);
            ListItem::new(line)
        })
        .collect();

    frame.render_widget(List::new(items).style(Style::default().bg(theme.bg)), inner);
}

pub fn render_tag(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let modal = centered_rect(50, 6, area);
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(Span::styled(
            " Add Tag ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default().bg(theme.bg));

    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let lines = vec![
        Line::from(Span::styled("Tag name:", Style::default().fg(theme.muted))),
        Line::from(Span::styled(
            format!("{}_", app.modal_input),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        Line::default(),
        Line::from(Span::styled("Enter to save, Esc to cancel", Style::default().fg(theme.muted))),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}
