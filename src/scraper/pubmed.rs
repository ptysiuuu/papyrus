use async_trait::async_trait;
use chrono::NaiveDate;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Client;

use crate::filters::FilterSet;
use crate::models::{Author, Paper, PaperSourceKind, SearchResult};

use super::PaperSource;

pub struct PubMedSource {
    client: Client,
    api_key: Option<String>,
}

impl PubMedSource {
    pub fn new(client: Client, api_key: Option<String>) -> Self {
        Self { client, api_key }
    }

    fn api_key_param(&self) -> String {
        self.api_key
            .as_ref()
            .map(|k| format!("&api_key={}", k))
            .unwrap_or_default()
    }

    async fn esearch(&self, term: &str, retmax: u32, retstart: u32) -> anyhow::Result<(Vec<String>, u64)> {
        let url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term={}&retmax={}&retstart={}&usehistory=n&retmode=json{}",
            urlencoding(term),
            retmax,
            retstart,
            self.api_key_param()
        );
        let resp = self.client.get(&url).send().await?;
        let json: serde_json::Value = resp.json().await?;
        let ids = json["esearchresult"]["idlist"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let count = json["esearchresult"]["count"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        Ok((ids, count))
    }

    async fn efetch(&self, ids: &[String]) -> anyhow::Result<Vec<Paper>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let id_list = ids.join(",");
        let url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=pubmed&id={}&retmode=xml{}",
            id_list,
            self.api_key_param()
        );
        let resp = self.client.get(&url).send().await?;
        let body = resp.text().await?;
        parse_pubmed_xml(&body)
    }
}

