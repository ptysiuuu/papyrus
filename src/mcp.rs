use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    ServiceExt,
    handler::server::wrapper::{Json, Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars::{self, JsonSchema},
    tool, tool_router,
    transport::io::stdio,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::cache::DiskCache;
use crate::citation_graph::{CitationGraphClient, CitationGraphStore};
use crate::db::Database;
use crate::export::{export_papers, ExportFormat};
use crate::filters::{FilterSet, parse_flexible_date_pub};
use crate::models::{Paper, PaperSourceKind};
use crate::scraper::PaperSource;
use crate::similarity::{SimilarityClient, TfIdfIndex};

// ─── Input / output types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SearchInput {
    #[schemars(description = "Full-text keyword query")]
    pub query: Option<String>,
    #[schemars(description = "Search within titles only")]
    pub title: Option<String>,
    #[schemars(description = "Search within abstracts only")]
    pub abstract_text: Option<String>,
    #[schemars(description = "Filter by author names (repeatable)")]
    pub authors: Option<Vec<String>>,
    #[schemars(description = "Subject categories e.g. cs.AI, physics")]
    pub categories: Option<Vec<String>>,
    #[schemars(description = "Filter by journal name")]
    pub journal: Option<String>,
    #[schemars(description = "Fetch by DOI")]
    pub doi: Option<String>,
    #[schemars(description = "Fetch by arXiv ID e.g. 2301.07041")]
    pub arxiv_id: Option<String>,
    #[schemars(description = "Published on or after (YYYY, YYYY-MM, or YYYY-MM-DD)")]
    pub date_from: Option<String>,
    #[schemars(description = "Published on or before (YYYY, YYYY-MM, or YYYY-MM-DD)")]
    pub date_to: Option<String>,
    #[schemars(description = "Minimum citation count")]
    pub min_citations: Option<u32>,
    #[schemars(description = "Maximum citation count")]
    pub max_citations: Option<u32>,
    #[schemars(description = "Only papers with a freely accessible PDF")]
    pub has_pdf: Option<bool>,
    #[schemars(description = "Only papers linked to a code repository")]
    pub has_code: Option<bool>,
    #[schemars(description = "Exclude preprints")]
    pub peer_reviewed: Option<bool>,
    #[schemars(description = "Only preprints")]
    pub preprint_only: Option<bool>,
    #[schemars(description = "Only open-access papers")]
    pub open_access: Option<bool>,
    #[schemars(description = "Sources: arxiv, semantic, pubmed, crossref, all")]
    pub sources: Option<Vec<String>>,
    #[schemars(description = "Sort order: relevance (default), date-desc, date-asc, citations-desc")]
    pub sort: Option<String>,
    #[schemars(description = "Max results per source (default 20, max 500)")]
    pub limit: Option<u32>,
    #[schemars(description = "Skip first N results")]
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchOutput {
    pub papers: Vec<Paper>,
    pub total: usize,
    pub sources_hit: Vec<String>,
    pub sources_degraded: Vec<String>,
    pub cached: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct GetPaperOutput {
    pub paper: Option<Paper>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPaperInput {
    #[schemars(description = "DOI of the paper")]
    pub doi: Option<String>,
    #[schemars(description = "arXiv ID e.g. 2301.07041")]
    pub arxiv_id: Option<String>,
    #[schemars(description = "PubMed ID (PMID)")]
    pub pubmed_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportInput {
    #[schemars(description = "Paper IDs from a previous search_papers call")]
    pub paper_ids: Vec<String>,
    #[schemars(description = "Export format: json, csv, or bibtex")]
    pub format: String,
    #[schemars(description = "Output path; if omitted, a temp file is created")]
    pub path: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ExportOutput {
    pub path: String,
    pub count: usize,
}

// ─── New tool I/O types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExploreCitationsInput {
    #[schemars(description = "Semantic Scholar paper ID")]
    pub paper_id: String,
    #[schemars(description = "Traversal direction: 'ancestors' (references) or 'descendants' (citing papers)")]
    pub direction: Option<String>,
    #[schemars(description = "How many hops to traverse (default 2, max 5)")]
    pub depth: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CitationNode {
    pub s2id: String,
    pub title: String,
    pub citation_count: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ExploreCitationsOutput {
    pub nodes: Vec<CitationNode>,
    pub count: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LiteratureReviewInput {
    #[schemars(description = "Research topic to survey")]
    pub topic: String,
    #[schemars(description = "Max papers to return (default 20)")]
    pub limit: Option<u32>,
    #[schemars(description = "Sources: arxiv, semantic, pubmed, crossref, all")]
    pub sources: Option<Vec<String>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LiteratureReviewOutput {
    pub papers: Vec<Paper>,
    pub total: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckWatchesInput {
    #[schemars(description = "If true, also mark results as seen (updates last_run)")]
    pub mark_seen: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WatchResult {
    pub watch_id: String,
    pub watch_name: Option<String>,
    pub query: String,
    pub new_papers: Vec<Paper>,
    pub new_count: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CheckWatchesOutput {
    pub results: Vec<WatchResult>,
    pub total_new: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimilarPapersInput {
    #[schemars(description = "Semantic Scholar paper ID to find recommendations for")]
    pub paper_id: String,
    #[schemars(description = "Max results (default 10)")]
    pub limit: Option<u32>,
    #[schemars(description = "Use local TF-IDF instead of S2 API")]
    pub offline: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SimilarPapersOutput {
    pub papers: Vec<Paper>,
    pub total: usize,
}

// ─── Server ──────────────────────────────────────────────────────────────────

pub struct PapyrusMcp {
    sources: Vec<Arc<dyn PaperSource>>,
    disk_cache: Option<Arc<DiskCache>>,
    paper_store: Arc<Mutex<HashMap<String, Paper>>>,
    db: Arc<Mutex<Option<Database>>>,
    s2_api_key: Option<String>,
}

impl PapyrusMcp {
    pub fn new(sources: Vec<Arc<dyn PaperSource>>, disk_cache: Option<Arc<DiskCache>>) -> Self {
        Self::with_config(sources, disk_cache, None)
    }

    pub fn with_config(
        sources: Vec<Arc<dyn PaperSource>>,
        disk_cache: Option<Arc<DiskCache>>,
        s2_api_key: Option<String>,
    ) -> Self {
        let db = Database::open_default().ok();
        Self {
            sources,
            disk_cache,
            paper_store: Arc::new(Mutex::new(HashMap::new())),
            db: Arc::new(Mutex::new(db)),
            s2_api_key,
        }
    }

    async fn do_search(&self, fs: FilterSet) -> anyhow::Result<SearchOutput> {
        let mut all_papers: Vec<Paper> = Vec::new();
        let mut sources_hit: Vec<String> = Vec::new();
        let mut sources_degraded: Vec<String> = Vec::new();
        let mut any_cached = false;

        // Map PaperSourceKind variants to the scraper name() strings
        fn source_name(kind: &PaperSourceKind) -> &'static str {
            match kind {
                PaperSourceKind::Arxiv => "arXiv",
                PaperSourceKind::SemanticScholar => "Semantic Scholar",
                PaperSourceKind::PubMed => "PubMed",
                PaperSourceKind::CrossRef => "CrossRef",
            }
        }
        let requested: std::collections::HashSet<&str> = fs.sources
            .iter()
            .map(source_name)
            .collect();
        let active: Vec<_> = self.sources
            .iter()
            .filter(|s| requested.is_empty() || requested.contains(s.name()))
            .collect();

        for source in active {
            let name = source.name();
            let cache_key = DiskCache::cache_key(&fs, name);

            if let Some(ref dc) = self.disk_cache {
                if let Some((papers, _)) = dc.get(&cache_key) {
                    sources_hit.push(name.to_string());
                    all_papers.extend(papers);
                    any_cached = true;
                    continue;
                }
            }

            match source.fetch(&fs, 0).await {
                Ok(result) => {
                    if let Some(ref dc) = self.disk_cache {
                        let _ = dc.put(&cache_key, &result.papers, result.total_count);
                    }
                    sources_hit.push(name.to_string());
                    all_papers.extend(result.papers);
                }
                Err(e) => {
                    sources_degraded.push(format!("{}: {}", name, e));
                }
            }
        }

        let mut seen = std::collections::HashSet::new();
        all_papers.retain(|p| seen.insert(p.dedup_key()));

        // Store in paper store for later export
        {
            let mut store = self.paper_store.lock().await;
            for paper in &all_papers {
                store.insert(paper.id.clone(), paper.clone());
            }
        }

        Ok(SearchOutput {
            total: all_papers.len(),
            papers: all_papers,
            sources_hit,
            sources_degraded,
            cached: any_cached,
        })
    }
}

fn build_filter_set(input: SearchInput) -> anyhow::Result<FilterSet> {
    let mut fs = FilterSet::default();
    fs.query = input.query;
    fs.title_query = input.title;
    fs.abstract_query = input.abstract_text;
    fs.authors = input.authors.unwrap_or_default();
    fs.categories = input.categories.unwrap_or_default();
    fs.journal = input.journal;
    fs.doi = input.doi;
    fs.arxiv_id = input.arxiv_id;
    fs.min_citations = input.min_citations;
    fs.max_citations = input.max_citations;
    fs.has_pdf = input.has_pdf.unwrap_or(false);
    fs.has_code = input.has_code.unwrap_or(false);
    fs.peer_reviewed_only = input.peer_reviewed.unwrap_or(false);
    fs.preprint_only = input.preprint_only.unwrap_or(false);
    fs.open_access_only = input.open_access.unwrap_or(false);
    fs.limit = input.limit.unwrap_or(20).min(500);
    fs.offset = input.offset.unwrap_or(0);

    if let Some(s) = input.sort {
        fs.sort = s.parse()?;
    }
    if let Some(from_str) = input.date_from {
        fs.date_from = parse_flexible_date_pub(&from_str);
    }
    if let Some(to_str) = input.date_to {
        fs.date_to = parse_flexible_date_pub(&to_str);
    }
    if let Some(srcs) = input.sources {
        if !srcs.is_empty() {
            let mut parsed = Vec::new();
            for s in &srcs {
                parsed.extend(parse_source(s)?);
            }
            fs.sources = parsed;
        }
    }
    Ok(fs)
}

fn parse_source(s: &str) -> anyhow::Result<Vec<PaperSourceKind>> {
    match s.to_lowercase().as_str() {
        "arxiv" => Ok(vec![PaperSourceKind::Arxiv]),
        "semantic" | "semantic_scholar" | "s2" => Ok(vec![PaperSourceKind::SemanticScholar]),
        "pubmed" => Ok(vec![PaperSourceKind::PubMed]),
        "crossref" => Ok(vec![PaperSourceKind::CrossRef]),
        "all" => Ok(vec![
            PaperSourceKind::Arxiv,
            PaperSourceKind::SemanticScholar,
            PaperSourceKind::PubMed,
            PaperSourceKind::CrossRef,
        ]),
        other => Err(anyhow::anyhow!("Unknown source: {}", other)),
    }
}

#[tool_router]
impl PapyrusMcp {
    #[tool(description = "Search academic papers from arXiv, Semantic Scholar, PubMed, and CrossRef. Returns papers with metadata including title, authors, abstract, DOI, and PDF link.")]
    async fn search_papers(
        &self,
        Parameters(input): Parameters<SearchInput>,
    ) -> Result<Json<SearchOutput>, rmcp::ErrorData> {
        let fs = build_filter_set(input)
            .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
        self.do_search(fs)
            .await
            .map(Json)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))
    }

    #[tool(description = "Fetch a single paper by DOI, arXiv ID, or PubMed ID. At least one identifier is required.")]
    async fn get_paper(
        &self,
        Parameters(input): Parameters<GetPaperInput>,
    ) -> Result<Json<GetPaperOutput>, rmcp::ErrorData> {
        if input.doi.is_none() && input.arxiv_id.is_none() && input.pubmed_id.is_none() {
            return Err(rmcp::ErrorData::invalid_params(
                "at least one of doi, arxiv_id, or pubmed_id is required",
                None,
            ));
        }

        let mut fs = FilterSet::default();
        fs.limit = 1;

        // Scope sources to only those that understand the given identifier.
        // arXiv uses id_list (not DOI); PubMed uses PMID; CrossRef/S2 resolve DOIs.
        if let Some(ref arxiv_id) = input.arxiv_id {
            fs.arxiv_id = Some(arxiv_id.clone());
            fs.sources = vec![PaperSourceKind::Arxiv];
        } else if let Some(ref pubmed_id) = input.pubmed_id {
            // PubMed ID: pass as a query to PubMed only
            fs.query = Some(pubmed_id.clone());
            fs.sources = vec![PaperSourceKind::PubMed];
        } else if let Some(ref doi) = input.doi {
            fs.doi = Some(doi.clone());
            fs.sources = vec![PaperSourceKind::SemanticScholar, PaperSourceKind::CrossRef];
        }

        let out = self
            .do_search(fs)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(GetPaperOutput { paper: out.papers.into_iter().next() }))
    }

    #[tool(description = "Export papers to JSON, CSV, or BibTeX. Use paper IDs from a previous search_papers call. Returns the file path written.")]
    async fn export_papers(
        &self,
        Parameters(input): Parameters<ExportInput>,
    ) -> Result<Json<ExportOutput>, rmcp::ErrorData> {
        let fmt = ExportFormat::from_str(&input.format)
            .ok_or_else(|| rmcp::ErrorData::invalid_params(
                format!("unknown format '{}'; use json, csv, or bibtex", input.format),
                None,
            ))?;

        let store = self.paper_store.lock().await;
        let papers: Vec<Paper> = input
            .paper_ids
            .iter()
            .filter_map(|id| store.get(id).cloned())
            .collect();
        drop(store);

        let path = if let Some(p) = input.path {
            PathBuf::from(p)
        } else {
            let ext = match fmt {
                ExportFormat::Json => "json",
                ExportFormat::Csv => "csv",
                ExportFormat::BibTeX => "bib",
            };
            std::env::temp_dir().join(format!("papyrus_export_{}.{}", uuid::Uuid::new_v4(), ext))
        };

        let mut file = std::fs::File::create(&path)
            .map_err(|e| rmcp::ErrorData::internal_error(format!("create {:?}: {}", path, e), None))?;
        export_papers(&papers, &fmt, &mut file)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        file.flush()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ExportOutput {
            count: papers.len(),
            path: path.to_string_lossy().into_owned(),
        }))
    }

    #[tool(description = "Explore the citation graph for a paper. Fetch ancestors (papers it references) or descendants (papers that cite it). Requires the paper to have been indexed with 'cite-graph fetch' first, or will auto-fetch from Semantic Scholar.")]
    async fn explore_citations(
        &self,
        Parameters(input): Parameters<ExploreCitationsInput>,
    ) -> Result<Json<ExploreCitationsOutput>, rmcp::ErrorData> {
        let depth = input.depth.unwrap_or(2).min(5);
        let direction = input.direction.as_deref().unwrap_or("ancestors");

        let db = Database::open_default()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        // Auto-fetch from S2 if not in graph
        let store = CitationGraphStore::new(db);
        let has_refs = store.get_references(&input.paper_id)
            .map(|r| !r.is_empty())
            .unwrap_or(false);

        if !has_refs {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default();
            let graph_client = CitationGraphClient::new(client, self.s2_api_key.clone());
            let _ = graph_client.fetch_and_store_references(&input.paper_id, &store, 100).await;
            let _ = graph_client.fetch_and_store_citations(&input.paper_id, &store, 100).await;
        }

        let nodes = match direction {
            "descendants" => store.descendants(&input.paper_id, depth),
            _ => store.ancestors(&input.paper_id, depth),
        }
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let result: Vec<CitationNode> = nodes
            .into_iter()
            .map(|n| CitationNode { s2id: n.s2id, title: n.title, citation_count: n.citation_count })
            .collect();
        let count = result.len();
        Ok(Json(ExploreCitationsOutput { nodes: result, count }))
    }

    #[tool(description = "Run a literature review: multi-source search, deduplicate, rank by citations, return top papers with metadata. A single call to survey a research topic.")]
    async fn literature_review(
        &self,
        Parameters(input): Parameters<LiteratureReviewInput>,
    ) -> Result<Json<LiteratureReviewOutput>, rmcp::ErrorData> {
        let mut fs = FilterSet::default();
        fs.query = Some(input.topic.clone());
        fs.limit = input.limit.unwrap_or(20).min(100);
        if let Some(srcs) = input.sources {
            if !srcs.is_empty() {
                let mut parsed = Vec::new();
                for s in &srcs {
                    if let Ok(p) = parse_source(s) {
                        parsed.extend(p);
                    }
                }
                if !parsed.is_empty() {
                    fs.sources = parsed;
                }
            }
        }

        let out = self.do_search(fs).await
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        // Sort by citation count descending
        let mut papers = out.papers;
        papers.sort_by(|a, b| b.citation_count.unwrap_or(0).cmp(&a.citation_count.unwrap_or(0)));

        let total = papers.len();
        Ok(Json(LiteratureReviewOutput { papers, total }))
    }

    #[tool(description = "Check all saved watch queries for new papers since last run. Returns new papers per watch.")]
    async fn check_watches(
        &self,
        Parameters(input): Parameters<CheckWatchesInput>,
    ) -> Result<Json<CheckWatchesOutput>, rmcp::ErrorData> {
        let mark_seen = input.mark_seen.unwrap_or(true);

        let db = Database::open_default()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let watches = db.list_watches()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let mut results = Vec::new();
        let mut total_new = 0;

        for w in &watches {
            let mut fs = FilterSet::default();
            fs.query = Some(w.query.clone());

            let out = self.do_search(fs).await.unwrap_or_else(|_| crate::mcp::SearchOutput {
                papers: Vec::new(),
                total: 0,
                sources_hit: Vec::new(),
                sources_degraded: Vec::new(),
                cached: false,
            });

            let db2 = Database::open_default()
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
            let runner = crate::watch::WatchRunner::new(db2);
            let new_papers = if mark_seen {
                runner.filter_new_papers(&w.id, &out.papers)
                    .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
            } else {
                // Dry run: check without marking
                let db3 = Database::open_default()
                    .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
                let mut new = Vec::new();
                for paper in &out.papers {
                    let key = format!("{}:{}", crate::db::source_kind_to_str(&paper.source), paper.source_id);
                    if !db3.was_watch_paper_seen(&w.id, &key)
                        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))? {
                        new.push(paper.clone());
                    }
                }
                new
            };

            if mark_seen {
                let _ = db.update_watch_last_run(&w.id);
            }

            let new_count = new_papers.len();
            total_new += new_count;

            results.push(WatchResult {
                watch_id: w.id.clone(),
                watch_name: w.name.clone(),
                query: w.query.clone(),
                new_papers,
                new_count,
            });
        }

        Ok(Json(CheckWatchesOutput { results, total_new }))
    }

    #[tool(description = "Find papers similar to a given Semantic Scholar paper ID, using the S2 recommendations API or offline TF-IDF against your local library.")]
    async fn similar_papers(
        &self,
        Parameters(input): Parameters<SimilarPapersInput>,
    ) -> Result<Json<SimilarPapersOutput>, rmcp::ErrorData> {
        let limit = input.limit.unwrap_or(10);
        let offline = input.offline.unwrap_or(false);

        if offline {
            let db = Database::open_default()
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
            let query = db.get_paper_by_id(&input.paper_id)
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
                .ok_or_else(|| rmcp::ErrorData::invalid_params(
                    format!("Paper {} not found in local library", input.paper_id),
                    None,
                ))?;

            let all = db.list_papers(usize::MAX, 0)
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
            let index = TfIdfIndex::build(&all);
            let similar = index.find_similar(&query, limit as usize);
            let papers: Vec<Paper> = similar.into_iter().map(|(p, _)| p.clone()).collect();
            let total = papers.len();
            return Ok(Json(SimilarPapersOutput { papers, total }));
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        let sim_client = SimilarityClient::new(client, self.s2_api_key.clone());
        let papers = sim_client.recommendations(&input.paper_id, limit).await
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let total = papers.len();
        Ok(Json(SimilarPapersOutput { papers, total }))
    }
}

#[rmcp::tool_handler]
impl rmcp::ServerHandler for PapyrusMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("papyrus", env!("CARGO_PKG_VERSION"))
                    .with_description("Terminal research paper scraper — arXiv, Semantic Scholar, PubMed, CrossRef"),
            )
            .with_instructions(
                "Search academic papers across multiple sources. \
                 Call search_papers to find papers; use the returned paper IDs \
                 with export_papers to save results. Supports filtering by date, \
                 citations, open-access status, and more.",
            )
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

pub async fn run_mcp_server(
    sources: Vec<Arc<dyn PaperSource>>,
    disk_cache: Option<Arc<DiskCache>>,
) -> anyhow::Result<()> {
    run_mcp_server_with_key(sources, disk_cache, None).await
}

pub async fn run_mcp_server_with_key(
    sources: Vec<Arc<dyn PaperSource>>,
    disk_cache: Option<Arc<DiskCache>>,
    s2_api_key: Option<String>,
) -> anyhow::Result<()> {
    let server = PapyrusMcp::with_config(sources, disk_cache, s2_api_key);
    let running = server.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
