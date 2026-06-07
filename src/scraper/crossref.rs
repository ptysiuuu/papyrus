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

pub struct CrossRefSource {
    client: Client,
    polite_email: Option<String>,
    limiter: Arc<Limiter>,
}

impl CrossRefSource {
    pub fn new(client: Client, polite_email: Option<String>) -> Self {
        Self { client, polite_email, limiter: ratelimit::crossref() }
    }
}

#[derive(Debug, Deserialize)]
struct CrResponse {
    message: CrMessage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CrMessage {
    items: Vec<CrWork>,
    total_results: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CrWork {
    #[serde(rename = "DOI")]
    doi: Option<String>,
    title: Option<Vec<String>>,
    author: Option<Vec<CrAuthor>>,
    published: Option<CrDate>,
    published_print: Option<CrDate>,
    published_online: Option<CrDate>,
    #[serde(rename = "container-title")]
    container_title: Option<Vec<String>>,
    #[serde(rename = "type")]
    work_type: Option<String>,
    is_referenced_by_count: Option<u32>,
    references_count: Option<u32>,
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    link: Option<Vec<CrLink>>,
    #[serde(rename = "URL")]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrAuthor {
    given: Option<String>,
    family: Option<String>,
    #[serde(rename = "ORCID")]
    orcid: Option<String>,
    affiliation: Option<Vec<CrAffiliation>>,
}

#[derive(Debug, Deserialize)]
struct CrAffiliation {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrDate {
    #[serde(rename = "date-parts")]
    date_parts: Vec<Vec<Option<u32>>>,
}

#[derive(Debug, Deserialize)]
struct CrLink {
    #[serde(rename = "content-type")]
    content_type: String,
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "intended-application")]
    intended_application: Option<String>,
}

fn cr_work_to_paper(w: CrWork) -> Option<Paper> {
    let title = w.title?.into_iter().next()?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    let doi = w.doi.clone().unwrap_or_default();
    let source_id = if doi.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        doi.clone()
    };

    let mut paper = Paper::new(PaperSourceKind::CrossRef, source_id, &title);
    paper.doi = if doi.is_empty() { None } else { Some(doi.clone()) };

    paper.authors = w
        .author
        .unwrap_or_default()
        .into_iter()
        .map(|a| {
            let name = match (a.given, a.family) {
                (Some(g), Some(f)) => format!("{} {}", g, f),
                (None, Some(f)) => f,
                (Some(g), None) => g,
                _ => String::new(),
            };
            let affiliation = a
                .affiliation
                .and_then(|affs| affs.into_iter().find_map(|a| a.name));
            Author {
                name,
                affiliation,
                orcid: a.orcid,
            }
        })
        .filter(|a| !a.name.is_empty())
        .collect();

    // Resolve date from multiple possible fields
    let date = w
        .published
        .as_ref()
        .or(w.published_print.as_ref())
        .or(w.published_online.as_ref());
    if let Some(d) = date {
        if let Some(parts) = d.date_parts.first() {
            let year = parts.first().and_then(|v| *v).unwrap_or(0) as i32;
            let month = parts.get(1).and_then(|v| *v).unwrap_or(1);
            let day = parts.get(2).and_then(|v| *v).unwrap_or(1);
            paper.published_date = NaiveDate::from_ymd_opt(year, month, day);
        }
    }