#[async_trait]
impl PaperSource for PubMedSource {
    fn name(&self) -> &'static str {
        "PubMed"
    }

    async fn fetch(&self, filters: &FilterSet, page: u32) -> anyhow::Result<SearchResult> {
        let mut terms: Vec<String> = Vec::new();

        if let Some(q) = &filters.query {
            terms.push(format!("{}[All Fields]", q));
        }
        for author in &filters.authors {
            terms.push(format!("{}[Author]", author));
        }
        if let Some(from) = filters.date_from {
            if let Some(to) = filters.date_to {
                terms.push(format!(
                    "{}[PDAT]:{}[PDAT]",
                    from.format("%Y/%m/%d"),
                    to.format("%Y/%m/%d")
                ));
            }
        }
        if filters.peer_reviewed_only {
            terms.push("journal article[pt]".to_string());
        }

        if terms.is_empty() && filters.doi.is_none() {
            return Ok(SearchResult {
                papers: vec![],
                total_count: Some(0),
                source: PaperSourceKind::PubMed,
            });
        }

        let term = if terms.is_empty() {
            filters.doi.clone().unwrap_or_default()
        } else {
            terms.join(" AND ")
        };

        let retstart = filters.offset + page * filters.limit;
        let (ids, total) = self.esearch(&term, filters.limit, retstart).await?;
        let papers = self.efetch(&ids).await?;

        Ok(SearchResult {
            papers,
            total_count: Some(total),
            source: PaperSourceKind::PubMed,
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

fn parse_pubmed_xml(xml: &str) -> anyhow::Result<Vec<Paper>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut papers: Vec<Paper> = Vec::new();
    let mut current: Option<PubMedBuilder> = None;
    let mut tag_stack: Vec<String> = Vec::new();
    let mut in_author = false;
    let mut current_author = PubAuthor::default();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = tag_name(e.local_name().as_ref());
                tag_stack.push(tag.clone());
                match tag.as_str() {
                    "PubmedArticle" => {
                        current = Some(PubMedBuilder::default());
                    }
                    "Author" if current.is_some() => {
                        in_author = true;
                        current_author = PubAuthor::default();
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = tag_name(e.local_name().as_ref());
                tag_stack.pop();
                match tag.as_str() {
                    "PubmedArticle" => {
                        if let Some(b) = current.take() {
                            if let Some(p) = b.build() {
                                papers.push(p);
                            }
                        }
                    }
                    "Author" if in_author => {
                        if let Some(ref mut b) = current {
                            let full_name = if current_author.fore_name.is_empty() {
                                current_author.last_name.clone()
                            } else {
                                format!("{} {}", current_author.fore_name, current_author.last_name)
                            };
                            if !full_name.is_empty() {
                                b.authors.push(Author {
                                    name: full_name,
                                    affiliation: None,
                                    orcid: None,
                                });
                            }
                        }
                        in_author = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let current_tag = tag_stack.last().cloned().unwrap_or_default();
                if in_author {
                    match current_tag.as_str() {
                        "LastName" => current_author.last_name = text,
                        "ForeName" | "Initials" if current_author.fore_name.is_empty() => {
                            current_author.fore_name = text;
                        }
                        _ => {}
                    }
                    continue;
                }
                if let Some(ref mut b) = current {
                    match current_tag.as_str() {
                        "PMID" if tag_stack.len() == 3 => b.pmid = text,
                        "ArticleTitle" => b.title = text,
                        "AbstractText" => {
                            if b.abstract_text.is_empty() {
                                b.abstract_text = text;
                            } else {
                                b.abstract_text.push(' ');
                                b.abstract_text.push_str(&text);
                            }
                        }
                        "Title" if parent_tag(&tag_stack) == "Journal" => {
                            b.journal = text;
                        }
                        "Year" if parent_tag(&tag_stack) == "PubDate" => {
                            b.year = text.parse().ok();
                        }
                        "Month" if parent_tag(&tag_stack) == "PubDate" && b.month == 0 => {
                            b.month = parse_month(&text);
                        }
                        "Day" if parent_tag(&tag_stack) == "PubDate" && b.day == 0 => {
                            b.day = text.parse().unwrap_or(0);
                        }
                        "ArticleId" => {
                            // Article IDs are handled in the Empty event handler below
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag = tag_name(e.local_name().as_ref());
                if tag == "ArticleId" {
                    if let Some(ref mut b) = current {
                        let mut _id_type = String::new();
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            if key == "IdType" {
                                _id_type = attr.unescape_value().unwrap_or_default().to_string();
                            }
                        }
                        let _ = b;
                    }
                }
            }
            // Handle ArticleId with attributes and text content via combined approach
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("PubMed XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    // Second pass for DOIs via a simpler string search (quick-xml attribute+text combination is complex)
    enrich_with_dois(xml, &mut papers);

    Ok(papers)
}

fn enrich_with_dois(xml: &str, papers: &mut Vec<Paper>) {
    // Extract DOIs from ArticleId elements with IdType="doi"
    let mut idx = 0;
    let mut pmid_order: Vec<String> = Vec::new();
    // Build a map from PMID to DOI
    let mut pmid_doi: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut current_pmid = String::new();

    while idx < xml.len() {
        if let Some(start) = xml[idx..].find("<PMID ") {
            let abs = idx + start;
            if let Some(end) = xml[abs..].find("</PMID>") {
                let tag_content = &xml[abs..abs + end + 7];
                if tag_content.contains("Version=\"1\"") {
                    if let Some(text_start) = tag_content.find('>') {
                        let text_end = tag_content.find("</PMID>").unwrap_or(tag_content.len());
                        current_pmid = tag_content[text_start + 1..text_end].to_string();
                        pmid_order.push(current_pmid.clone());
                    }
                }
                idx = abs + end + 7;
            } else {
                break;
            }
        } else if let Some(start) = xml[idx..].find("<ArticleId IdType=\"doi\">") {
            let abs = idx + start + "<ArticleId IdType=\"doi\">".len();
            if let Some(end) = xml[abs..].find("</ArticleId>") {
                let doi = xml[abs..abs + end].to_string();
                if !current_pmid.is_empty() {
                    pmid_doi.insert(current_pmid.clone(), doi);
                }
                idx = abs + end;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    for paper in papers.iter_mut() {
        if let Some(doi) = pmid_doi.get(&paper.source_id) {
            paper.doi = Some(doi.clone());
        }
    }
}

fn tag_name(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes).unwrap_or("").to_string()
}

fn parent_tag(stack: &[String]) -> &str {
    if stack.len() >= 2 {
        &stack[stack.len() - 2]
    } else {
        ""
    }
}

fn parse_month(s: &str) -> u32 {
    match s {
        "Jan" => 1, "Feb" => 2, "Mar" => 3, "Apr" => 4,
        "May" => 5, "Jun" => 6, "Jul" => 7, "Aug" => 8,
        "Sep" => 9, "Oct" => 10, "Nov" => 11, "Dec" => 12,
        other => other.parse().unwrap_or(1),
    }
}

#[derive(Default)]
struct PubMedBuilder {
    pmid: String,
    title: String,
    authors: Vec<Author>,
    abstract_text: String,
    year: Option<i32>,
    month: u32,
    day: u32,
    journal: String,
}

impl PubMedBuilder {
    fn build(self) -> Option<Paper> {
        if self.pmid.is_empty() || self.title.is_empty() {
            return None;
        }
        let mut paper = Paper::new(PaperSourceKind::PubMed, &self.pmid, &self.title);
        paper.authors = self.authors;
        paper.abstract_text = if self.abstract_text.is_empty() { None } else { Some(self.abstract_text) };
        paper.journal = if self.journal.is_empty() { None } else { Some(self.journal) };
        paper.is_peer_reviewed = true; // PubMed indexes peer-reviewed literature
        paper.html_url = Some(format!("https://pubmed.ncbi.nlm.nih.gov/{}/", self.pmid));

        if let Some(year) = self.year {
            let month = if self.month == 0 { 1 } else { self.month };
            let day = if self.day == 0 { 1 } else { self.day };
            paper.published_date = NaiveDate::from_ymd_opt(year, month, day);
        }

        Some(paper)
    }
}

#[derive(Default)]
struct PubAuthor {
    last_name: String,
    fore_name: String,
}
