use papyrus_lib::db::Database;
use papyrus_lib::models::{Author, Paper, PaperSourceKind};
use tempfile::tempdir;

fn make_paper(source: PaperSourceKind, source_id: &str, title: &str) -> Paper {
    let mut p = Paper::new(source, source_id, title);
    p.abstract_text = Some(format!("Abstract for {}", title));
    p.authors = vec![Author { name: "Alice Smith".to_string(), affiliation: None, orcid: None }];
    p
}

#[test]
fn test_upsert_and_retrieve() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let paper = make_paper(PaperSourceKind::Arxiv, "2301.00001", "Attention Mechanisms");
    db.upsert_paper(&paper).unwrap();

    let found = db.get_paper_by_source("arxiv", "2301.00001").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().title, "Attention Mechanisms");
}

#[test]
fn test_upsert_idempotent() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let mut paper = make_paper(PaperSourceKind::Arxiv, "2301.00002", "Transformers");
    db.upsert_paper(&paper).unwrap();

    // Update citation count and upsert again
    paper.citation_count = Some(100);
    db.upsert_paper(&paper).unwrap();

    let found = db.get_paper_by_source("arxiv", "2301.00002").unwrap().unwrap();
    assert_eq!(found.citation_count, Some(100));
    assert_eq!(db.paper_count().unwrap(), 1);
}

#[test]
fn test_fts_search_by_title() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    db.upsert_paper(&make_paper(PaperSourceKind::SemanticScholar, "s2:1", "BERT: Pre-training of Deep Bidirectional Transformers")).unwrap();
    db.upsert_paper(&make_paper(PaperSourceKind::SemanticScholar, "s2:2", "GPT-3: Language Models are Few-Shot Learners")).unwrap();
    db.upsert_paper(&make_paper(PaperSourceKind::Arxiv, "ax:3", "Unrelated Computer Vision Paper")).unwrap();

    let results = db.fts_search("transformers", false).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].title.contains("BERT"));
}

#[test]
fn test_fts_search_fulltext() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let mut paper = make_paper(PaperSourceKind::Arxiv, "ax:1", "Neural Networks");
    paper.full_text = Some("The self-attention mechanism scales quadratically with sequence length".to_string());
    db.upsert_paper(&paper).unwrap();

    let results = db.fts_search("quadratically", true).unwrap();
    assert_eq!(results.len(), 1);

    // Without fulltext flag, should not find it (title/abstract don't contain the word)
    let results_no_ft = db.fts_search("quadratically", false).unwrap();
    assert_eq!(results_no_ft.len(), 0);
}

#[test]
fn test_tag_management() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let paper = make_paper(PaperSourceKind::Arxiv, "ax:1", "Some Paper");
    db.upsert_paper(&paper).unwrap();

    db.add_tag(&paper.id, "survey").unwrap();
    db.add_tag(&paper.id, "foundational").unwrap();

    let tags = db.get_tags(&paper.id).unwrap();
    assert!(tags.contains(&"survey".to_string()));
    assert!(tags.contains(&"foundational".to_string()));

    db.remove_tag(&paper.id, "survey").unwrap();
    let tags = db.get_tags(&paper.id).unwrap();
    assert!(!tags.contains(&"survey".to_string()));
    assert!(tags.contains(&"foundational".to_string()));
}

#[test]
fn test_collection_management() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let p1 = make_paper(PaperSourceKind::Arxiv, "ax:1", "Paper One");
    let p2 = make_paper(PaperSourceKind::Arxiv, "ax:2", "Paper Two");
    db.upsert_paper(&p1).unwrap();
    db.upsert_paper(&p2).unwrap();

    let coll_id = db.create_collection("thesis-chapter-2").unwrap();
    db.add_to_collection(&coll_id, &p1.id).unwrap();
    db.add_to_collection(&coll_id, &p2.id).unwrap();

    let papers = db.get_collection_papers(&coll_id).unwrap();
    assert_eq!(papers.len(), 2);

    db.remove_from_collection(&coll_id, &p1.id).unwrap();
    let papers = db.get_collection_papers(&coll_id).unwrap();
    assert_eq!(papers.len(), 1);
}

