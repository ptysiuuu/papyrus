#![allow(dead_code)]
mod app;
mod config;
mod error;
mod export;
mod filters;
mod models;
mod scraper;
mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use app::{App, AppEvent, FilterFieldType, Focus, Modal};
use config::Config;
use export::{export_papers, ExportFormat};
use filters::FilterArgs;
use models::{Paper, PaperSourceKind};
use scraper::{ArxivSource, CrossRefSource, PaperSource, PubMedSource, SemanticScholarSource};

#[derive(Parser, Debug)]
#[command(name = "papyrus", about = "Terminal research paper scraper", version)]
struct Cli {
    #[command(flatten)]
    filters: FilterArgs,

    #[arg(short = 'o', long)]
    output: Option<PathBuf>,

    #[arg(short = 'f', long)]
    format: Option<String>,

    #[arg(long = "no-tui")]
    no_tui: bool,

    #[arg(long)]
    quiet: bool,

    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(long, default_value = "15")]
    timeout: u64,

    #[arg(long, default_value = "3")]
    retries: u32,

    #[arg(long, default_value = "4")]
    concurrent: usize,

    #[arg(long = "api-key")]
    api_key: Option<String>,

    #[arg(short = 'v', long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load(cli.config.as_ref()).unwrap_or_else(|e| {
        eprintln!("Config error: {}. Using defaults.", e);
        Config::default()
    });

    let filter_set = cli.filters.into_filter_set()?;
    let http_client = build_http_client(cli.timeout, &config)?;

    let sources = build_sources(&filter_set, &http_client, cli.api_key.as_deref(), &config);

    if cli.no_tui {
        run_batch(filter_set, sources, cli.output, cli.format, cli.quiet).await
    } else {
        run_tui(filter_set, sources, config).await
    }
}

fn build_http_client(timeout_secs: u64, config: &Config) -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent(&config.network.user_agent)
        .build()
        .context("Building HTTP client")
}

fn build_sources(
    filter_set: &filters::FilterSet,
    client: &reqwest::Client,
    api_key_override: Option<&str>,
    config: &Config,
) -> Vec<Arc<dyn PaperSource>> {
    let mut sources: Vec<Arc<dyn PaperSource>> = Vec::new();

    for src in &filter_set.sources {
        match src {
            PaperSourceKind::Arxiv => {
                sources.push(Arc::new(ArxivSource::new(client.clone())));
            }
            PaperSourceKind::SemanticScholar => {
                let key = api_key_override
                    .map(String::from)
                    .or_else(|| config.api_keys.semantic_scholar.clone())
                    .filter(|k| !k.is_empty());
                sources.push(Arc::new(SemanticScholarSource::new(client.clone(), key)));
            }
            PaperSourceKind::PubMed => {
                let key = api_key_override
                    .map(String::from)
                    .or_else(|| config.api_keys.pubmed.clone())
                    .filter(|k| !k.is_empty());
                sources.push(Arc::new(PubMedSource::new(client.clone(), key)));
            }
            PaperSourceKind::CrossRef => {
                let email = if config.network.polite_email.is_empty() {
                    None
                } else {
                    Some(config.network.polite_email.clone())
                };
                sources.push(Arc::new(CrossRefSource::new(client.clone(), email)));
            }
        }
    }
    sources
}

async fn run_batch(
    filter_set: filters::FilterSet,
    sources: Vec<Arc<dyn PaperSource>>,
    output: Option<PathBuf>,
    format_override: Option<String>,
    quiet: bool,
) -> anyhow::Result<()> {
    let mut all_papers: Vec<Paper> = Vec::new();
    let mut had_error = false;

    for source in &sources {
        if !quiet {
            eprintln!("[papyrus] Fetching from {}…", source.name());
        }
        match source.fetch(&filter_set, 0).await {
            Ok(result) => {
                if !quiet {
                    eprintln!("[papyrus] {} → {} papers", source.name(), result.papers.len());
                }
                all_papers.extend(result.papers);
            }
            Err(e) => {
                eprintln!("[papyrus] {} error: {}", source.name(), e);
                had_error = true;
            }
        }
    }

    all_papers = deduplicate(all_papers);

    if let Some(ref path) = output {
        let fmt = format_override
            .as_deref()
            .and_then(ExportFormat::from_str)
            .or_else(|| ExportFormat::from_path(path))
            .unwrap_or(ExportFormat::Json);

        let mut file = std::fs::File::create(path)
            .with_context(|| format!("Creating output file {:?}", path))?;
        export_papers(&all_papers, &fmt, &mut file)?;

        if !quiet {
            eprintln!("[papyrus] Exported {} papers to {:?}", all_papers.len(), path);
        }
    } else {
        let json = serde_json::to_string_pretty(&all_papers)?;
        println!("{}", json);
    }

    if had_error {
        std::process::exit(1);
    }
    Ok(())
}

