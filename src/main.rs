#![allow(dead_code)]
mod app;
mod cache;
mod config;
mod error;
mod export;
mod filters;
mod mcp;
mod models;
mod ratelimit;
mod scraper;
mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use app::{App, AppEvent, FilterFieldType, Focus, Modal};
use cache::DiskCache;
use config::Config;
use error::PapyrusError;
use export::{export_papers, ExportFormat};
use filters::FilterArgs;
use models::{Paper, PaperSourceKind};
use scraper::{ArxivSource, CrossRefSource, PaperSource, PubMedSource, SemanticScholarSource};

#[derive(Parser, Debug)]
#[command(name = "papyrus", about = "Terminal research paper scraper", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    filters: FilterArgs,

    #[arg(short = 'o', long)]
    output: Option<PathBuf>,

    #[arg(short = 'f', long)]
    format: Option<String>,

    #[arg(long = "no-tui")]
    no_tui: bool,

    #[arg(long, value_enum, default_value = "json", requires = "no_tui")]
    output_mode: OutputMode,

    #[arg(long)]
    quiet: bool,

    #[arg(long = "no-cache")]
    no_cache: bool,

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

#[derive(Debug, Clone, ValueEnum, Default)]
enum OutputMode {
    #[default]
    Json,
    Jsonl,
    Pretty,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Manage API keys stored in config.toml
    Keys(KeysArgs),
    /// Cache management
    Cache(CacheArgs),
    /// Start an MCP server over stdio (for use with Claude Code and other MCP hosts)
    Serve,
    /// Print JSON schema for tool input/output
    Schema {
        /// Which schema to print: input, output, or all
        #[arg(value_enum, default_value = "all")]
        which: SchemaTarget,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum SchemaTarget {
    Input,
    Output,
    All,
}

#[derive(clap::Args, Debug)]
struct KeysArgs {
    #[command(subcommand)]
    action: KeysAction,
}

#[derive(Subcommand, Debug)]
enum KeysAction {
    /// Set an API key: papyrus keys set <source> <key>
    Set { source: String, key: String },
    /// List configured keys (values are masked)
    List,
    /// Remove an API key: papyrus keys remove <source>
    Remove { source: String },
}

#[derive(clap::Args, Debug)]
struct CacheArgs {
    #[command(subcommand)]
    action: CacheAction,
}

#[derive(Subcommand, Debug)]
enum CacheAction {
    /// Delete all cached responses
    Clear,
    /// Show cache size and entry count
    Stats,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.clone().unwrap_or_else(Config::default_path);
    let config = Config::load(Some(&config_path)).unwrap_or_else(|e| {
        eprintln!("Config error: {}. Using defaults.", e);
        Config::default()
    });

    let http_client = build_http_client(cli.timeout, &config)?;

    // Subcommands that need sources (serve) or no sources (schema, keys, cache)
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Serve => {
                let filter_set = filters::FilterSet::default();
                let sources = build_sources(&filter_set, &http_client, cli.api_key.as_deref(), &config);
                let disk_cache = if cli.no_cache {
                    None
                } else {
                    DiskCache::new(Config::cache_dir(), config.general.cache_ttl_minutes)
                        .ok()
                        .map(Arc::new)
                };
                return mcp::run_mcp_server(sources, disk_cache).await;
            }
            Commands::Schema { which } => {
                return print_schema(which);
            }
            other => {
                return dispatch_subcommand(other, &config_path, &config);
            }
        }
    }

    let filter_set = cli.filters.into_filter_set()?;
    let sources = build_sources(&filter_set, &http_client, cli.api_key.as_deref(), &config);

    let disk_cache = if cli.no_cache {
        None
    } else {
        let ttl = config.general.cache_ttl_minutes;
        DiskCache::new(Config::cache_dir(), ttl).ok().map(Arc::new)
    };

