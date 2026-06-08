#![allow(dead_code)]
mod app;
mod cache;
mod citation_graph;
mod config;
mod db;
mod dedup;
mod download;
mod error;
mod export;
mod filters;
mod library;
mod mcp;
mod models;
mod plugin;
mod ratelimit;
mod scraper;
mod similarity;
mod ui;
mod watch;

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
        #[arg(value_enum, default_value = "all")]
        which: SchemaTarget,
    },
    /// Local paper library management
    Library(LibraryArgs),
    /// Citation graph operations
    #[command(name = "cite-graph")]
    CiteGraph(CiteGraphArgs),
    /// Watch queries for new papers
    Watch(WatchArgs),
    /// Find similar papers
    Similar(SimilarArgs),
    /// Download PDFs
    Download(DownloadArgs),
    /// Plugin management
    Plugins(PluginsArgs),
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

// ─── Library ─────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
struct LibraryArgs {
    #[command(subcommand)]
    action: LibraryAction,
}

#[derive(Subcommand, Debug)]
enum LibraryAction {
    /// Full-text search your local library
    Search {
        query: String,
        /// Also search inside PDF full-text
        #[arg(long)]
        fulltext: bool,
    },
    /// Add a paper by source:id (e.g. arxiv:2301.07041) to your library
    Add { paper_id: String },
    /// Tag a paper
    Tag { paper_id: String, tags: Vec<String> },
    /// Remove a tag from a paper
    Untag { paper_id: String, tag: String },
    /// Show library statistics
    Stats,
    /// Set read status (unread|reading|read|reviewed)
    Status { paper_id: String, status: String },
    /// Add or replace notes for a paper
    Note { paper_id: String, note: String },
    /// Set priority 1-5
    Priority { paper_id: String, #[arg(value_parser = clap::value_parser!(u8).range(1..=5))] priority: u8 },
    /// Show potential duplicate papers
    Duplicates,
    /// Create a collection
    #[command(name = "create-collection")]
    CreateCollection { name: String },
    /// List collections
    #[command(name = "list-collections")]
    ListCollections,
    /// Export a literature review
    #[command(name = "export-review")]
    ExportReview {
        #[arg(long)]
        collection: Option<String>,
        #[arg(short = 'o', long)]
        output: PathBuf,
        #[arg(short = 'f', long, default_value = "json")]
        format: String,
    },
}

// ─── Citation graph ───────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
struct CiteGraphArgs {
    #[command(subcommand)]
    action: CiteGraphAction,
}

#[derive(Subcommand, Debug)]
enum CiteGraphAction {
    /// Fetch and store references/citations for a paper
    Fetch {
        paper_id: String,
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    /// Walk backwards through references
    Ancestors {
        paper_id: String,
        #[arg(long, default_value = "3")]
        depth: usize,
    },
    /// Walk forward through citations
    Descendants {
        paper_id: String,
        #[arg(long, default_value = "2")]
        depth: usize,
    },
    /// Find shared references between two papers
    Common { id1: String, id2: String },
    /// Find highest-cited root nodes
    Seminal {
        #[arg(long, default_value = "10")]
        limit: usize,
    },
}

// ─── Watch ────────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
struct WatchArgs {
    #[command(subcommand)]
    action: WatchAction,
}

#[derive(Subcommand, Debug)]
enum WatchAction {
    /// Add a watch query
    Add {
        query: String,
        #[arg(long, short = 's')]
        sources: Vec<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        notify: bool,
    },
    /// Run all watches and report new papers
    Run {
        #[arg(long, default_value = "jsonl")]
        output_mode: String,
    },
    /// List saved watches
    List,
    /// Remove a watch by ID
    Remove { id: String },
}

// ─── Similar ─────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
struct SimilarArgs {
    /// Semantic Scholar paper ID
    paper_id: Option<String>,
    /// Find similar papers from your local library (offline TF-IDF)
    #[arg(long)]
    from_library: bool,
    #[arg(long, default_value = "10")]
    limit: u32,
}

// ─── Download ────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
struct DownloadArgs {
    /// Paper ID to download (from library or last search)
    paper_id: Option<String>,
    /// Download all papers with a PDF URL from last search results
    #[arg(long)]
    all: bool,
    /// Download directory (default: ~/papers)
    #[arg(long)]
    dir: Option<PathBuf>,
}

// ─── Plugins ─────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
struct PluginsArgs {
    #[command(subcommand)]
    action: PluginsAction,
}

#[derive(Subcommand, Debug)]
enum PluginsAction {
    /// List installed plugins
    List,
    /// Install a plugin (copy to plugins dir)
    Install { name: String },
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
            Commands::CiteGraph(args) => {
                return dispatch_cite_graph(args, &http_client, cli.api_key.as_deref(), &config).await;
            }
            Commands::Watch(args) => {
                return dispatch_watch(args, &http_client, cli.api_key.as_deref(), &config).await;
            }
            Commands::Similar(args) => {
                return dispatch_similar(args, &http_client, cli.api_key.as_deref(), &config).await;
            }
            Commands::Download(args) => {
                return dispatch_download(args, &http_client, &config).await;
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
        Commands::Library(args) => {
            let db = db::Database::open_default()?;
            dispatch_library(args, &db)?;
        }
        Commands::Plugins(args) => {
            dispatch_plugins(args)?;
        }
        // Async commands handled before dispatch_subcommand
        Commands::Serve | Commands::Schema { .. }
        | Commands::CiteGraph(_) | Commands::Watch(_)
        | Commands::Similar(_) | Commands::Download(_) => unreachable!(),
    }
    Ok(())
}

fn dispatch_library(args: LibraryArgs, db: &db::Database) -> anyhow::Result<()> {
    match args.action {
        LibraryAction::Search { query, fulltext } => {
            let papers = library::cmd_library_search(db, &query, fulltext)?;
            if papers.is_empty() {
                println!("No results found in library.");
            } else {
                println!("{:>4}  {:<60}  {:>4}  {:>6}", "#", "Title", "Year", "Cites");
                println!("{}", "-".repeat(80));
                for (i, p) in papers.iter().enumerate() {
                    let title: String = p.title.chars().take(60).collect();
                    let year = p.year().map(|y| y.to_string()).unwrap_or_default();
                    let cites = p.citation_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
                    println!("{:>4}  {:<60}  {:>4}  {:>6}", i + 1, title, year, cites);
                }
            }
        }
        LibraryAction::Add { paper_id } => {
            // paper_id format: "source:source_id"
            let (src, sid) = paper_id.split_once(':')
                .ok_or_else(|| anyhow::anyhow!("Use format source:id (e.g. arxiv:2301.07041)"))?;
            if let Some(paper) = db.get_paper_by_source(src, sid)? {
                println!("Paper already in library: {}", paper.title);
            } else {
                println!("Paper {} not found in cache. Run a search first to populate the library.", paper_id);
            }
        }
        LibraryAction::Tag { paper_id, tags } => {
            let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
            library::cmd_library_tag(db, &paper_id, &tag_refs)?;
            println!("Tagged {} with: {}", &paper_id[..8.min(paper_id.len())], tags.join(", "));
        }
        LibraryAction::Untag { paper_id, tag } => {
            library::cmd_library_untag(db, &paper_id, &tag)?;
            println!("Removed tag '{}' from {}", tag, &paper_id[..8.min(paper_id.len())]);
        }
        LibraryAction::Stats => {
            library::cmd_library_stats(db)?;
        }
        LibraryAction::Status { paper_id, status } => {
            library::cmd_library_status(db, &paper_id, &status)?;
            println!("Set status '{}' on {}", status, &paper_id[..8.min(paper_id.len())]);
        }
        LibraryAction::Note { paper_id, note } => {
            library::cmd_library_note(db, &paper_id, &note)?;
            println!("Note saved.");
        }
        LibraryAction::Priority { paper_id, priority } => {
            library::cmd_library_priority(db, &paper_id, priority)?;
            println!("Priority set to {}.", priority);
        }
        LibraryAction::Duplicates => {
            library::cmd_library_duplicates(db)?;
        }
        LibraryAction::CreateCollection { name } => {
            library::cmd_create_collection(db, &name)?;
        }
        LibraryAction::ListCollections => {
            library::cmd_list_collections(db)?;
        }
        LibraryAction::ExportReview { collection, output, format } => {
            let fmt = export::ExportFormat::from_str(&format)
                .ok_or_else(|| anyhow::anyhow!("Unknown format: {}", format))?;
            library::cmd_library_export_review(db, collection.as_deref(), &output, fmt)?;
        }
    }
    Ok(())
}

fn dispatch_plugins(args: PluginsArgs) -> anyhow::Result<()> {
    let plugins_dir = plugin::plugins_dir();
    match args.action {
        PluginsAction::List => {
            let plugins = plugin::discover_plugins(&plugins_dir)?;
            if plugins.is_empty() {
                println!("No plugins installed. Plugin dir: {}", plugins_dir.display());
                println!("Drop a plugin directory with manifest.toml into: {}", plugins_dir.display());
            } else {
                println!("{:<20} {:<10} {}", "Name", "Version", "Description");
                println!("{}", "-".repeat(60));
                for p in plugins {
                    println!("{:<20} {:<10} {}", p.name, p.version, p.description);
                }
            }
        }
        PluginsAction::Install { name } => {
            println!("To install a plugin, place its directory in: {}", plugins_dir.join(&name).display());
            println!("The directory must contain a manifest.toml file.");
        }
    }
    Ok(())
}

async fn dispatch_cite_graph(
    args: CiteGraphArgs,
    client: &reqwest::Client,
    api_key: Option<&str>,
    config: &Config,
) -> anyhow::Result<()> {
    let db = db::Database::open_default()?;
    let store = citation_graph::CitationGraphStore::new(db);
    let semantic_key = Config::resolve_key(api_key, "PAPYRUS_SEMANTIC_KEY", config.api_keys.semantic_scholar.as_deref());
    let graph_client = citation_graph::CitationGraphClient::new(client.clone(), semantic_key);

    match args.action {
        CiteGraphAction::Fetch { paper_id, limit } => {
            eprintln!("Fetching references for {}...", paper_id);
            let n = graph_client.fetch_and_store_references(&paper_id, &store, limit).await?;
            eprintln!("Fetching citations for {}...", paper_id);
            let m = graph_client.fetch_and_store_citations(&paper_id, &store, limit).await?;
            println!("Stored {} references and {} citations.", n, m);
        }
        CiteGraphAction::Ancestors { paper_id, depth } => {
            let nodes = store.ancestors(&paper_id, depth)?;
            println!("Ancestors of {} (depth {}): {} nodes", &paper_id, depth, nodes.len());
            for node in &nodes {
                let cites = node.citation_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
                println!("  [{:>8}] {} ({})", cites, node.title, node.s2id);
            }
        }
        CiteGraphAction::Descendants { paper_id, depth } => {
            let nodes = store.descendants(&paper_id, depth)?;
            println!("Descendants of {} (depth {}): {} nodes", &paper_id, depth, nodes.len());
            for node in &nodes {
                let cites = node.citation_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
                println!("  [{:>8}] {} ({})", cites, node.title, node.s2id);
            }
        }
        CiteGraphAction::Common { id1, id2 } => {
            let common = store.common_references(&id1, &id2)?;
            println!("Shared references between {} and {}: {} papers", id1, id2, common.len());
            for node in &common {
                let cites = node.citation_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
                println!("  [{:>8}] {} ({})", cites, node.title, node.s2id);
            }
        }
        CiteGraphAction::Seminal { limit } => {
            let nodes = store.seminal_nodes(limit)?;
            println!("Top {} seminal papers in citation graph:", limit);
            for (i, node) in nodes.iter().enumerate() {
                let cites = node.citation_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
                println!("  {:>3}. [{:>8}] {} ({})", i + 1, cites, node.title, node.s2id);
            }
        }
    }
    Ok(())
}

async fn dispatch_watch(
    args: WatchArgs,
    client: &reqwest::Client,
    api_key: Option<&str>,
    config: &Config,
) -> anyhow::Result<()> {
    let db = db::Database::open_default()?;

    match args.action {
        WatchAction::Add { query, sources, name, notify } => {
            let src_refs: Vec<&str> = if sources.is_empty() {
                vec!["arxiv", "semantic_scholar"]
            } else {
                sources.iter().map(String::as_str).collect()
            };
            let id = db.add_watch(&query, &src_refs, name.as_deref(), notify)?;
            println!("Watch added: '{}' (id: {})", query, &id[..8]);
        }
        WatchAction::List => {
            let watches = db.list_watches()?;
            if watches.is_empty() {
                println!("No watches configured.");
            } else {
                for w in &watches {
                    let last = w.last_run_at.as_deref().unwrap_or("never");
                    println!("[{}] {:?}: '{}' (sources: {}, last: {})",
                        &w.id[..8],
                        w.name.as_deref().unwrap_or("-"),
                        w.query,
                        w.sources.join(","),
                        last);
                }
            }
        }
        WatchAction::Remove { id } => {
            db.remove_watch(&id)?;
            println!("Watch {} removed.", &id[..8.min(id.len())]);
        }
        WatchAction::Run { output_mode } => {
            let watches = db.list_watches()?;
            if watches.is_empty() {
                eprintln!("No watches configured. Use 'papyrus watch add' to add one.");
                return Ok(());
            }

            let filter_set = filters::FilterSet::default();
            let sources = build_sources(&filter_set, client, api_key, config);

            for w in &watches {
                eprintln!("Running watch: {:?} '{}'", w.name.as_deref().unwrap_or(""), w.query);
                let mut fs = filters::FilterSet::default();
                fs.query = Some(w.query.clone());

                // Filter sources to those in the watch
                let watch_sources: Vec<Arc<dyn PaperSource>> = sources.iter()
                    .filter(|s| {
                        let name = s.name().to_lowercase().replace(' ', "_");
                        w.sources.contains(&name) || w.sources.is_empty()
                    })
                    .cloned()
                    .collect();

                let mut all_papers: Vec<Paper> = Vec::new();
                for source in &watch_sources {
                    match source.fetch(&fs, 0).await {
                        Ok(result) => all_papers.extend(result.papers),
                        Err(e) => eprintln!("  {} error: {}", source.name(), e),
                    }
                }
                all_papers = dedup::fuzzy_dedup(all_papers);

                let runner = watch::WatchRunner::new(db::Database::open_default()?);
                let new_papers = runner.filter_new_papers(&w.id, &all_papers)?;
                runner.update_last_run(&w.id)?;

                if new_papers.is_empty() {
                    eprintln!("  No new papers.");
                } else {
                    eprintln!("  {} new papers found.", new_papers.len());
                    if output_mode == "jsonl" {
                        watch::emit_jsonl(&new_papers, w.name.as_deref(), &w.query);
                    } else {
                        for p in &new_papers {
                            println!("[{}] {}", p.published_date.map(|d| d.to_string()).unwrap_or_default(), p.title);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn dispatch_similar(
    args: SimilarArgs,
    client: &reqwest::Client,
    api_key: Option<&str>,
    config: &Config,
) -> anyhow::Result<()> {
    let semantic_key = Config::resolve_key(api_key, "PAPYRUS_SEMANTIC_KEY", config.api_keys.semantic_scholar.as_deref());
    let sim_client = similarity::SimilarityClient::new(client.clone(), semantic_key);

    if let Some(paper_id) = args.paper_id {
        if args.from_library {
            // Offline TF-IDF against library
            let db = db::Database::open_default()?;
            let query_paper = db.get_paper_by_id(&paper_id)?
                .or_else(|| {
                    // Try source:id lookup
                    if let Some((src, sid)) = paper_id.split_once(':') {
                        db.get_paper_by_source(src, sid).ok().flatten()
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("Paper {} not found in library", paper_id))?;

            let all_papers = db.list_papers(usize::MAX, 0)?;
            let index = similarity::TfIdfIndex::build(&all_papers);
            let similar = index.find_similar(&query_paper, args.limit as usize);

            println!("Papers similar to: {}", query_paper.title);
            for (p, score) in &similar {
                println!("  {:.3}  {}", score, p.title);
            }
        } else {
            // Use S2 recommendations API
            let papers = sim_client.recommendations(&paper_id, args.limit).await?;
            println!("Papers similar to S2 ID: {}", paper_id);
            for p in &papers {
                let cites = p.citation_count.map(|c| format!("{}", c)).unwrap_or_else(|| "-".to_string());
                println!("  [{:>6}] {}", cites, p.title);
            }
        }
    } else if args.from_library {
        println!("Library-wide recommendations: run with a specific paper_id for TF-IDF similarity.");
    } else {
        anyhow::bail!("Provide a paper_id or use --from-library");
    }
    Ok(())
}

async fn dispatch_download(
    args: DownloadArgs,
    client: &reqwest::Client,
    config: &Config,
) -> anyhow::Result<()> {
    let base_dir = args.dir
        .or_else(|| {
            let path = &config.output.default_export_path;
            Some(PathBuf::from(path.replace('~', &std::env::var("HOME").unwrap_or_default())))
        })
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("papers")
        });

    let downloader = download::PdfDownloader::new(client.clone(), base_dir);
    let db = db::Database::open_default()?;

    if let Some(paper_id) = args.paper_id {
        let paper = db.get_paper_by_id(&paper_id)?
            .or_else(|| {
                if let Some((src, sid)) = paper_id.split_once(':') {
                    db.get_paper_by_source(src, sid).ok().flatten()
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow::anyhow!("Paper {} not found in library", paper_id))?;

        let path = downloader.download(&paper, Some(&db)).await?;
        println!("Downloaded to: {}", path.display());
    } else if args.all {
        eprintln!("No cached papers available for bulk download. Run a search first.");
    } else {
        anyhow::bail!("Provide a paper_id or use --all");
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

    // Auto-save to local library (silent, best-effort)
    if let Ok(lib_db) = db::Database::open_default() {
        for p in &all_papers {
            let _ = lib_db.upsert_paper(p);
        }
    }

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
    dedup::fuzzy_dedup(papers)
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
            // Auto-save to library (silent, best-effort)
            if let Ok(lib_db) = db::Database::open_default() {
                for p in &papers {
                    let _ = lib_db.upsert_paper(p);
                }
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
        KeyCode::Char('i') => {
            // Save selected paper to library
            if let Some(paper) = app.selected_paper() {
                let paper = paper.clone();
                if let Ok(lib_db) = db::Database::open_default() {
                    match lib_db.upsert_paper(&paper) {
                        Ok(_) => app.status_message = format!("✓ Saved to library: {}", app::truncate(&paper.title, 40)),
                        Err(e) => app.status_message = format!("Library error: {}", e),
                    }
                }
            }
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
