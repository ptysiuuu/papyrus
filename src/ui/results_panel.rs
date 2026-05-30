use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::App;
use super::Theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect, theme: &Theme, focused: bool) {
    let border_style = if focused {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border)
    };

    let papers = app.visible_papers();
    let count_label = if let Some(total) = app.total_count {
        format!("Results ({} found)", total)
    } else if !papers.is_empty() {
        format!("Results ({} loaded)", papers.len())
    } else {
        "Results".to_string()
    };

    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", count_label),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::default().bg(theme.bg));

    let source_badge_color = |src: &crate::models::PaperSourceKind| match src {
        crate::models::PaperSourceKind::Arxiv => theme.success,
        crate::models::PaperSourceKind::SemanticScholar => theme.accent,
        crate::models::PaperSourceKind::PubMed => ratatui::style::Color::Rgb(255, 158, 100),
        crate::models::PaperSourceKind::CrossRef => ratatui::style::Color::Rgb(224, 175, 104),
    };

    let items: Vec<ListItem> = papers
        .iter()
        .enumerate()
        .map(|(i, paper)| {
            let title_max = area.width.saturating_sub(18) as usize;
            let title = if paper.title.len() > title_max {
                format!("{}…", &paper.title[..title_max.min(paper.title.len())])
            } else {
                paper.title.clone()
            };
            let year = paper
                .year()
                .map(|y| y.to_string())
                .unwrap_or_else(|| "----".to_string());
            let badge = paper.source.to_string();

            let line = Line::from(vec![
                Span::styled(
                    format!("{:>3}. ", i + 1),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(title, Style::default().fg(theme.fg)),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", badge),
                    Style::default().fg(source_badge_color(&paper.source)).add_modifier(Modifier::DIM),
                ),
                Span::styled(
                    format!(" {}", year),
                    Style::default().fg(theme.muted),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    if !papers.is_empty() {
        state.select(Some(app.selected_idx));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(theme.selected_bg)
                .fg(theme.selected_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut state);

    // Fuzzy search indicator at bottom of panel
    if app.fuzzy_active {
        let hint_area = Rect {
            x: area.x + 2,
            y: area.y + area.height.saturating_sub(2),
            width: area.width.saturating_sub(4),
            height: 1,
        };
        let fuzzy_line = Line::from(vec![
            Span::styled("/ ", Style::default().fg(theme.accent)),
            Span::styled(app.fuzzy_input.as_str(), Style::default().fg(theme.fg)),
        ]);
        frame.render_widget(
            ratatui::widgets::Paragraph::new(fuzzy_line)
                .style(Style::default().bg(theme.selected_bg)),
            hint_area,
        );
    }
}
