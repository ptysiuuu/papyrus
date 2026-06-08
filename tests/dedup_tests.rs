use papyrus_lib::dedup::{normalize_title, trigram_jaccard, fuzzy_dedup, merge_richest};
use papyrus_lib::models::{Author, Paper, PaperSourceKind};

fn paper_with_title(title: &str) -> Paper {
    Paper::new(PaperSourceKind::Arxiv, "id1", title)
}

#[test]
fn test_normalize_strips_articles() {
    assert_eq!(normalize_title("The Transformer Architecture"), "transformer architecture");
    assert_eq!(normalize_title("A New Approach to Attention"), "new approach to attention");
    assert_eq!(normalize_title("An Empirical Study"), "empirical study");
}

#[test]
fn test_normalize_unicode_nfc() {
    // Precomposed vs decomposed forms should normalize to same string
    let composed = "Über Neural Networks";
    let normalized = normalize_title(composed);
    assert!(!normalized.contains("Ü"));
    assert!(normalized.contains("uber") || normalized.contains("ber"));
}

#[test]
fn test_normalize_removes_punctuation() {
    // Colons, hyphens become spaces; case is lowered; "of" is kept (not a leading article)
    assert_eq!(normalize_title("BERT: Pre-training of Deep Networks"), "bert pre training of deep networks");
}

#[test]
fn test_trigram_jaccard_identical() {
    assert!((trigram_jaccard("attention is all you need", "attention is all you need") - 1.0).abs() < 1e-6);
}

#[test]
fn test_trigram_jaccard_empty() {
    assert_eq!(trigram_jaccard("", "anything"), 0.0);
    assert_eq!(trigram_jaccard("anything", ""), 0.0);
    assert_eq!(trigram_jaccard("", ""), 0.0);
}

#[test]
fn test_trigram_jaccard_similar() {
    // Case-variant should be very similar
    let score = trigram_jaccard(
        "attention is all you need",
        "Attention Is All You Need",
    );
    // After lowercasing both: identical → 1.0
    assert!(score > 0.9, "score={}", score);
}

#[test]
fn test_trigram_jaccard_dissimilar() {
    let score = trigram_jaccard("deep learning", "quantum physics");
    assert!(score < 0.3, "score={}", score);
}

#[test]
fn test_trigram_jaccard_partial_match() {
    // These titles share substantial word overlap
    let score = trigram_jaccard(
        "attention is all you need",
        "attention mechanisms are all you need",
    );
    assert!(score > 0.3 && score < 1.0, "score={}", score);
}

#[test]
fn test_fuzzy_dedup_same_doi() {
    let mut p1 = paper_with_title("Attention Is All You Need");
    p1.doi = Some("10.1234/paper1".to_string());

    let mut p2 = paper_with_title("Attention is all you need");
    p2.doi = Some("10.1234/paper1".to_string());

    let deduped = fuzzy_dedup(vec![p1, p2]);
    assert_eq!(deduped.len(), 1);
}

#[test]
fn test_fuzzy_dedup_different_doi_different_papers() {
    let mut p1 = paper_with_title("Attention Is All You Need");
    p1.doi = Some("10.1234/paper1".to_string());

    let mut p2 = paper_with_title("Quantum Computing Fundamentals");
    p2.doi = Some("10.1234/paper2".to_string());

    let deduped = fuzzy_dedup(vec![p1, p2]);
    assert_eq!(deduped.len(), 2);
}

#[test]
fn test_fuzzy_dedup_similar_title_no_doi() {
    // Very similar titles without DOI should dedup
    let p1 = paper_with_title("Attention is All You Need");
    let p2 = paper_with_title("Attention Is All You Need.");

    let deduped = fuzzy_dedup(vec![p1, p2]);
    assert_eq!(deduped.len(), 1);
}

#[test]
fn test_fuzzy_dedup_different_titles() {
    let p1 = paper_with_title("BERT: Pre-training Deep Bidirectional Transformers");
    let p2 = paper_with_title("GPT-3: Language Models are Few-Shot Learners");

    let deduped = fuzzy_dedup(vec![p1, p2]);
    assert_eq!(deduped.len(), 2);
}

#[test]
fn test_fuzzy_dedup_prefers_richest() {
    // p2 has abstract; p1 does not — dedup should keep the one with abstract
    let mut p1 = Paper::new(PaperSourceKind::CrossRef, "cr:1", "Attention Is All You Need");
    p1.citation_count = None;
    p1.abstract_text = None;

    let mut p2 = Paper::new(PaperSourceKind::SemanticScholar, "s2:1", "Attention is all you need");
    p2.citation_count = Some(50000);
    p2.abstract_text = Some("We propose the Transformer...".to_string());

    let deduped = fuzzy_dedup(vec![p1, p2]);
    assert_eq!(deduped.len(), 1);
    assert!(deduped[0].abstract_text.is_some());
    assert_eq!(deduped[0].citation_count, Some(50000));
}

#[test]
fn test_merge_richest_picks_abstract() {
    let mut base = Paper::new(PaperSourceKind::CrossRef, "cr:1", "Paper");
    base.abstract_text = None;
    base.citation_count = None;
    base.pdf_url = None;

    let mut richer = Paper::new(PaperSourceKind::SemanticScholar, "s2:1", "Paper");
    richer.abstract_text = Some("Some abstract".to_string());
    richer.citation_count = Some(100);
    richer.pdf_url = Some("https://example.com/paper.pdf".to_string());

    let merged = merge_richest(base, richer);
    assert_eq!(merged.abstract_text, Some("Some abstract".to_string()));
    assert_eq!(merged.citation_count, Some(100));
    assert!(merged.pdf_url.is_some());
}

#[test]
fn test_fuzzy_dedup_arxiv_id_cross_reference() {
    // Two papers: one from CrossRef (no arxiv_id), one from arXiv — same arxiv_id cross-references
    let mut p1 = Paper::new(PaperSourceKind::CrossRef, "cr:1", "Attention Is All You Need");
    p1.arxiv_id = Some("1706.03762".to_string());

    let mut p2 = Paper::new(PaperSourceKind::Arxiv, "1706.03762", "Attention Is All You Need");
    p2.arxiv_id = Some("1706.03762".to_string());

    let deduped = fuzzy_dedup(vec![p1, p2]);
    assert_eq!(deduped.len(), 1);
}
