use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
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

    let block = Block::default()
        .title(Span::styled(
            " Detail View ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .style(Style::default().bg(theme.bg));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(paper) = app.selected_paper() else {
        let placeholder = Paragraph::new(Line::from(Span::styled(
            "Select a paper to view details",
            Style::default().fg(theme.muted),
        )));
        frame.render_widget(placeholder, inner);
        return;
    };

    let label = |s: &str| Span::styled(format!("{:<16}", s), Style::default().fg(theme.muted));
    let value = |s: &str| Span::styled(s.to_string(), Style::default().fg(theme.fg));
    let link = |s: &str| {
        Span::styled(
            s.to_string(),
            Style::default().fg(theme.accent).add_modifier(Modifier::UNDERLINED),
        )
    };

    let mut lines: Vec<Line> = Vec::new();

    // Title (possibly multi-line)
    lines.push(Line::from(vec![
        label("Title"),
        Span::styled(
            paper.title.clone(),
            Style::default().fg(theme.selected_fg).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::default());

    // Authors
    let authors_str = paper.authors.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    lines.push(Line::from(vec![label("Authors"), value(&authors_str)]));

    // Date
    let date_str = paper
        .published_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    lines.push(Line::from(vec![label("Date"), value(&date_str)]));

    // Source + ID
    let source_id = if let Some(arxiv) = &paper.arxiv_id {
        format!("{} [{}]", paper.source, arxiv)
    } else {
        format!("{} [{}]", paper.source, paper.source_id)
    };
    lines.push(Line::from(vec![label("Source"), value(&source_id)]));

    // Citations
    if let Some(cites) = paper.citation_count {
        lines.push(Line::from(vec![
            label("Citations"),
            value(&format_number(cites)),
        ]));
    }

    // Categories
    if !paper.categories.is_empty() {
        lines.push(Line::from(vec![
            label("Categories"),
            value(&paper.categories.join(", ")),
        ]));
    }

    // Journal
    lines.push(Line::from(vec![
        label("Journal"),
        value(paper.journal.as_deref().unwrap_or("—")),
    ]));

    // DOI
    if let Some(doi) = &paper.doi {
        lines.push(Line::from(vec![label("DOI"), link(doi)]));
    }

    // Open access / peer reviewed badges
    let mut badges: Vec<Span> = Vec::new();
    if paper.is_open_access {
        badges.push(Span::styled(
            " OA ",
            Style::default().bg(theme.success).fg(theme.bg).add_modifier(Modifier::BOLD),
        ));
        badges.push(Span::raw(" "));
    }
    if paper.is_peer_reviewed {
        badges.push(Span::styled(
            " Peer Reviewed ",
            Style::default().bg(theme.accent).fg(theme.bg),
        ));
    }
    if !badges.is_empty() {
        lines.push(Line::default());
        lines.push(Line::from(badges));
    }

    // Tags
    if !paper.tags.is_empty() {
        let tag_spans: Vec<Span> = paper
            .tags
            .iter()
            .flat_map(|t| {
                vec![
                    Span::styled(
                        format!(" {} ", t),
                        Style::default().bg(theme.chip_bg).fg(theme.chip_fg),
                    ),
                    Span::raw(" "),
                ]
            })
            .collect();
        lines.push(Line::from(vec![label("Tags")]));
        lines.push(Line::from(tag_spans));
    }

    // Abstract
    if let Some(abs) = &paper.abstract_text {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Abstract",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled("─".repeat(inner.width as usize), Style::default().fg(theme.border))));
        // Word-wrap the abstract manually by splitting into words
        let words: Vec<&str> = abs.split_whitespace().collect();
        let max_width = inner.width.saturating_sub(2) as usize;
        let mut current_line = String::new();
        for word in words {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= max_width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(Line::from(Span::styled(current_line.clone(), Style::default().fg(theme.fg))));
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(Line::from(Span::styled(current_line, Style::default().fg(theme.fg))));
        }
    }

    // Links section
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Links & Actions",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled("─".repeat(inner.width as usize), Style::default().fg(theme.border))));

    let mut action_spans: Vec<Span> = Vec::new();
    if paper.pdf_url.is_some() {
        action_spans.push(Span::styled("[p] PDF  ", Style::default().fg(theme.success)));
    }
    if paper.html_url.is_some() {
        action_spans.push(Span::styled("[Enter] HTML  ", Style::default().fg(theme.accent)));
    }
    if paper.code_url.is_some() {
        action_spans.push(Span::styled("[c] Code  ", Style::default().fg(theme.chip_fg)));
    }
    if paper.doi.is_some() {
        action_spans.push(Span::styled("[d] Copy DOI  ", Style::default().fg(theme.muted)));
    }
    action_spans.push(Span::styled("[b] BibTeX  ", Style::default().fg(theme.muted)));
    action_spans.push(Span::styled("[t] Tag", Style::default().fg(theme.muted)));
    lines.push(Line::from(action_spans));

    let para = Paragraph::new(lines)
        .scroll((app.detail_scroll as u16, 0))
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(theme.bg));

    frame.render_widget(para, inner);
}

fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
