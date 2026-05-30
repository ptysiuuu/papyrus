use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Client;

use crate::filters::{FilterSet, SortOrder};
use crate::models::{Author, Paper, PaperSourceKind, SearchResult};
use crate::ratelimit::{self, Limiter};

use super::PaperSource;

pub struct ArxivSource {
    client: Client,
    limiter: Arc<Limiter>,
}

impl ArxivSource {
    pub fn new(client: Client) -> Self {
        Self { client, limiter: ratelimit::arxiv() }
    }

    fn build_query(filters: &FilterSet) -> String {
        let mut parts: Vec<String> = Vec::new();

        if let Some(q) = &filters.query {
            parts.push(format!("all:{}", q));
        }
        if let Some(t) = &filters.title_query {
            parts.push(format!("ti:{}", t));
        }
        if let Some(a) = &filters.abstract_query {
            parts.push(format!("abs:{}", a));
        }
        for author in &filters.authors {
            parts.push(format!("au:{}", author));
        }
        for cat in &filters.categories {
            parts.push(format!("cat:{}", cat));
        }
        if let Some(arxiv_id) = &filters.arxiv_id {
            return arxiv_id.clone();
        }

        if parts.is_empty() {
            return String::from("all:*");
        }

        parts.join(" AND ")
    }
}

#[async_trait]
impl PaperSource for ArxivSource {
    fn name(&self) -> &'static str {
        "arXiv"
    }

    async fn fetch(&self, filters: &FilterSet, page: u32) -> anyhow::Result<SearchResult> {
        let start = filters.offset + page * filters.limit;

        // Direct ID lookup uses id_list parameter instead of search_query
        let url = if let Some(arxiv_id) = &filters.arxiv_id {
            format!(
                "http://export.arxiv.org/api/query?id_list={}&max_results=1",
                urlencoding::encode(arxiv_id)
            )
        } else {
            let query = Self::build_query(filters);
            let (sort_by, sort_order) = match &filters.sort {
                SortOrder::DateDesc => ("submittedDate", "descending"),
                SortOrder::DateAsc => ("submittedDate", "ascending"),
                SortOrder::CitationsDesc => ("relevance", "descending"),
                SortOrder::Relevance => ("relevance", "descending"),
            };
            format!(
                "http://export.arxiv.org/api/query?search_query={}&start={}&max_results={}&sortBy={}&sortOrder={}",
                urlencoding::encode(&query),
                start,
                filters.limit,
                sort_by,
                sort_order
            )
        };

        ratelimit::throttle(&self.limiter).await;
        let resp = self.client.get(&url).send().await?;
        if resp.status().as_u16() == 429 {
            let retry = retry_after(&resp);
            return Err(crate::error::PapyrusError::RateLimited {
                src: "arXiv".to_string(),
                retry_after_secs: retry,
            }
            .into());
        }
        let body = resp.text().await?;

        let papers = parse_arxiv_atom(&body)?;
        let total = extract_total_results(&body);

        let mut result = papers;

        // Apply post-fetch filters that the API doesn't support natively
        if filters.has_pdf {
            result.retain(|p| p.pdf_url.is_some());
        }
        if let Some(from) = filters.date_from {
            result.retain(|p| p.published_date.map_or(true, |d| d >= from));
        }
        if let Some(to) = filters.date_to {
            result.retain(|p| p.published_date.map_or(true, |d| d <= to));
        }

        Ok(SearchResult {
            papers: result,
            total_count: total,
            source: PaperSourceKind::Arxiv,
        })
    }
}

fn extract_total_results(xml: &str) -> Option<u64> {
    let marker = "<opensearch:totalResults>";
    let end_marker = "</opensearch:totalResults>";
    let start = xml.find(marker)? + marker.len();
    let end = xml[start..].find(end_marker)? + start;
    xml[start..end].trim().parse().ok()
}