fn deduplicate(papers: Vec<Paper>) -> Vec<Paper> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for paper in papers {
        let key = paper.dedup_key();
        if seen.insert(key) {
            result.push(paper);
        }
    }
    result
}

async fn run_tui(
    filter_set: filters::FilterSet,
    sources: Vec<Arc<dyn PaperSource>>,
    _config: Config,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let sources = Arc::new(sources);
    let mut app = App::new(filter_set.clone());

    if filter_set.query.is_some()
        || filter_set.arxiv_id.is_some()
        || filter_set.doi.is_some()
        || !filter_set.authors.is_empty()
    {
        let tx = app.event_tx.clone();
        let srcs = sources.clone();
        let fs = filter_set.clone();
        let _ = tx.send(AppEvent::SearchStarted);
        tokio::spawn(async move {
            fetch_all(&srcs, &fs, 0, tx).await;
        });
        app.is_fetching = true;
    }

    let result = run_tui_loop(&mut terminal, &mut app, sources).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    sources: Arc<Vec<Arc<dyn PaperSource>>>,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        loop {
            match app.event_rx.try_recv() {
                Ok(ev) => handle_app_event(app, ev),
                Err(_) => break,
            }
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = handle_key(app, key) {
                    match action {
                        KeyAction::Quit => return Ok(()),
                        KeyAction::Search(fs) => {
                            let tx = app.event_tx.clone();
                            let srcs = sources.clone();
                            let page = app.page;
                            app.is_fetching = true;
                            app.papers.clear();
                            app.status_message = "Searching…".to_string();
                            let _ = tx.send(AppEvent::SearchStarted);
                            tokio::spawn(async move {
                                fetch_all(&srcs, &fs, page, tx).await;
                            });
                        }
                        KeyAction::Refresh => {
                            let fs = app.filters.clone();
                            let tx = app.event_tx.clone();
                            let srcs = sources.clone();
                            let page = app.page;
                            app.is_fetching = true;
                            app.papers.clear();
                            app.status_message = "Refreshing…".to_string();
                            let _ = tx.send(AppEvent::SearchStarted);
                            tokio::spawn(async move {
                                fetch_all(&srcs, &fs, page, tx).await;
                            });
                        }
                        KeyAction::Export => {
                            do_export(app);
                        }
                        KeyAction::OpenUrl(url) => {
                            let _ = open::that(url);
                        }
                        KeyAction::CopyToClipboard(text) => {
                            copy_to_clipboard(&text);
                        }
                    }
                }
            }
        }
    }
}

fn handle_app_event(app: &mut App, ev: AppEvent) {
    match ev {
        AppEvent::SearchStarted => {
            app.is_fetching = true;
        }
        AppEvent::PapersReceived(papers, total, source_name) => {
            app.papers.extend(papers);
            app.papers = deduplicate(std::mem::take(&mut app.papers));
            if let Some(t) = total {
                app.total_count = Some(t.max(app.total_count.unwrap_or(0)));
            }
            app.status_message = format!("Loaded {} papers ({})", app.papers.len(), source_name);
        }
        AppEvent::SearchCompleted => {
            app.is_fetching = false;
            if app.papers.is_empty() {
                app.status_message = "No results found".to_string();
            } else {
                app.status_message = format!("✓ {} papers found", app.papers.len());
            }
            if app.selected_idx >= app.papers.len() && !app.papers.is_empty() {
                app.selected_idx = 0;
            }
        }
        AppEvent::SearchError(source, msg) => {
            app.fetch_errors.push(format!("{}: {}", source, msg));
            app.status_message = format!("⚠ {} error: {}", source, msg);
        }
        AppEvent::Quit => {}
    }
}

enum KeyAction {
    Quit,
    Search(filters::FilterSet),
    Refresh,
    Export,
    OpenUrl(String),
    CopyToClipboard(String),
}

fn handle_key(app: &mut App, key: KeyEvent) -> Option<KeyAction> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(KeyAction::Quit);
    }

    match app.modal {
        Modal::Search => handle_search_modal(app, key),
        Modal::Filter => handle_filter_modal(app, key),
        Modal::Export => handle_export_modal(app, key),
        Modal::Help | Modal::Tag => handle_generic_modal(app, key),
        Modal::None => handle_normal(app, key),
    }
}

fn handle_search_modal(app: &mut App, key: KeyEvent) -> Option<KeyAction> {
    match key.code {
        KeyCode::Esc => {
            app.close_modal();
            None
        }
        KeyCode::Enter => {
            let fs = app.apply_search_modal()?;
            Some(KeyAction::Search(fs))
        }
        KeyCode::Up => {
            app.history_up();
            None
        }
        KeyCode::Down => {
            app.history_down();
            None
        }
        KeyCode::Backspace => {
            app.modal_input_backspace();
            None
        }
        KeyCode::Char(c) => {
            app.modal_input_push(c);
            None
        }
        _ => None,
    }
}

