use papyrus_lib::db::Database;
use papyrus_lib::watch::WatchRunner;
use papyrus_lib::models::{Paper, PaperSourceKind};
use tempfile::tempdir;

fn make_paper(source: PaperSourceKind, source_id: &str, title: &str) -> Paper {
    Paper::new(source, source_id, title)
}

#[test]
fn test_watch_new_papers_detected() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let watch_id = db.add_watch("transformers", &["arxiv"], Some("test-watch"), true).unwrap();

    // Simulate a batch of results: p1 is new, p2 was already seen
    let p1 = make_paper(PaperSourceKind::Arxiv, "2501.00001", "New Transformer Paper");
    let p2 = make_paper(PaperSourceKind::Arxiv, "2301.00001", "Old Paper");

    // Mark p2 as already seen
    let p2_key = format!("{}:{}", "arxiv", p2.source_id);
    db.mark_watch_paper_seen(&watch_id, &p2_key).unwrap();

    let papers = vec![p1.clone(), p2.clone()];
    let runner = WatchRunner::new(db);
    let new_papers = runner.filter_new_papers(&watch_id, &papers).unwrap();

    assert_eq!(new_papers.len(), 1);
    assert_eq!(new_papers[0].title, "New Transformer Paper");
}

#[test]
fn test_watch_update_last_run() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let watch_id = db.add_watch("deep learning", &["arxiv"], None, true).unwrap();

    let watches = db.list_watches().unwrap();
    assert!(watches[0].last_run_at.is_none());

    let runner = WatchRunner::new(db);
    runner.update_last_run(&watch_id).unwrap();

    let watches = runner.db().list_watches().unwrap();
    assert!(watches[0].last_run_at.is_some());
}

#[test]
fn test_watch_json_output_format() {
    let p = make_paper(PaperSourceKind::Arxiv, "2501.00001", "Test Paper");
    let json = serde_json::to_string(&p).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["title"], "Test Paper");
    assert_eq!(parsed["source"], "arxiv");
}

#[test]
fn test_watch_sources_parse() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();

    let watch_id = db.add_watch("llm", &["arxiv", "semantic_scholar"], None, true).unwrap();
    let watches = db.list_watches().unwrap();
    let w = &watches[0];
    assert!(w.sources.contains(&"arxiv".to_string()));
    assert!(w.sources.contains(&"semantic_scholar".to_string()));
}