fn parse_arxiv_atom(xml: &str) -> anyhow::Result<Vec<Paper>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut papers: Vec<Paper> = Vec::new();
    let mut current: Option<PaperBuilder> = None;
    let mut current_tag = String::new();
    let mut in_author = false;
    let mut author_name = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.local_name();
                let tag = std::str::from_utf8(name.as_ref()).unwrap_or("").to_string();
                current_tag = tag.clone();
                match tag.as_str() {
                    "entry" => {
                        current = Some(PaperBuilder::default());
                    }
                    "author" if current.is_some() => {
                        in_author = true;
                        author_name.clear();
                    }
                    "link" if current.is_some() => {
                        if let Some(ref mut p) = current {
                            let mut rel = String::new();
                            let mut href = String::new();
                            let mut title_attr = String::new();
                            for attr in e.attributes().flatten() {
                                let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                                let val = attr.unescape_value().unwrap_or_default().to_string();
                                match key {
                                    "rel" => rel = val,
                                    "href" => href = val,
                                    "title" => title_attr = val,
                                    _ => {}
                                }
                            }
                            if title_attr == "pdf" || rel == "related" && href.contains("pdf") {
                                p.pdf_url = Some(href.clone());
                            }
                            if rel == "alternate" {
                                p.html_url = Some(href);
                            } else if p.pdf_url.is_none() && href.contains("/pdf/") {
                                p.pdf_url = Some(href);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.local_name();
                let tag = std::str::from_utf8(name.as_ref()).unwrap_or("").to_string();
                match tag.as_str() {
                    "entry" => {
                        if let Some(builder) = current.take() {
                            if let Some(paper) = builder.build() {
                                papers.push(paper);
                            }
                        }
                    }
                    "author" if in_author => {
                        if let Some(ref mut p) = current {
                            if !author_name.is_empty() {
                                p.authors.push(Author {
                                    name: author_name.clone(),
                                    affiliation: None,
                                    orcid: None,
                                });
                            }
                        }
                        in_author = false;
                        author_name.clear();
                    }
                    _ => {}
                }
                current_tag.clear();
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                if in_author && current_tag == "name" {
                    author_name = text;
                } else if let Some(ref mut p) = current {
                    match current_tag.as_str() {
                        "id" => {
                            // arXiv ID is the URL: http://arxiv.org/abs/XXXX.XXXXX
                            let id = text.trim_start_matches("http://arxiv.org/abs/")
                                .trim_start_matches("https://arxiv.org/abs/")
                                .to_string();
                            p.arxiv_id = Some(id.clone());
                            p.source_id = id;
                        }
                        "title" => p.title = text,
                        "summary" => p.abstract_text = Some(text),
                        "published" => {
                            p.published = NaiveDate::parse_from_str(&text[..10], "%Y-%m-%d").ok();
                        }
                        "updated" => {
                            p.updated = NaiveDate::parse_from_str(&text[..10], "%Y-%m-%d").ok();
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.local_name();
                let tag = std::str::from_utf8(name.as_ref()).unwrap_or("").to_string();
                if tag == "link" {
                    if let Some(ref mut p) = current {
                        let mut rel = String::new();
                        let mut href = String::new();
                        let mut link_type = String::new();
                        let mut title_attr = String::new();
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            let val = attr.unescape_value().unwrap_or_default().to_string();
                            match key {
                                "rel" => rel = val,
                                "href" => href = val,
                                "type" => link_type = val,
                                "title" => title_attr = val,
                                _ => {}
                            }
                        }
                        if title_attr == "pdf" || link_type == "application/pdf" {
                            p.pdf_url = Some(href.clone());
                        } else if rel == "alternate" && link_type == "text/html" {
                            p.html_url = Some(href.clone());
                        }
                    }
                }
                // Parse arxiv category tags
                if tag == "primary_category" || tag == "category" {
                    if let Some(ref mut p) = current {
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            if key == "term" {
                                let val = attr.unescape_value().unwrap_or_default().to_string();
                                if !p.categories.contains(&val) {
                                    p.categories.push(val);
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(papers)
}

#[derive(Default)]
struct PaperBuilder {
    source_id: String,
    arxiv_id: Option<String>,
    title: String,
    authors: Vec<Author>,
    abstract_text: Option<String>,
    published: Option<NaiveDate>,
    updated: Option<NaiveDate>,
    categories: Vec<String>,
    pdf_url: Option<String>,
    html_url: Option<String>,
}

impl PaperBuilder {
    fn build(self) -> Option<Paper> {
        if self.title.is_empty() {
            return None;
        }
        let arxiv_id = self.arxiv_id.clone().or_else(|| Some(self.source_id.clone()));
        let pdf_url = self.pdf_url.or_else(|| {
            arxiv_id.as_ref().map(|id| format!("https://arxiv.org/pdf/{}", id))
        });
        let html_url = self.html_url.or_else(|| {
            arxiv_id.as_ref().map(|id| format!("https://arxiv.org/abs/{}", id))
        });
        let source_id = self.source_id.clone();
        let mut paper = Paper::new(PaperSourceKind::Arxiv, source_id, self.title);
        paper.authors = self.authors;
        paper.abstract_text = self.abstract_text;
        paper.published_date = self.published;
        paper.updated_date = self.updated;
        paper.categories = self.categories;
        paper.pdf_url = pdf_url;
        paper.html_url = html_url;
        paper.arxiv_id = arxiv_id;
        paper.is_open_access = true; // arXiv papers are always open access
        Some(paper)
    }
}

fn retry_after(resp: &reqwest::Response) -> u64 {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
}

// Inline URL encoding to avoid extra dependency
mod urlencoding {
    pub fn encode(s: &str) -> String {
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
}