    paper.journal = w
        .container_title
        .and_then(|ct| ct.into_iter().next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    paper.citation_count = w.is_referenced_by_count;
    paper.reference_count = w.references_count;

    // Strip XML tags from abstract (CrossRef wraps in <jats:...>)
    paper.abstract_text = w.abstract_text.map(|a| strip_jats_tags(&a));

    // PDF link
    if let Some(links) = w.link {
        for link in links {
            if link.content_type == "application/pdf"
                || link.intended_application.as_deref() == Some("text-mining")
            {
                paper.pdf_url = Some(link.url);
                paper.is_open_access = true;
                break;
            }
        }
    }

    paper.html_url = w.url.or_else(|| {
        paper.doi.as_ref().map(|d| format!("https://doi.org/{}", d))
    });

    let work_type = w.work_type.unwrap_or_default();
    paper.is_peer_reviewed = matches!(
        work_type.as_str(),
        "journal-article" | "proceedings-article" | "book-chapter"
    );

    Some(paper)
}

fn strip_jats_tags(s: &str) -> String {
    // CrossRef abstracts use JATS XML. Three special cases:
    //   jats:tex-math  — raw LaTeX; apply clean_latex and emit
    //   mml:math       — MathML duplicate of tex-math; skip entirely
    //   jats:sup/sub   — superscript/subscript; convert to Unicode
    // Everything else: strip the tag, emit the text content as-is.
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut mml_depth: usize = 0;
    let mut tex_buf: Option<String> = None;
    let mut sup_buf: Option<String> = None;
    let mut sub_buf: Option<String> = None;

    while let Some(c) = chars.next() {
        if c != '<' {
            match (mml_depth, &mut tex_buf, &mut sup_buf, &mut sub_buf) {
                (0, Some(ref mut b), _, _) => b.push(c),
                (0, _, Some(ref mut b), _) => b.push(c),
                (0, _, _, Some(ref mut b)) => b.push(c),
                (0, _, _, _) => out.push(c),
                _ => {} // inside mml:math — skip
            }
            continue;
        }

        // Read the full tag into a buffer
        let mut tag = String::new();
        for tc in chars.by_ref() {
            if tc == '>' { break; }
            tag.push(tc);
        }
        let tag = tag.trim();
        let is_close = tag.starts_with('/');
        let name_part = if is_close { &tag[1..] } else { tag };
        let tag_name = name_part.split_whitespace().next().unwrap_or("").trim_end_matches('/');

        match tag_name {
            "mml:math" | "math" => {
                if !is_close { mml_depth += 1; }
                else if mml_depth > 0 { mml_depth -= 1; }
            }
            "jats:tex-math" | "tex-math" => {
                if !is_close {
                    tex_buf = Some(String::new());
                } else if let Some(buf) = tex_buf.take() {
                    out.push_str(&super::clean_latex(&buf));
                }
            }
            "jats:sup" | "sup" => {
                if !is_close {
                    sup_buf = Some(String::new());
                } else if let Some(buf) = sup_buf.take() {
                    for gc in buf.chars() {
                        out.push(super::to_superscript(gc).unwrap_or(gc));
                    }
                }
            }
            "jats:sub" | "sub" => {
                if !is_close {
                    sub_buf = Some(String::new());
                } else if let Some(buf) = sub_buf.take() {
                    for gc in buf.chars() {
                        out.push(super::to_subscript(gc).unwrap_or(gc));
                    }
                }
            }
            _ => {} // all other tags: stripped, content emitted normally
        }
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[async_trait]
impl PaperSource for CrossRefSource {
    fn name(&self) -> &'static str {
        "CrossRef"
    }

    async fn fetch(&self, filters: &FilterSet, page: u32) -> anyhow::Result<SearchResult> {
        if filters.query.is_none() && filters.doi.is_none() && filters.title_query.is_none() {
            return Ok(SearchResult {
                papers: vec![],
                total_count: Some(0),
                source: PaperSourceKind::CrossRef,
            });
        }

        // DOI lookup
        if let Some(doi) = &filters.doi {
            let url = format!("https://api.crossref.org/works/{}", urlencoding(doi));
            ratelimit::throttle(&self.limiter).await;
            let resp = self.client.get(&url).send().await?;
            if resp.status().is_success() {
                #[derive(Deserialize)]
                struct SingleWork {
                    message: CrWork,
                }
                let body: SingleWork = resp.json().await?;
                let papers = cr_work_to_paper(body.message).into_iter().collect();
                return Ok(SearchResult {
                    papers,
                    total_count: Some(1),
                    source: PaperSourceKind::CrossRef,
                });
            }
        }

        let offset = filters.offset + page * filters.limit;
        let mut params: Vec<String> = Vec::new();

        if let Some(q) = &filters.query {
            let search_q = super::expand_chemical_formula(q).unwrap_or_else(|| q.clone());
            params.push(format!("query.bibliographic={}", urlencoding(&search_q)));
        }
        if let Some(t) = &filters.title_query {
            params.push(format!("query.title={}", urlencoding(t)));
        }
        for author in &filters.authors {
            params.push(format!("query.author={}", urlencoding(author)));
        }

        params.push(format!("rows={}", filters.limit));
        params.push(format!("offset={}", offset));

        let mut filter_parts: Vec<String> = vec![
            // Restrict to academic document types; excludes books, films, artworks, etc.
            "type:journal-article".to_string(),
            "type:proceedings-article".to_string(),
            "type:book-chapter".to_string(),
        ];
        if let Some(from) = filters.date_from {
            filter_parts.push(format!("from-pub-date:{}", from.format("%Y-%m-%d")));
        }
        if let Some(to) = filters.date_to {
            filter_parts.push(format!("until-pub-date:{}", to.format("%Y-%m-%d")));
        }
        if filters.has_pdf || filters.open_access_only {
            filter_parts.push("has-full-text:true".to_string());
        }
        params.push(format!("filter={}", filter_parts.join(",")));

        // Sort
        match filters.sort {
            SortOrder::DateDesc => {
                params.push("sort=published&order=desc".to_string());
            }
            SortOrder::DateAsc => {
                params.push("sort=published&order=asc".to_string());
            }
            SortOrder::CitationsDesc => {
                params.push("sort=is-referenced-by-count&order=desc".to_string());
            }
            _ => {
                params.push("sort=relevance&order=desc".to_string());
            }
        }

        if let Some(email) = &self.polite_email {
            if !email.is_empty() {
                params.push(format!("mailto={}", urlencoding(email)));
            }
        }

        let url = format!("https://api.crossref.org/works?{}", params.join("&"));
        ratelimit::throttle(&self.limiter).await;
        let resp = self.client.get(&url).send().await?;
        if resp.status().as_u16() == 429 {
            let retry = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(5);
            return Err(PapyrusError::RateLimited {
                src: "CrossRef".to_string(),
                retry_after_secs: retry,
            }
            .into());
        }
        let body: CrResponse = resp.json().await?;

        let total = body.message.total_results;
        let mut papers: Vec<Paper> = body
            .message
            .items
            .into_iter()
            .filter_map(cr_work_to_paper)
            .collect();

        if let Some(min) = filters.min_citations {
            papers.retain(|p| p.citation_count.unwrap_or(0) >= min);
        }
        if let Some(max) = filters.max_citations {
            papers.retain(|p| p.citation_count.unwrap_or(u32::MAX) <= max);
        }

        Ok(SearchResult {
            papers,
            total_count: total,
            source: PaperSourceKind::CrossRef,
        })
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