fn handle_filter_modal(app: &mut App, key: KeyEvent) -> Option<KeyAction> {
    match key.code {
        KeyCode::Esc => {
            app.close_modal();
            None
        }
        KeyCode::Enter => {
            let fs = app.apply_filter_modal()?;
            Some(KeyAction::Search(fs))
        }
        KeyCode::Tab | KeyCode::Down => {
            app.filter_field_idx = (app.filter_field_idx + 1) % app.filter_fields.len();
            None
        }
        KeyCode::BackTab | KeyCode::Up => {
            if app.filter_field_idx == 0 {
                app.filter_field_idx = app.filter_fields.len() - 1;
            } else {
                app.filter_field_idx -= 1;
            }
            None
        }
        KeyCode::Char(' ') => {
            let idx = app.filter_field_idx;
            if let FilterFieldType::Toggle(ref mut v) = app.filter_fields[idx].field_type {
                *v = !*v;
                app.filter_fields[idx].value = if *v { "yes" } else { "no" }.to_string();
            }
            None
        }
        KeyCode::Backspace => {
            let idx = app.filter_field_idx;
            if let FilterFieldType::Text = app.filter_fields[idx].field_type {
                app.filter_fields[idx].value.pop();
            }
            None
        }
        KeyCode::Char(c) => {
            let idx = app.filter_field_idx;
            if let FilterFieldType::Text = app.filter_fields[idx].field_type {
                app.filter_fields[idx].value.push(c);
            }
            None
        }
        _ => None,
    }
}

fn handle_export_modal(app: &mut App, key: KeyEvent) -> Option<KeyAction> {
    match key.code {
        KeyCode::Esc => {
            app.close_modal();
            None
        }
        KeyCode::Enter => {
            app.close_modal();
            Some(KeyAction::Export)
        }
        KeyCode::Up => {
            if app.export_format_idx > 0 {
                app.export_format_idx -= 1;
            }
            None
        }
        KeyCode::Down => {
            if app.export_format_idx < 2 {
                app.export_format_idx += 1;
            }
            None
        }
        KeyCode::Tab => {
            app.export_scope_idx = (app.export_scope_idx + 1) % 2;
            None
        }
        KeyCode::Backspace => {
            app.export_path_input.pop();
            None
        }
        KeyCode::Char(c) => {
            app.export_path_input.push(c);
            None
        }
        _ => None,
    }
}

fn handle_generic_modal(app: &mut App, key: KeyEvent) -> Option<KeyAction> {
    match key.code {
        KeyCode::Esc => {
            if app.modal == Modal::Tag {
                let tag = app.modal_input.trim().to_string();
                if !tag.is_empty() {
                    let idx = app.selected_idx;
                    if let Some(paper) = app.papers.get_mut(idx) {
                        if !paper.tags.contains(&tag) {
                            paper.tags.push(tag.clone());
                            app.status_message = format!("Tagged with \"{}\"", tag);
                        }
                    }
                }
            }
            app.close_modal();
            None
        }
        KeyCode::Enter if app.modal == Modal::Tag => {
            let tag = app.modal_input.trim().to_string();
            if !tag.is_empty() {
                let idx = app.selected_idx;
                if let Some(paper) = app.papers.get_mut(idx) {
                    if !paper.tags.contains(&tag) {
                        paper.tags.push(tag.clone());
                        app.status_message = format!("Tagged with \"{}\"", tag);
                    }
                }
            }
            app.close_modal();
            None
        }
        KeyCode::Backspace if app.modal == Modal::Tag => {
            app.modal_input_backspace();
            None
        }
        KeyCode::Char(c) if app.modal == Modal::Tag => {
            app.modal_input_push(c);
            None
        }
        _ => None,
    }
}