    if cli.no_tui {
        run_batch(filter_set, sources, cli.output, cli.format, cli.quiet, cli.output_mode, disk_cache).await
    } else {
        run_tui(filter_set, sources, config, disk_cache).await
    }
}

fn print_schema(which: SchemaTarget) -> anyhow::Result<()> {
    use schemars::schema_for;
    let input_schema = serde_json::to_value(schema_for!(filters::FilterSet))?;
    let output_schema = serde_json::to_value(schema_for!(models::Paper))?;
    let out = match which {
        SchemaTarget::Input => serde_json::to_string_pretty(&input_schema)?,
        SchemaTarget::Output => serde_json::to_string_pretty(&output_schema)?,
        SchemaTarget::All => serde_json::to_string_pretty(&serde_json::json!({
            "input": input_schema,
            "output": output_schema,
        }))?,
    };
    println!("{}", out);
    Ok(())
}

fn dispatch_subcommand(
    cmd: Commands,
    config_path: &PathBuf,
    config: &Config,
) -> anyhow::Result<()> {
    match cmd {
        Commands::Keys(args) => match args.action {
            KeysAction::Set { source, key } => {
                Config::set_key(config_path, &source, &key)?;
                println!("Key set for '{}'.", source);
            }
            KeysAction::List => {
                println!("Configured API keys ({}):", config_path.display());
                Config::list_keys(config);
            }
            KeysAction::Remove { source } => {
                Config::remove_key(config_path, &source)?;
                println!("Key removed for '{}'.", source);
            }
        },
        Commands::Cache(args) => match args.action {
            CacheAction::Clear => {
                let cache = DiskCache::new(Config::cache_dir(), 60)?;
                let n = cache.clear()?;
                println!("Cleared {} cache entries.", n);
            }
            CacheAction::Stats => {
                let cache = DiskCache::new(Config::cache_dir(), 60)?;
                let (count, bytes) = cache.stats();
                println!(
                    "Cache: {} entries, {:.1} KB on disk",
                    count,
                    bytes as f64 / 1024.0
                );
                println!("Location: {}", Config::cache_dir().display());
            }
        },
        // Serve and Schema are handled before dispatch_subcommand is called
        Commands::Serve | Commands::Schema { .. } => unreachable!(),
    }
    Ok(())
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

    // Key resolution order: CLI --api-key → env var → config.toml
    let semantic_key = Config::resolve_key(
        api_key_override,
        "PAPYRUS_SEMANTIC_KEY",
        config.api_keys.semantic_scholar.as_deref(),
    );
    let pubmed_key = Config::resolve_key(
        api_key_override,
        "PAPYRUS_PUBMED_KEY",
        config.api_keys.pubmed.as_deref(),
    );

    for src in &filter_set.sources {
        match src {
            PaperSourceKind::Arxiv => {
                sources.push(Arc::new(ArxivSource::new(client.clone())));
            }
            PaperSourceKind::SemanticScholar => {
                sources.push(Arc::new(SemanticScholarSource::new(
                    client.clone(),
                    semantic_key.clone(),
                )));
            }
            PaperSourceKind::PubMed => {
                sources.push(Arc::new(PubMedSource::new(
                    client.clone(),
                    pubmed_key.clone(),
                )));
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
    output_mode: OutputMode,
    disk_cache: Option<Arc<DiskCache>>,
) -> anyhow::Result<()> {
    let mut all_papers: Vec<Paper> = Vec::new();
    let mut sources_hit: Vec<String> = Vec::new();
    let mut sources_degraded: Vec<String> = Vec::new();
    let mut all_rate_limited = true;
    let mut any_source_tried = false;

    for source in &sources {
        let name = source.name();
        let cache_key = DiskCache::cache_key(&filter_set, name);
        any_source_tried = true;

        // Try cache first
        if let Some(ref dc) = disk_cache {
            if let Some((papers, _)) = dc.get(&cache_key) {
                if !quiet {
                    eprintln!("[papyrus] {} -> {} papers (cached)", name, papers.len());
                }
                if matches!(output_mode, OutputMode::Jsonl) && output.is_none() {
                    for p in &papers {
                        println!("{}", serde_json::to_string(p).unwrap_or_default());
                    }
                }
                sources_hit.push(name.to_string());
                all_papers.extend(papers);
                all_rate_limited = false;
                continue;
            }
        }

        if !quiet {
            eprintln!("[papyrus] Fetching from {}...", name);
        }
        match fetch_with_retry(source.clone(), filter_set.clone(), 0, None, name.to_string()).await {
            Ok(result) => {
                if !quiet {
                    eprintln!("[papyrus] {} -> {} papers", name, result.papers.len());
                }
                if let Some(ref dc) = disk_cache {
                    let _ = dc.put(&cache_key, &result.papers, result.total_count);
                }
                if matches!(output_mode, OutputMode::Jsonl) && output.is_none() {
                    for p in &result.papers {
                        println!("{}", serde_json::to_string(p).unwrap_or_default());
                    }
                }
                sources_hit.push(name.to_string());
                all_papers.extend(result.papers);
                all_rate_limited = false;
            }
            Err(e) => {
                let is_rl = e.downcast_ref::<error::PapyrusError>()
                    .map_or(false, |pe| matches!(pe, error::PapyrusError::RateLimited { .. }));
                eprintln!("[papyrus] {} error: {}", name, e);
                sources_degraded.push(format!("{}: {}", name, e));
                if !is_rl {
                    all_rate_limited = false;
                }
            }
        }
    }

    all_papers = deduplicate(all_papers);

    // Emit JSONL metadata line last
    if matches!(output_mode, OutputMode::Jsonl) && output.is_none() {
        let meta = serde_json::json!({
            "__meta": true,
            "total": all_papers.len(),
            "sources_hit": sources_hit,
            "sources_degraded": sources_degraded,
        });
        println!("{}", serde_json::to_string(&meta).unwrap_or_default());
    }

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
    } else if !matches!(output_mode, OutputMode::Jsonl) {
        match output_mode {
            OutputMode::Json => {
                println!("{}", serde_json::to_string_pretty(&all_papers)?);
            }
            OutputMode::Pretty => {
                print_pretty_table(&all_papers);
            }
            OutputMode::Jsonl => {}
        }
    }

    // Exit codes per spec Section 17.3
    let had_error = !sources_degraded.is_empty();
    if all_papers.is_empty() && had_error {
        if any_source_tried && all_rate_limited {
            std::process::exit(4); // all sources rate limited
        }
        std::process::exit(2); // total failure
    }
    if had_error {
        std::process::exit(1); // partial success
    }
    Ok(())
}

fn print_pretty_table(papers: &[Paper]) {
    println!("{:>4}  {:<60}  {:>4}  {:<12}  {:>6}", "#", "Title", "Year", "Source", "Cites");
    println!("{}", "-".repeat(95));
    for (i, p) in papers.iter().enumerate() {
        let title: String = p.title.chars().take(60).collect();
        let year = p.year().map(|y| y.to_string()).unwrap_or_else(|| "    ".to_string());
        let src = p.source.to_string();
        let cites = p.citation_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
        println!("{:>4}  {:<60}  {:>4}  {:<12}  {:>6}", i + 1, title, year, src, cites);
    }
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
    disk_cache: Option<Arc<DiskCache>>,
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
        let dc = disk_cache.clone();
        let _ = tx.send(AppEvent::SearchStarted);
        tokio::spawn(async move {
            fetch_all(&srcs, &fs, 0, tx, dc).await;
        });
        app.is_fetching = true;
    }

    let result = run_tui_loop(&mut terminal, &mut app, sources, disk_cache.clone()).await;

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
    disk_cache: Option<Arc<DiskCache>>,
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
                            let dc = disk_cache.clone();
                            let page = app.page;
                            app.is_fetching = true;
                            app.papers.clear();
                            app.cached_sources.clear();
                            app.status_message = "Searching…".to_string();
                            let _ = tx.send(AppEvent::SearchStarted);
                            tokio::spawn(async move {
                                fetch_all(&srcs, &fs, page, tx, dc).await;
                            });
                        }
                        KeyAction::Refresh => {
                            let fs = app.filters.clone();
                            let tx = app.event_tx.clone();
                            let srcs = sources.clone();
                            let dc = disk_cache.clone();
                            let page = app.page;
                            app.is_fetching = true;
                            app.papers.clear();
                            app.cached_sources.clear();
                            app.status_message = "Refreshing…".to_string();
                            let _ = tx.send(AppEvent::SearchStarted);
                            tokio::spawn(async move {
                                fetch_all(&srcs, &fs, page, tx, dc).await;
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
        AppEvent::PapersReceived(papers, total, source_name, from_cache) => {
            if from_cache {
                app.cached_sources.insert(source_name.clone());
            }
            app.papers.extend(papers);
            app.papers = deduplicate(std::mem::take(&mut app.papers));
            if let Some(t) = total {
                app.total_count = Some(t.max(app.total_count.unwrap_or(0)));
            }
            let cache_label = if from_cache { " (cached)" } else { "" };
            app.status_message = format!("Loaded {} papers ({}{})", app.papers.len(), source_name, cache_label);
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
            app.status_message = format!("⚠ {}: {}", source, msg);
        }
        AppEvent::StatusUpdate(msg) => {
            app.status_message = msg;
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
    disk_cache: Option<Arc<DiskCache>>,
) {
    let mut handles = Vec::new();
    for source in sources.iter().cloned() {
        let fs = fs.clone();
        let tx = tx.clone();
        let dc = disk_cache.clone();
        handles.push(tokio::spawn(async move {
            let name = source.name().to_string();
            let cache_key = DiskCache::cache_key(&fs, &name);

            // Serve from cache when available
            if let Some(ref disk_cache) = dc {
                if let Some((papers, total)) = disk_cache.get(&cache_key) {
                    let _ = tx.send(AppEvent::PapersReceived(papers, total, name, true));
                    return;
                }
            }

            match fetch_with_retry(source, fs, page, Some(tx.clone()), name.clone()).await {
                Ok(result) => {
                    if let Some(ref disk_cache) = dc {
                        let _ = disk_cache.put(&cache_key, &result.papers, result.total_count);
                    }
                    let _ = tx.send(AppEvent::PapersReceived(
                        result.papers,
                        result.total_count,
                        name,
                        false,
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

/// Fetch from a source with one automatic retry on HTTP 429.
async fn fetch_with_retry(
    source: Arc<dyn PaperSource>,
    fs: filters::FilterSet,
    page: u32,
    tx: Option<mpsc::UnboundedSender<AppEvent>>,
    name: String,
) -> anyhow::Result<models::SearchResult> {
    match source.fetch(&fs, page).await {
        Ok(r) => Ok(r),
        Err(e) => {
            if let Some(PapyrusError::RateLimited { retry_after_secs, .. }) =
                e.downcast_ref::<PapyrusError>()
            {
                let wait = *retry_after_secs;
                if let Some(ref tx) = tx {
                    let _ = tx.send(AppEvent::StatusUpdate(format!(
                        "[{}] rate limited — retrying in {}s",
                        name, wait
                    )));
                } else {
                    eprintln!("[papyrus] [{}] rate limited — retrying in {}s", name, wait);
                }
                tokio::time::sleep(Duration::from_secs(wait)).await;
                source.fetch(&fs, page).await
            } else {
                Err(e)
            }
        }
    }
}

fn copy_to_clipboard(text: &str) {
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            let _ = cb.set_text(text);
        }
        Err(_) => {}
    }
}
