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
use crate::export::{export_papers, ExportFormat};
use crate::filters::{FilterSet, parse_flexible_date_pub};
use crate::models::{Paper, PaperSourceKind};
use crate::scraper::PaperSource;

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

// ─── Server ──────────────────────────────────────────────────────────────────

pub struct PapyrusMcp {
    sources: Vec<Arc<dyn PaperSource>>,
    disk_cache: Option<Arc<DiskCache>>,
    paper_store: Arc<Mutex<HashMap<String, Paper>>>,
}

impl PapyrusMcp {
    pub fn new(sources: Vec<Arc<dyn PaperSource>>, disk_cache: Option<Arc<DiskCache>>) -> Self {
        Self {
            sources,
            disk_cache,
            paper_store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn do_search(&self, fs: FilterSet) -> anyhow::Result<SearchOutput> {
        let mut all_papers: Vec<Paper> = Vec::new();
        let mut sources_hit: Vec<String> = Vec::new();
        let mut sources_degraded: Vec<String> = Vec::new();
        let mut any_cached = false;

        for source in &self.sources {
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
    ) -> Result<Json<Option<Paper>>, rmcp::ErrorData> {
        if input.doi.is_none() && input.arxiv_id.is_none() && input.pubmed_id.is_none() {
            return Err(rmcp::ErrorData::invalid_params(
                "at least one of doi, arxiv_id, or pubmed_id is required",
                None,
            ));
        }

        let mut fs = FilterSet::default();
        fs.doi = input.doi;
        fs.arxiv_id = input.arxiv_id;
        fs.limit = 1;

        let out = self
            .do_search(fs)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(out.papers.into_iter().next()))
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
    let server = PapyrusMcp::new(sources, disk_cache);
    let running = server.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
