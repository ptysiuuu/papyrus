use std::collections::HashSet;

use unicode_normalization::UnicodeNormalization;

use crate::models::Paper;

const JACCARD_THRESHOLD: f64 = 0.85;

static ARTICLES: &[&str] = &["the", "a", "an"];

/// Normalize a title for comparison: NFC → lowercase → strip non-alphanum → remove leading articles → collapse whitespace.
pub fn normalize_title(title: &str) -> String {
    // NFC normalize, then decompose diacritics by keeping only ASCII-compatible chars
    let nfc: String = title.nfc().collect();
    let ascii_folded: String = nfc
        .chars()
        .map(|c| {
            if c.is_ascii() {
                c.to_ascii_lowercase()
            } else {
                // Map common Latin accented chars to ASCII equivalents
                fold_unicode(c)
            }
        })
        .collect();

    // Strip non-alphanumeric (keep spaces)
    let stripped: String = ascii_folded
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
        .collect();

    // Split into words, drop leading articles
    let words: Vec<&str> = stripped.split_whitespace().collect();
    let words: Vec<&str> = if words.first().map(|w| ARTICLES.contains(w)).unwrap_or(false) {
        words[1..].to_vec()
    } else {
        words
    };

    words.join(" ")
}

fn fold_unicode(c: char) -> char {
    // Map common Latin-extended characters to their ASCII base
    match c {
        'à'|'á'|'â'|'ã'|'ä'|'å'|'ā'|'ă'|'ą' => 'a',
        'è'|'é'|'ê'|'ë'|'ē'|'ĕ'|'ę'|'ě' => 'e',
        'ì'|'í'|'î'|'ï'|'ī'|'ĭ'|'į'|'ı' => 'i',
        'ò'|'ó'|'ô'|'õ'|'ö'|'ø'|'ō'|'ŏ'|'ő' => 'o',
        'ù'|'ú'|'û'|'ü'|'ū'|'ŭ'|'ů'|'ű' => 'u',
        'ç'|'ć'|'ĉ'|'ċ'|'č' => 'c',
        'ñ'|'ń'|'ņ'|'ň' => 'n',
        'ý'|'ÿ' => 'y',
        'ß' => 's',
        'ž'|'ź'|'ż' => 'z',
        'š'|'ś'|'ŝ'|'ş' => 's',
        'ř'|'ŗ' => 'r',
        'ĺ'|'ļ'|'ľ'|'ŀ'|'ł' => 'l',
        'ğ'|'ĝ'|'ġ'|'ģ' => 'g',
        'ħ'|'ĥ' => 'h',
        'ð' => 'd',
        'þ' => 't',
        'æ' => 'a',
        'œ' => 'o',
        _ => ' ', // drop other non-ASCII
    }
}

/// Word-trigram Jaccard similarity between two strings (pre-lowercased).
pub fn trigram_jaccard(a: &str, b: &str) -> f64 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    let a_tris = word_trigrams(&a_lower);
    let b_tris = word_trigrams(&b_lower);
    if a_tris.is_empty() && b_tris.is_empty() {
        return 0.0;
    }
    if a_tris.is_empty() || b_tris.is_empty() {
        return 0.0;
    }
    let intersection = a_tris.intersection(&b_tris).count();
    let union = a_tris.union(&b_tris).count();
    intersection as f64 / union as f64
}

fn word_trigrams(s: &str) -> HashSet<String> {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut tris = HashSet::new();
    // Character trigrams on the concatenated string for robustness
    let concat: Vec<char> = s.chars().collect();
    for i in 0..concat.len().saturating_sub(2) {
        tris.insert(concat[i..i+3].iter().collect::<String>());
    }
    // Also add word bigrams and trigrams
    for w in words.windows(2) {
        tris.insert(w.join(" "));
    }
    for w in words.windows(3) {
        tris.insert(w.join(" "));
    }
    tris
}