fn handle_normal(app: &mut App, key: KeyEvent) -> Option<KeyAction> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
        app.fuzzy_active = !app.fuzzy_active;
        if !app.fuzzy_active {
            app.fuzzy_input.clear();
            app.fuzzy_filter = None;
        }
        return None;
    }

    if app.fuzzy_active {
        match key.code {
            KeyCode::Esc => {
                app.fuzzy_active = false;
                app.fuzzy_input.clear();
                app.fuzzy_filter = None;
            }
            KeyCode::Backspace => {
                app.fuzzy_input.pop();
                app.fuzzy_filter = if app.fuzzy_input.is_empty() {
                    None
                } else {
                    Some(app.fuzzy_input.clone())
                };
            }
            KeyCode::Char(c) => {
                app.fuzzy_input.push(c);
                app.fuzzy_filter = Some(app.fuzzy_input.clone());
                app.selected_idx = 0;
            }
            _ => {}
        }
        return None;
    }

    match key.code {
        KeyCode::Char('q') => return Some(KeyAction::Quit),
        KeyCode::Char('/') => app.open_search_modal(),
        KeyCode::Char('f') => app.open_filter_modal(),
        KeyCode::Char('e') => app.open_export_modal(),
        KeyCode::Char('?') => app.modal = Modal::Help,
        KeyCode::Char('r') => return Some(KeyAction::Refresh),

        KeyCode::Char('j') | KeyCode::Down => {
            if app.focus == Focus::Results {
                app.move_down();
            } else {
                app.scroll_detail_down();
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.focus == Focus::Results {
                app.move_up();
            } else {
                app.scroll_detail_up();
            }
        }
        KeyCode::Char('J') => app.scroll_detail_down(),
        KeyCode::Char('K') => app.scroll_detail_up(),
        KeyCode::Char('g') => app.jump_first(),
        KeyCode::Char('G') => app.jump_last(),
        KeyCode::Tab => app.toggle_focus(),

        KeyCode::Char('n') => {
            app.page += 1;
            return Some(KeyAction::Refresh);
        }
        KeyCode::Char('N') => {
            if app.page > 0 {
                app.page -= 1;
                return Some(KeyAction::Refresh);
            }
        }

        KeyCode::Enter => {
            if let Some(paper) = app.selected_paper() {
                let url = paper
                    .html_url
                    .clone()
                    .or_else(|| paper.doi.as_ref().map(|d| format!("https://doi.org/{}", d)));
                if let Some(url) = url {
                    return Some(KeyAction::OpenUrl(url));
                }
            }
        }
        KeyCode::Char('p') => {
            if let Some(url) = app.selected_paper().and_then(|p| p.pdf_url.clone()) {
                return Some(KeyAction::OpenUrl(url));
            }
        }
        KeyCode::Char('c') => {
            if let Some(url) = app.selected_paper().and_then(|p| p.code_url.clone()) {
                return Some(KeyAction::OpenUrl(url));
            }
        }
        KeyCode::Char('d') => {
            if let Some(doi) = app.selected_paper().and_then(|p| p.doi.clone()) {
                app.status_message = format!("Copied DOI: {}", doi);
                return Some(KeyAction::CopyToClipboard(doi));
            }
        }
        KeyCode::Char('y') => {
            if let Some(title) = app.selected_paper().map(|p| p.title.clone()) {
                app.status_message = format!("Copied: {}", app::truncate(&title, 50));
                return Some(KeyAction::CopyToClipboard(title));
            }
        }
        KeyCode::Char('b') => {
            app.add_to_bibtex();
        }
        KeyCode::Char('t') => {
            app.modal = Modal::Tag;
            app.modal_input.clear();
            app.modal_cursor = 0;
        }
        _ => {}
    }
    None
}

fn do_export(app: &mut App) {
    let fmt = match app.export_format_idx {
        0 => ExportFormat::Json,
        1 => ExportFormat::Csv,
        2 => ExportFormat::BibTeX,
        _ => ExportFormat::Json,
    };

    let papers: Vec<Paper> = if app.export_scope_idx == 1 {
        app.bibtex_buffer.clone()
    } else {
        app.papers.clone()
    };

    let path = PathBuf::from(&app.export_path_input);
    match std::fs::File::create(&path) {
        Ok(mut f) => match export_papers(&papers, &fmt, &mut f) {
            Ok(_) => {
                app.status_message =
                    format!("✓ Exported {} papers to {:?}", papers.len(), path);
            }
            Err(e) => {
                app.status_message = format!("Export error: {}", e);
            }
        },
        Err(e) => {
            app.status_message = format!("Cannot create {:?}: {}", path, e);
        }
    }
}

async fn fetch_all(
    sources: &[Arc<dyn PaperSource>],
    fs: &filters::FilterSet,
    page: u32,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    let mut handles = Vec::new();
    for source in sources.iter().cloned() {
        let fs = fs.clone();
        let tx = tx.clone();
        handles.push(tokio::spawn(async move {
            let name = source.name().to_string();
            match source.fetch(&fs, page).await {
                Ok(result) => {
                    let _ = tx.send(AppEvent::PapersReceived(
                        result.papers,
                        result.total_count,
                        name,
                    ));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::SearchError(name, e.to_string()));
                }
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    let _ = tx.send(AppEvent::SearchCompleted);
}

fn copy_to_clipboard(text: &str) {
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            let _ = cb.set_text(text);
        }
        Err(_) => {}
    }
}
