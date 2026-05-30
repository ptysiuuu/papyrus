use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use reqwest::Client;
use serde::Deserialize;

use crate::error::PapyrusError;
use crate::filters::{FilterSet, SortOrder};
use crate::models::{Author, Paper, PaperSourceKind, SearchResult};
use crate::ratelimit::{self, Limiter};

use super::PaperSource;

pub struct SemanticScholarSource {
    client: Client,
    api_key: Option<String>,
    limiter: Arc<Limiter>,
}

impl SemanticScholarSource {
    pub fn new(client: Client, api_key: Option<String>) -> Self {
        let limiter = if api_key.as_deref().map_or(false, |k| !k.is_empty()) {
            ratelimit::semantic_keyed()
        } else {
            ratelimit::semantic_unkeyed()
        };
        Self { client, api_key, limiter }
    }
}

const S2_FIELDS: &str =
    "paperId,title,authors,year,abstract,citationCount,referenceCount,isOpenAccess,openAccessPdf,externalIds,publicationTypes,journal,publicationDate";

#[derive(Debug, Deserialize)]
struct S2Response {
    data: Vec<S2Paper>,
    total: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct S2Paper {
    paper_id: String,
    title: Option<String>,
    authors: Option<Vec<S2Author>>,
    year: Option<i32>,
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    citation_count: Option<u32>,
    reference_count: Option<u32>,
    is_open_access: Option<bool>,
    open_access_pdf: Option<S2Pdf>,
    external_ids: Option<S2ExternalIds>,
    publication_types: Option<Vec<String>>,
    journal: Option<S2Journal>,
    publication_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2Author {
    name: String,
}

#[derive(Debug, Deserialize)]
struct S2Pdf {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct S2ExternalIds {
    #[serde(rename = "DOI")]
    doi: Option<String>,
    #[serde(rename = "ArXiv")]
    arxiv: Option<String>,
    pub_med: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2Journal {
    name: Option<String>,
}

fn s2_paper_to_paper(s: S2Paper) -> Paper {
    let title = s.title.unwrap_or_else(|| "Untitled".to_string());
    let mut paper = Paper::new(PaperSourceKind::SemanticScholar, s.paper_id, &title);

    paper.authors = s
        .authors
        .unwrap_or_default()
        .into_iter()
        .map(|a| Author {
            name: a.name,
            affiliation: None,
            orcid: None,
        })
        .collect();

    paper.abstract_text = s.abstract_text;
    paper.citation_count = s.citation_count;
    paper.reference_count = s.reference_count;
    paper.is_open_access = s.is_open_access.unwrap_or(false);
    paper.pdf_url = s.open_access_pdf.and_then(|p| p.url);

    if let Some(ext) = s.external_ids {
        paper.doi = ext.doi;
        paper.arxiv_id = ext.arxiv.clone();
        if let Some(arxiv_id) = &ext.arxiv {
            if paper.pdf_url.is_none() {
                paper.pdf_url = Some(format!("https://arxiv.org/pdf/{}", arxiv_id));
            }
            paper.html_url = Some(format!("https://arxiv.org/abs/{}", arxiv_id));
        }
    }

    if let Some(journal) = s.journal {
        paper.journal = journal.name;
    }

    // Parse date
    if let Some(date_str) = s.publication_date {
        paper.published_date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").ok();
    }
    if paper.published_date.is_none() {
        if let Some(year) = s.year {
            paper.published_date = NaiveDate::from_ymd_opt(year, 1, 1);
        }
    }

    let pub_types = s.publication_types.unwrap_or_default();
    paper.is_peer_reviewed = pub_types
        .iter()
        .any(|t| t.eq_ignore_ascii_case("JournalArticle") || t.eq_ignore_ascii_case("Conference"));

    paper
}

#[async_trait]
impl PaperSource for SemanticScholarSource {
    fn name(&self) -> &'static str {
        "Semantic Scholar"
    }

    async fn fetch(&self, filters: &FilterSet, page: u32) -> anyhow::Result<SearchResult> {
        let query = filters
            .query
            .clone()
            .or_else(|| filters.title_query.clone())
            .unwrap_or_default();

        if query.is_empty() && filters.doi.is_none() && filters.arxiv_id.is_none() {
            return Ok(SearchResult {
                papers: vec![],
                total_count: Some(0),
                source: PaperSourceKind::SemanticScholar,
            });
        }

        let offset = filters.offset + page * filters.limit;
        let mut url = format!(
            "https://api.semanticscholar.org/graph/v1/paper/search?query={}&fields={}&limit={}&offset={}",
            urlencoding(query.trim()),
            S2_FIELDS,
            filters.limit,
            offset,
        );

        // Year filter
        if let (Some(from), Some(to)) = (filters.date_from, filters.date_to) {
            url.push_str(&format!("&year={}-{}", from.format("%Y"), to.format("%Y")));
        } else if let Some(from) = filters.date_from {
            url.push_str(&format!("&year={}-", from.format("%Y")));
        }

        if filters.open_access_only {
            url.push_str("&openAccessPdf=true");
        }

        let mut req = self.client.get(&url);
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }

        ratelimit::throttle(&self.limiter).await;
        let resp = req.send().await?;
        if resp.status().as_u16() == 429 {
            let retry = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(5);
            return Err(PapyrusError::RateLimited {
                src: "Semantic Scholar".to_string(),
                retry_after_secs: retry,
            }
            .into());
        }
        let body: S2Response = resp.json().await?;

        let mut papers: Vec<Paper> = body.data.into_iter().map(s2_paper_to_paper).collect();

        // Post-fetch filters
        if filters.has_pdf {
            papers.retain(|p| p.pdf_url.is_some());
        }
        if let Some(min) = filters.min_citations {
            papers.retain(|p| p.citation_count.unwrap_or(0) >= min);
        }
        if let Some(max) = filters.max_citations {
            papers.retain(|p| p.citation_count.unwrap_or(u32::MAX) <= max);
        }
        if filters.peer_reviewed_only {
            papers.retain(|p| p.is_peer_reviewed);
        }
        if !filters.sort.eq(&SortOrder::Relevance) {
            sort_papers(&mut papers, &filters.sort);
        }

        Ok(SearchResult {
            papers,
            total_count: body.total,
            source: PaperSourceKind::SemanticScholar,
        })
    }
}

fn sort_papers(papers: &mut Vec<Paper>, sort: &SortOrder) {
    match sort {
        SortOrder::DateDesc => papers.sort_by(|a, b| b.published_date.cmp(&a.published_date)),
        SortOrder::DateAsc => papers.sort_by(|a, b| a.published_date.cmp(&b.published_date)),
        SortOrder::CitationsDesc => {
            papers.sort_by(|a, b| b.citation_count.unwrap_or(0).cmp(&a.citation_count.unwrap_or(0)))
        }
        _ => {}
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            other => {
                out.push('%');
                out.push(char::from_digit((other >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit((other & 0xf) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}
