pub mod detail_panel;
pub mod filter_bar;
pub mod input_modal;
pub mod layout;
pub mod results_panel;
pub mod status_bar;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::{App, Focus, Modal};

pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub accent_dim: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub border: Color,
    pub border_focused: Color,
    pub header_bg: Color,
    pub status_bg: Color,
    pub chip_bg: Color,
    pub chip_fg: Color,
    pub muted: Color,
    pub error: Color,
    pub success: Color,
}

pub const DARK: Theme = Theme {
    bg: Color::Rgb(18, 18, 24),
    fg: Color::Rgb(220, 220, 228),
    accent: Color::Rgb(122, 162, 247),
    accent_dim: Color::Rgb(86, 95, 137),
    selected_bg: Color::Rgb(36, 40, 70),
    selected_fg: Color::Rgb(192, 202, 245),
    border: Color::Rgb(60, 64, 90),
    border_focused: Color::Rgb(122, 162, 247),
    header_bg: Color::Rgb(26, 27, 38),
    status_bg: Color::Rgb(26, 27, 38),
    chip_bg: Color::Rgb(41, 46, 66),
    chip_fg: Color::Rgb(166, 189, 245),
    muted: Color::Rgb(120, 130, 160),
    error: Color::Rgb(247, 118, 142),
    success: Color::Rgb(158, 206, 106),
};

pub fn render(frame: &mut Frame, app: &App) {
    let theme = &DARK;
    let area = frame.area();

    // Background
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.bg)),
        area,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(1), // filter bar
            Constraint::Min(0),    // main panels
            Constraint::Length(1), // status bar
        ])
        .split(area);

    render_header(frame, app, chunks[0], theme);
    filter_bar::render(frame, app, chunks[1], theme);
    render_main(frame, app, chunks[2], theme);
    status_bar::render(frame, app, chunks[3], theme);

    // Modals overlay
    match app.modal {
        Modal::Search => input_modal::render_search(frame, app, area, theme),
        Modal::Filter => input_modal::render_filter(frame, app, area, theme),
        Modal::Export => input_modal::render_export(frame, app, area, theme),
        Modal::Help => input_modal::render_help(frame, area, theme),
        Modal::Tag => input_modal::render_tag(frame, app, area, theme),
        Modal::None => {}
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let tick = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        / 100) as usize;

    let status_span = if app.is_fetching {
        Span::styled(
            format!(" {} fetching…", spinner_chars[tick % spinner_chars.len()]),
            Style::default().fg(theme.accent),
        )
    } else if !app.papers.is_empty() {
        Span::styled(
            format!(" ✓ {} results", app.papers.len()),
            Style::default().fg(theme.success),
        )
    } else {
        Span::styled(" ● idle".to_string(), Style::default().fg(theme.muted))
    };

    let source_spans: Vec<Span> = app
        .filters
        .sources
        .iter()
        .map(|s| {
            Span::styled(
                format!(" {} ", s),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();

    let mut spans = vec![
        Span::styled(
            " papyrus ",
            Style::default()
                .fg(theme.selected_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("v0.1.0", Style::default().fg(theme.muted)),
        Span::raw("  "),
    ];
    spans.extend(source_spans);
    spans.push(status_span);

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(theme.header_bg)),
        area,
    );
}

fn render_main(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    let results_focused = app.focus == Focus::Results && app.modal == Modal::None;
    let detail_focused = app.focus == Focus::Detail && app.modal == Modal::None;

    results_panel::render(frame, app, chunks[0], theme, results_focused);
    detail_panel::render(frame, app, chunks[1], theme, detail_focused);
}

pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
