use chrono::{Datelike, NaiveDate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaperSourceKind {
    Arxiv,
    SemanticScholar,
    PubMed,
    CrossRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReadStatus {
    #[default]
    Unread,
    Reading,
    Read,
    Reviewed,
}

impl ReadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReadStatus::Unread => "unread",
            ReadStatus::Reading => "reading",
            ReadStatus::Read => "read",
            ReadStatus::Reviewed => "reviewed",
        }
    }
}

impl std::fmt::Display for ReadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ReadStatus {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "unread" => Ok(ReadStatus::Unread),
            "reading" => Ok(ReadStatus::Reading),
            "read" => Ok(ReadStatus::Read),
            "reviewed" => Ok(ReadStatus::Reviewed),
            other => Err(anyhow::anyhow!("Unknown read status: '{}'", other)),
        }
    }
}

impl std::fmt::Display for PaperSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaperSourceKind::Arxiv => write!(f, "arXiv"),
            PaperSourceKind::SemanticScholar => write!(f, "S2"),
            PaperSourceKind::PubMed => write!(f, "PubMed"),
            PaperSourceKind::CrossRef => write!(f, "CrossRef"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Author {
    #[schemars(description = "Full name of the author")]
    pub name: String,
    #[schemars(description = "Author's institutional affiliation")]
    pub affiliation: Option<String>,
    #[schemars(description = "ORCID identifier")]
    pub orcid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Paper {
    pub id: String,
    pub source: PaperSourceKind,
    pub source_id: String,
    pub title: String,
    pub authors: Vec<Author>,
    pub abstract_text: Option<String>,
    pub published_date: Option<NaiveDate>,
    pub updated_date: Option<NaiveDate>,
    pub categories: Vec<String>,
    pub journal: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub pubmed_id: Option<String>,
    pub semantic_scholar_id: Option<String>,
    pub pdf_url: Option<String>,
    pub html_url: Option<String>,
    pub code_url: Option<String>,
    pub citation_count: Option<u32>,
    pub reference_count: Option<u32>,
    pub is_open_access: bool,
    pub is_peer_reviewed: bool,
    pub tags: Vec<String>,
    /// One-sentence TLDR from Semantic Scholar
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tldr: Option<String>,
    // ── Library fields (populated when loading from local DB) ──
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_status: Option<ReadStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_text: Option<String>,
}

impl Paper {
    pub fn new(source: PaperSourceKind, source_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source,
            source_id: source_id.into(),
            title: title.into(),
            authors: Vec::new(),
            abstract_text: None,
            published_date: None,
            updated_date: None,
            categories: Vec::new(),
            journal: None,
            doi: None,
            arxiv_id: None,
            pubmed_id: None,
            semantic_scholar_id: None,
            pdf_url: None,
            html_url: None,
            code_url: None,
            citation_count: None,
            reference_count: None,
            is_open_access: false,
            is_peer_reviewed: false,
            tags: Vec::new(),
            tldr: None,
            read_status: None,
            notes: None,
            priority: None,
            pdf_path: None,
            full_text: None,
        }
    }

    pub fn authors_display(&self) -> String {
        let names: Vec<&str> = self.authors.iter().map(|a| a.name.as_str()).collect();
        match names.len() {
            0 => String::from("Unknown"),
            1 => names[0].to_string(),
            2 => format!("{} & {}", names[0], names[1]),
            _ => format!("{} et al.", names[0]),
        }
    }

    pub fn year(&self) -> Option<i32> {
        self.published_date.map(|d| d.year())
    }

    pub fn dedup_key(&self) -> String {
        if let Some(doi) = &self.doi {
            return format!("doi:{}", doi.to_lowercase());
        }
        let normalized = self
            .title
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        format!("title:{}", normalized)
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub papers: Vec<Paper>,
    pub total_count: Option<u64>,
    pub source: PaperSourceKind,
}
