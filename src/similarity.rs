use std::collections::HashMap;

use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

use crate::models::Paper;

// ─── TF-IDF cosine similarity ────────────────────────────────────────────────

/// Compute cosine similarity between two text documents using TF vectors.
/// Returns a score in [0.0, 1.0].
pub fn tfidf_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a_tf = term_freq(a);
    let b_tf = term_freq(b);
    cosine_similarity(&a_tf, &b_tf)
}

fn term_freq(text: &str) -> HashMap<String, f64> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let total = text
        .split_whitespace()
        .map(|w| {
            let word = w.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>();
            if !word.is_empty() && !is_stopword(&word) {
                *counts.entry(word).or_insert(0) += 1;
            }
        })
        .count();

    if total == 0 {
        return HashMap::new();
    }

    counts
        .into_iter()
        .map(|(term, cnt)| (term, cnt as f64 / total as f64))
        .collect()
}

fn cosine_similarity(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> f64 {
    let dot: f64 = a
        .iter()
        .filter_map(|(term, &weight)| b.get(term).map(|&bw| weight * bw))
        .sum();

    let mag_a: f64 = a.values().map(|v| v * v).sum::<f64>().sqrt();
    let mag_b: f64 = b.values().map(|v| v * v).sum::<f64>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    (dot / (mag_a * mag_b)).min(1.0)
}

fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        "a" | "an" | "the" | "and" | "or" | "of" | "in" | "to" | "is" | "are" | "was"
        | "were" | "be" | "been" | "being" | "for" | "with" | "on" | "at" | "by"
        | "from" | "as" | "this" | "that" | "it" | "its" | "we" | "our" | "their"
        | "have" | "has" | "had" | "which" | "can" | "not" | "also" | "but" | "show"
        | "shown" | "propose" | "proposed" | "method" | "approach" | "paper" | "work"
    )
}

/// Build a TF-IDF index from a collection of papers and find similar ones.
pub struct TfIdfIndex {
    papers: Vec<(Paper, HashMap<String, f64>)>,
    idf: HashMap<String, f64>,
}

impl TfIdfIndex {
    pub fn build(papers: &[Paper]) -> Self {
        let n = papers.len();
        if n == 0 {
            return Self { papers: Vec::new(), idf: HashMap::new() };
        }

        // Build TF for each paper
        let tfs: Vec<HashMap<String, f64>> = papers
            .iter()
            .map(|p| {
                let mut text = p.title.clone();
                if let Some(abs) = &p.abstract_text {
                    text.push(' ');
                    text.push_str(abs);
                }
                term_freq(&text)
            })
            .collect();

        // Compute IDF
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        for tf in &tfs {
            for term in tf.keys() {
                *doc_freq.entry(term.clone()).or_insert(0) += 1;
            }
        }
        let idf: HashMap<String, f64> = doc_freq
            .into_iter()
            .map(|(term, df)| (term, ((n as f64 + 1.0) / (df as f64 + 1.0)).ln() + 1.0))
            .collect();

        // Apply IDF to each TF to get TF-IDF vectors
        let tfidf_papers: Vec<(Paper, HashMap<String, f64>)> = papers
            .iter()
            .zip(tfs)
            .map(|(paper, tf)| {
                let tfidf: HashMap<String, f64> = tf
                    .into_iter()
                    .map(|(term, tf_val)| {
                        let idf_val = idf.get(&term).copied().unwrap_or(1.0);
                        (term, tf_val * idf_val)
                    })
                    .collect();
                (paper.clone(), tfidf)
            })
            .collect();

        Self { papers: tfidf_papers, idf }
    }

    /// Find the top-k most similar papers to the query paper.
    pub fn find_similar<'a>(&'a self, query: &Paper, k: usize) -> Vec<(&'a Paper, f64)> {
        if self.papers.is_empty() {
            return Vec::new();
        }

        let mut text = query.title.clone();
        if let Some(abs) = &query.abstract_text {
            text.push(' ');
            text.push_str(abs);
        }
        let query_tf = term_freq(&text);
        let query_tfidf: HashMap<String, f64> = query_tf
            .into_iter()
            .map(|(term, tf)| {
                let idf = self.idf.get(&term).copied().unwrap_or(1.0);
                (term, tf * idf)
            })
            .collect();

        let mut scores: Vec<(&Paper, f64)> = self
            .papers
            .iter()
            .map(|(paper, tfidf)| (paper, cosine_similarity(&query_tfidf, tfidf)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }
}

// ─── Semantic Scholar recommendations API ────────────────────────────────────

pub struct SimilarityClient {
    client: Client,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct S2RecommendResponse {
    recommended_papers: Vec<S2RecommendPaper>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct S2RecommendPaper {
    paper_id: String,
    title: Option<String>,
    citation_count: Option<u32>,
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    year: Option<i32>,
    authors: Option<Vec<S2Author>>,
    is_open_access: Option<bool>,
    open_access_pdf: Option<S2Pdf>,
    external_ids: Option<S2ExternalIds>,
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
}

impl SimilarityClient {
    pub fn new(client: Client, api_key: Option<String>) -> Self {
        Self { client, api_key }
    }

    pub async fn recommendations(&self, s2id: &str, limit: u32) -> Result<Vec<Paper>> {
        use chrono::NaiveDate;
        use crate::models::{Author, PaperSourceKind};

        let url = format!(
            "https://api.semanticscholar.org/recommendations/v1/papers/forpaper/{}",
            s2id
        );
        let mut req = self.client.get(&url).query(&[
            ("fields", "paperId,title,abstract,year,authors,citationCount,isOpenAccess,openAccessPdf,externalIds"),
            ("limit", &limit.to_string()),
        ]);
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }
        let resp: S2RecommendResponse = req.send().await?.json().await?;

        let papers = resp
            .recommended_papers
            .into_iter()
            .map(|rp| {
                let mut p = Paper::new(
                    PaperSourceKind::SemanticScholar,
                    &rp.paper_id,
                    rp.title.as_deref().unwrap_or("(no title)"),
                );
                p.semantic_scholar_id = Some(rp.paper_id);
                p.abstract_text = rp.abstract_text;
                p.citation_count = rp.citation_count;
                p.is_open_access = rp.is_open_access.unwrap_or(false);
                if let Some(pdf) = rp.open_access_pdf {
                    p.pdf_url = pdf.url;
                }
                if let Some(ids) = rp.external_ids {
                    p.doi = ids.doi;
                    p.arxiv_id = ids.arxiv;
                }
                if let Some(year) = rp.year {
                    p.published_date = NaiveDate::from_ymd_opt(year, 1, 1);
                }
                if let Some(authors) = rp.authors {
                    p.authors = authors
                        .into_iter()
                        .map(|a| Author { name: a.name, affiliation: None, orcid: None })
                        .collect();
                }
                p
            })
            .collect();
        Ok(papers)
    }
}