/// Deduplicate a list of papers using enhanced matching:
/// 1. Exact DOI match
/// 2. Exact arXiv ID match
/// 3. Fuzzy title match (Jaccard ≥ threshold)
/// When duplicates are found, keep the richest record.
pub fn fuzzy_dedup(papers: Vec<Paper>) -> Vec<Paper> {
    let mut result: Vec<Paper> = Vec::new();

    for paper in papers {
        let mut merged = false;

        for existing in result.iter_mut() {
            if is_duplicate(existing, &paper) {
                // Merge into existing (keep richest)
                let richer = merge_richest(existing.clone(), paper.clone());
                *existing = richer;
                merged = true;
                break;
            }
        }

        if !merged {
            result.push(paper);
        }
    }

    result
}

fn is_duplicate(a: &Paper, b: &Paper) -> bool {
    // DOI match
    if let (Some(doi_a), Some(doi_b)) = (&a.doi, &b.doi) {
        if doi_a.to_lowercase() == doi_b.to_lowercase() {
            return true;
        }
    }

    // arXiv ID match
    if let (Some(ax_a), Some(ax_b)) = (&a.arxiv_id, &b.arxiv_id) {
        if ax_a == ax_b {
            return true;
        }
    }

    // Fuzzy title match (only if neither has a DOI — different DOIs = different papers)
    if a.doi.is_none() || b.doi.is_none() {
        let norm_a = normalize_title(&a.title);
        let norm_b = normalize_title(&b.title);
        if !norm_a.is_empty() && !norm_b.is_empty() {
            let score = trigram_jaccard(&norm_a, &norm_b);
            if score >= JACCARD_THRESHOLD {
                return true;
            }
        }
    }

    false
}

/// Merge two Paper records, keeping the richer fields.
/// The "richness" score prefers: abstract > no abstract, higher citations, PDF URL, etc.
pub fn merge_richest(a: Paper, b: Paper) -> Paper {
    if richness_score(&b) > richness_score(&a) {
        merge_fields(b, a)
    } else {
        merge_fields(a, b)
    }
}

/// Richness score: higher = more complete record.
fn richness_score(p: &Paper) -> u32 {
    let mut score = 0u32;
    if p.abstract_text.is_some() { score += 10; }
    if let Some(c) = p.citation_count { score += c.min(1000) / 100; }
    if p.pdf_url.is_some() { score += 3; }
    if p.doi.is_some() { score += 2; }
    if p.arxiv_id.is_some() { score += 2; }
    if !p.authors.is_empty() { score += 2; }
    if p.published_date.is_some() { score += 1; }
    score
}

/// Copy fields from `fallback` into `primary` wherever `primary` has None.
fn merge_fields(mut primary: Paper, fallback: Paper) -> Paper {
    if primary.abstract_text.is_none() { primary.abstract_text = fallback.abstract_text; }
    if primary.doi.is_none() { primary.doi = fallback.doi; }
    if primary.arxiv_id.is_none() { primary.arxiv_id = fallback.arxiv_id; }
    if primary.pubmed_id.is_none() { primary.pubmed_id = fallback.pubmed_id; }
    if primary.semantic_scholar_id.is_none() { primary.semantic_scholar_id = fallback.semantic_scholar_id; }
    if primary.pdf_url.is_none() { primary.pdf_url = fallback.pdf_url; }
    if primary.html_url.is_none() { primary.html_url = fallback.html_url; }
    if primary.code_url.is_none() { primary.code_url = fallback.code_url; }
    if primary.citation_count.is_none() { primary.citation_count = fallback.citation_count; }
    if primary.reference_count.is_none() { primary.reference_count = fallback.reference_count; }
    if primary.published_date.is_none() { primary.published_date = fallback.published_date; }
    if primary.journal.is_none() { primary.journal = fallback.journal; }
    if primary.authors.is_empty() { primary.authors = fallback.authors; }
    if primary.tldr.is_none() { primary.tldr = fallback.tldr; }
    // Merge tags
    for tag in fallback.tags {
        if !primary.tags.contains(&tag) {
            primary.tags.push(tag);
        }
    }
    primary
}