#[test]
fn test_list_collections() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    db.create_collection("alpha").unwrap();
    db.create_collection("beta").unwrap();

    let colls = db.list_collections().unwrap();
    assert_eq!(colls.len(), 2);
    let names: Vec<&str> = colls.iter().map(|(_, n, _)| n.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
}

#[test]
fn test_read_status() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let paper = make_paper(PaperSourceKind::Arxiv, "ax:1", "Test Paper");
    db.upsert_paper(&paper).unwrap();

    assert_eq!(db.get_read_status(&paper.id).unwrap(), Some("unread".to_string()));

    db.set_read_status(&paper.id, "reading").unwrap();
    assert_eq!(db.get_read_status(&paper.id).unwrap(), Some("reading".to_string()));

    db.set_read_status(&paper.id, "read").unwrap();
    assert_eq!(db.get_read_status(&paper.id).unwrap(), Some("read".to_string()));
}

#[test]
fn test_notes_and_priority() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let paper = make_paper(PaperSourceKind::Arxiv, "ax:1", "Test Paper");
    db.upsert_paper(&paper).unwrap();

    db.set_notes(&paper.id, "Key insight: attention scales as O(n²)").unwrap();
    db.set_priority(&paper.id, 5).unwrap();

    let retrieved = db.get_paper_by_id(&paper.id).unwrap().unwrap();
    assert_eq!(retrieved.notes, Some("Key insight: attention scales as O(n²)".to_string()));
    assert_eq!(retrieved.priority, Some(5));
}

#[test]
fn test_library_stats() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let mut p1 = make_paper(PaperSourceKind::Arxiv, "ax:1", "Paper 2020");
    p1.published_date = Some(chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap());
    let mut p2 = make_paper(PaperSourceKind::SemanticScholar, "s2:1", "Paper 2021");
    p2.published_date = Some(chrono::NaiveDate::from_ymd_opt(2021, 6, 15).unwrap());
    let mut p3 = make_paper(PaperSourceKind::Arxiv, "ax:2", "Paper 2021 B");
    p3.published_date = Some(chrono::NaiveDate::from_ymd_opt(2021, 9, 1).unwrap());
    db.upsert_paper(&p1).unwrap();
    db.upsert_paper(&p2).unwrap();
    db.upsert_paper(&p3).unwrap();

    let stats = db.stats().unwrap();
    assert_eq!(stats.total_papers, 3);
    assert_eq!(stats.by_source.get("arxiv").copied().unwrap_or(0), 2);
    assert_eq!(stats.by_source.get("semantic_scholar").copied().unwrap_or(0), 1);
    assert_eq!(stats.by_year.get(&2021).copied().unwrap_or(0), 2);
}

#[test]
fn test_citation_edge_store_and_retrieve() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    db.add_citation_edge("s2:root", "s2:ref1", None, false).unwrap();
    db.add_citation_edge("s2:root", "s2:ref2", Some("important context"), true).unwrap();
    db.add_citation_edge("s2:other", "s2:ref1", None, false).unwrap();

    let refs = db.get_references("s2:root").unwrap();
    assert_eq!(refs.len(), 2);

    let citers = db.get_citations("s2:ref1").unwrap();
    assert_eq!(citers.len(), 2);
}

#[test]
fn test_watch_crud() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let id = db.add_watch("llm alignment", &["arxiv", "semantic_scholar"], Some("daily-llm"), true).unwrap();

    let watches = db.list_watches().unwrap();
    assert_eq!(watches.len(), 1);
    assert_eq!(watches[0].query, "llm alignment");
    assert_eq!(watches[0].name, Some("daily-llm".to_string()));

    db.remove_watch(&id).unwrap();
    let watches = db.list_watches().unwrap();
    assert_eq!(watches.len(), 0);
}

#[test]
fn test_watch_seen_tracking() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let watch_id = db.add_watch("transformers", &["arxiv"], None, true).unwrap();

    assert!(!db.was_watch_paper_seen(&watch_id, "arxiv:2301.00001").unwrap());
    db.mark_watch_paper_seen(&watch_id, "arxiv:2301.00001").unwrap();
    assert!(db.was_watch_paper_seen(&watch_id, "arxiv:2301.00001").unwrap());
    assert!(!db.was_watch_paper_seen(&watch_id, "arxiv:2301.00002").unwrap());
}
