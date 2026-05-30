use chrono::{NaiveDate, Local, Duration, Datelike};
use clap::Args;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::PaperSourceKind;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, JsonSchema)]
pub enum SortOrder {
    #[default]
    Relevance,
    DateDesc,
    DateAsc,
    CitationsDesc,
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortOrder::Relevance => write!(f, "relevance"),
            SortOrder::DateDesc => write!(f, "date-desc"),
            SortOrder::DateAsc => write!(f, "date-asc"),
            SortOrder::CitationsDesc => write!(f, "citations-desc"),
        }
    }
}

impl std::str::FromStr for SortOrder {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "relevance" => Ok(SortOrder::Relevance),
            "date-desc" => Ok(SortOrder::DateDesc),
            "date-asc" => Ok(SortOrder::DateAsc),
            "citations-desc" => Ok(SortOrder::CitationsDesc),
            other => Err(anyhow::anyhow!("Unknown sort order: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FilterSet {
    pub query: Option<String>,
    pub title_query: Option<String>,
    pub abstract_query: Option<String>,
    pub authors: Vec<String>,
    pub categories: Vec<String>,
    pub journal: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub min_citations: Option<u32>,
    pub max_citations: Option<u32>,
    pub has_pdf: bool,
    pub has_code: bool,
    pub peer_reviewed_only: bool,
    pub preprint_only: bool,
    pub open_access_only: bool,
    pub sources: Vec<PaperSourceKind>,
    pub sort: SortOrder,
    pub limit: u32,
    pub offset: u32,
}

impl Default for FilterSet {
    fn default() -> Self {
        Self {
            query: None,
            title_query: None,
            abstract_query: None,
            authors: Vec::new(),
            categories: Vec::new(),
            journal: None,
            doi: None,
            arxiv_id: None,
            date_from: None,
            date_to: None,
            min_citations: None,
            max_citations: None,
            has_pdf: false,
            has_code: false,
            peer_reviewed_only: false,
            preprint_only: false,
            open_access_only: false,
            sources: vec![
                PaperSourceKind::Arxiv,
                PaperSourceKind::SemanticScholar,
                PaperSourceKind::PubMed,
                PaperSourceKind::CrossRef,
            ],
            sort: SortOrder::Relevance,
            limit: 20,
            offset: 0,
        }
    }
}

impl FilterSet {
    pub fn active_chips(&self) -> Vec<String> {
        let mut chips = Vec::new();
        if let Some(q) = &self.query {
            chips.push(format!("q: \"{}\"", q));
        }
        if let Some(t) = &self.title_query {
            chips.push(format!("title: \"{}\"", t));
        }
        if let Some(a) = &self.abstract_query {
            chips.push(format!("abs: \"{}\"", a));
        }
        for author in &self.authors {
            chips.push(format!("author: {}", author));
        }
        for cat in &self.categories {
            chips.push(format!("cat: {}", cat));
        }
        if let Some(j) = &self.journal {
            chips.push(format!("journal: {}", j));
        }
        if let Some(from) = &self.date_from {
            chips.push(format!("from: {}", from.format("%Y-%m-%d")));
        }
        if let Some(to) = &self.date_to {
            chips.push(format!("to: {}", to.format("%Y-%m-%d")));
        }
        if let Some(min) = self.min_citations {
            chips.push(format!("min-cite: {}", min));
        }
        if self.has_pdf {
            chips.push("has-pdf".to_string());
        }
        if self.has_code {
            chips.push("has-code".to_string());
        }
        if self.peer_reviewed_only {
            chips.push("peer-reviewed".to_string());
        }
        if self.preprint_only {
            chips.push("preprint-only".to_string());
        }
        if self.open_access_only {
            chips.push("open-access".to_string());
        }
        chips
    }
}

/// CLI args that map directly onto FilterSet fields.
#[derive(Debug, Args, Clone)]
pub struct FilterArgs {
    #[arg(short = 'q', long)]
    pub query: Option<String>,

    #[arg(long)]
    pub title: Option<String>,

    #[arg(long = "abstract")]
    pub abstract_query: Option<String>,

    #[arg(short = 'a', long = "author")]
    pub authors: Vec<String>,

    #[arg(short = 'c', long = "category")]
    pub categories: Vec<String>,

    #[arg(short = 'j', long)]
    pub journal: Option<String>,

    #[arg(long)]
    pub doi: Option<String>,

    #[arg(long = "arxiv-id")]
    pub arxiv_id: Option<String>,

    #[arg(long)]
    pub from: Option<String>,

    #[arg(long)]
    pub to: Option<String>,

    #[arg(short = 'y', long)]
    pub year: Option<u16>,

    #[arg(long = "last-days")]
    pub last_days: Option<u32>,

    #[arg(long = "last-months")]
    pub last_months: Option<u32>,

    #[arg(long = "min-citations")]
    pub min_citations: Option<u32>,

    #[arg(long = "max-citations")]
    pub max_citations: Option<u32>,

    #[arg(long = "has-pdf")]
    pub has_pdf: bool,

    #[arg(long = "has-code")]
    pub has_code: bool,

    #[arg(long = "peer-reviewed")]
    pub peer_reviewed: bool,

    #[arg(long = "preprint-only")]
    pub preprint_only: bool,

    #[arg(long = "open-access")]
    pub open_access: bool,

    #[arg(short = 's', long = "source")]
    pub sources: Vec<String>,

    #[arg(long, default_value = "relevance")]
    pub sort: String,

    #[arg(short = 'n', long, default_value = "20")]
    pub limit: u32,

    #[arg(long, default_value = "0")]
    pub offset: u32,
}

pub fn parse_flexible_date_pub(s: &str) -> Option<NaiveDate> {
    parse_flexible_date(s).ok()
}

fn parse_flexible_date(s: &str) -> anyhow::Result<NaiveDate> {
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&format!("{}-01", s), "%Y-%m-%d") {
        return Ok(d);
    }
    if let Ok(y) = s.parse::<i32>() {
        return NaiveDate::from_ymd_opt(y, 1, 1)
            .ok_or_else(|| anyhow::anyhow!("Invalid year: {}", y));
    }
    Err(anyhow::anyhow!("Cannot parse date: {}", s))
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

impl FilterArgs {
    pub fn into_filter_set(self) -> anyhow::Result<FilterSet> {
        let mut fs = FilterSet::default();

        fs.query = self.query;
        fs.title_query = self.title;
        fs.abstract_query = self.abstract_query;
        fs.authors = self.authors;
        fs.categories = self.categories;
        fs.journal = self.journal;
        fs.doi = self.doi;
        fs.arxiv_id = self.arxiv_id;
        fs.min_citations = self.min_citations;
        fs.max_citations = self.max_citations;
        fs.has_pdf = self.has_pdf;
        fs.has_code = self.has_code;
        fs.peer_reviewed_only = self.peer_reviewed;
        fs.preprint_only = self.preprint_only;
        fs.open_access_only = self.open_access;
        fs.limit = self.limit.min(500);
        fs.offset = self.offset;
        fs.sort = self.sort.parse()?;

        // Date resolution
        if let Some(y) = self.year {
            let year = y as i32;
            fs.date_from = NaiveDate::from_ymd_opt(year, 1, 1);
            fs.date_to = NaiveDate::from_ymd_opt(year, 12, 31);
        } else {
            if let Some(from_str) = self.from {
                fs.date_from = Some(parse_flexible_date(&from_str)?);
            }
            if let Some(to_str) = self.to {
                fs.date_to = Some(parse_flexible_date(&to_str)?);
            }
        }

        if let Some(days) = self.last_days {
            let today = Local::now().date_naive();
            fs.date_from = Some(today - Duration::days(days as i64));
            fs.date_to = Some(today);
        }

        if let Some(months) = self.last_months {
            let today = Local::now().date_naive();
            let from = NaiveDate::from_ymd_opt(
                today.year(),
                today.month().saturating_sub(months as u32).max(1),
                today.day(),
            )
            .unwrap_or(today);
            fs.date_from = Some(from);
            fs.date_to = Some(today);
        }

        // Sources
        if !self.sources.is_empty() {
            let mut sources = Vec::new();
            for s in &self.sources {
                sources.extend(parse_source(s)?);
            }
            fs.sources = sources;
        }

        Ok(fs)
    }
}
