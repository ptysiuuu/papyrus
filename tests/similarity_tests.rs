use papyrus_lib::similarity::{tfidf_similarity, TfIdfIndex};
use papyrus_lib::models::{Paper, PaperSourceKind};

fn paper_with_abstract(id: &str, title: &str, abs: &str) -> Paper {
    let mut p = Paper::new(PaperSourceKind::Arxiv, id, title);
    p.abstract_text = Some(abs.to_string());
    p
}

#[test]
fn test_tfidf_identical_documents() {
    let text = "attention mechanisms self-attention transformer neural network";
    let score = tfidf_similarity(text, text);
    assert!((score - 1.0).abs() < 1e-6, "Identical docs should score 1.0, got {}", score);
}

#[test]
fn test_tfidf_empty_documents() {
    assert_eq!(tfidf_similarity("", "anything"), 0.0);
    assert_eq!(tfidf_similarity("anything", ""), 0.0);
    assert_eq!(tfidf_similarity("", ""), 0.0);
}

#[test]
fn test_tfidf_unrelated_documents() {
    let score = tfidf_similarity(
        "quantum entanglement superconductor physics",
        "neural network gradient descent backpropagation",
    );
    assert!(score < 0.2, "Unrelated docs should score low, got {}", score);
}

#[test]
fn test_tfidf_related_documents() {
    let score = tfidf_similarity(
        "attention is all you need transformer architecture",
        "self-attention multi-head attention transformer model",
    );
    assert!(score > 0.3, "Related docs should score high, got {}", score);
}

#[test]
fn test_tfidf_index_find_similar() {
    let papers = vec![
        paper_with_abstract("p1", "Attention Mechanisms", "self-attention transformer architecture multi-head"),
        paper_with_abstract("p2", "BERT Pre-training", "bidirectional encoder representations transformer language model"),
        paper_with_abstract("p3", "Quantum Computing", "qubit entanglement superposition quantum circuit"),
        paper_with_abstract("p4", "GPT Language Model", "autoregressive transformer language generation text"),
    ];

    let index = TfIdfIndex::build(&papers);
    let query = paper_with_abstract("q", "Query", "transformer language model attention");
    let similar = index.find_similar(&query, 2);

    assert_eq!(similar.len(), 2);
    // p1, p2, p4 are all transformer-related; p3 is not
    // The top 2 should not include p3
    let ids: Vec<&str> = similar.iter().map(|(p, _)| p.source_id.as_str()).collect();
    assert!(!ids.contains(&"p3"), "Quantum paper should not be in top 2 transformer results");
}

#[test]
fn test_tfidf_index_empty_library() {
    let index = TfIdfIndex::build(&[]);
    let query = paper_with_abstract("q", "Query", "transformer");
    let similar = index.find_similar(&query, 5);
    assert!(similar.is_empty());
}

#[test]
fn test_tfidf_index_scores_are_bounded() {
    let papers = vec![
        paper_with_abstract("p1", "Paper A", "transformer attention neural"),
        paper_with_abstract("p2", "Paper B", "recurrent LSTM sequence"),
    ];
    let index = TfIdfIndex::build(&papers);
    let query = paper_with_abstract("q", "Query", "transformer");
    let similar = index.find_similar(&query, 10);
    for (_, score) in &similar {
        assert!(*score >= 0.0 && *score <= 1.0, "Score out of bounds: {}", score);
    }
}
