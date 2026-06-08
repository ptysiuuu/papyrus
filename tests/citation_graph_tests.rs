use papyrus_lib::db::Database;
use papyrus_lib::citation_graph::{CitationGraphStore, GraphNode};
use tempfile::tempdir;

fn insert_node(store: &CitationGraphStore, s2id: &str, title: &str) {
    store.ensure_node(s2id, title, None).unwrap();
}

#[test]
fn test_store_and_retrieve_edges() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let store = CitationGraphStore::new(db);

    insert_node(&store, "s2:root", "Root Paper");
    insert_node(&store, "s2:ref1", "Reference 1");
    insert_node(&store, "s2:ref2", "Reference 2");

    store.add_reference("s2:root", "s2:ref1", None, false).unwrap();
    store.add_reference("s2:root", "s2:ref2", None, true).unwrap();

    let refs = store.get_references("s2:root").unwrap();
    assert_eq!(refs.len(), 2);
    let cited_ids: Vec<&str> = refs.iter().map(|n| n.s2id.as_str()).collect();
    assert!(cited_ids.contains(&"s2:ref1"));
    assert!(cited_ids.contains(&"s2:ref2"));
}

#[test]
fn test_ancestors_depth_1() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let store = CitationGraphStore::new(db);

    // root → a → b → c
    insert_node(&store, "root", "Root");
    insert_node(&store, "a", "A");
    insert_node(&store, "b", "B");
    insert_node(&store, "c", "C");

    store.add_reference("root", "a", None, false).unwrap();
    store.add_reference("a", "b", None, false).unwrap();
    store.add_reference("b", "c", None, false).unwrap();

    let ancestors = store.ancestors("root", 1).unwrap();
    assert_eq!(ancestors.len(), 1, "depth=1 should give only direct refs");
    assert_eq!(ancestors[0].s2id, "a");
}

#[test]
fn test_ancestors_depth_2() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let store = CitationGraphStore::new(db);

    insert_node(&store, "root", "Root");
    insert_node(&store, "a", "A");
    insert_node(&store, "b", "B");
    insert_node(&store, "c", "C");

    store.add_reference("root", "a", None, false).unwrap();
    store.add_reference("a", "b", None, false).unwrap();
    store.add_reference("b", "c", None, false).unwrap();

    let ancestors = store.ancestors("root", 2).unwrap();
    let ids: Vec<&str> = ancestors.iter().map(|n| n.s2id.as_str()).collect();
    assert!(ids.contains(&"a"), "should include direct ref a");
    assert!(ids.contains(&"b"), "should include ref b at depth 2");
    assert!(!ids.contains(&"c"), "c is at depth 3, should not appear");
}

#[test]
fn test_descendants_depth_2() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let store = CitationGraphStore::new(db);

    // paper1 ← citer1 ← citer2
    insert_node(&store, "paper1", "Foundational Paper");
    insert_node(&store, "citer1", "First Citer");
    insert_node(&store, "citer2", "Second Citer");

    store.add_reference("citer1", "paper1", None, false).unwrap();
    store.add_reference("citer2", "citer1", None, false).unwrap();

    let descs = store.descendants("paper1", 2).unwrap();
    let ids: Vec<&str> = descs.iter().map(|n| n.s2id.as_str()).collect();
    assert!(ids.contains(&"citer1"));
    assert!(ids.contains(&"citer2"));
}

#[test]
fn test_common_references() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let store = CitationGraphStore::new(db);

    insert_node(&store, "p1", "Paper 1");
    insert_node(&store, "p2", "Paper 2");
    insert_node(&store, "shared1", "Shared Ref 1");
    insert_node(&store, "shared2", "Shared Ref 2");
    insert_node(&store, "unique1", "Unique to P1");

    store.add_reference("p1", "shared1", None, false).unwrap();
    store.add_reference("p1", "shared2", None, false).unwrap();
    store.add_reference("p1", "unique1", None, false).unwrap();
    store.add_reference("p2", "shared1", None, false).unwrap();
    store.add_reference("p2", "shared2", None, false).unwrap();

    let common = store.common_references("p1", "p2").unwrap();
    assert_eq!(common.len(), 2);
    let ids: Vec<&str> = common.iter().map(|n| n.s2id.as_str()).collect();
    assert!(ids.contains(&"shared1"));
    assert!(ids.contains(&"shared2"));
    assert!(!ids.contains(&"unique1"));
}

#[test]
fn test_no_cycles_in_traversal() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let store = CitationGraphStore::new(db);

    // Cycle: a → b → c → a
    insert_node(&store, "a", "A");
    insert_node(&store, "b", "B");
    insert_node(&store, "c", "C");
    store.add_reference("a", "b", None, false).unwrap();
    store.add_reference("b", "c", None, false).unwrap();
    store.add_reference("c", "a", None, false).unwrap();

    // Should not infinite loop, should complete
    let ancestors = store.ancestors("a", 10).unwrap();
    // b and c should appear exactly once each
    let ids: Vec<&str> = ancestors.iter().map(|n| n.s2id.as_str()).collect();
    let b_count = ids.iter().filter(|&&id| id == "b").count();
    let c_count = ids.iter().filter(|&&id| id == "c").count();
    assert_eq!(b_count, 1);
    assert_eq!(c_count, 1);
}

#[test]
fn test_seminal_finds_high_citation_roots() {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let store = CitationGraphStore::new(db);

    // Graph: modern1 cites medium1 cites foundational (many citations)
    store.ensure_node("foundational", "Attention Is All You Need", Some(50000)).unwrap();
    store.ensure_node("medium1", "BERT", Some(40000)).unwrap();
    store.ensure_node("modern1", "GPT-4", Some(1000)).unwrap();
    store.ensure_node("modern2", "LLaMA", Some(2000)).unwrap();

    store.add_reference("modern1", "medium1", None, false).unwrap();
    store.add_reference("modern1", "foundational", None, false).unwrap();
    store.add_reference("modern2", "medium1", None, false).unwrap();
    store.add_reference("modern2", "foundational", None, false).unwrap();
    store.add_reference("medium1", "foundational", None, false).unwrap();

    // Seminal should rank by citation count, foundational should be first
    let seminal = store.seminal_nodes(5).unwrap();
    assert!(!seminal.is_empty());
    assert_eq!(seminal[0].s2id, "foundational");
}
