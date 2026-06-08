use anyhow::Result;

use crate::db::Database;
use crate::export::{export_papers, ExportFormat};
use crate::models::{Paper, ReadStatus};

/// Dispatch a library subcommand. Returns printed output.
pub fn cmd_library_search(db: &Database, query: &str, fulltext: bool) -> Result<Vec<Paper>> {
    db.fts_search(query, fulltext)
}

pub fn cmd_library_add(db: &Database, paper: &Paper) -> Result<()> {
    db.upsert_paper(paper)
}

pub fn cmd_library_tag(db: &Database, paper_id: &str, tags: &[&str]) -> Result<()> {
    for tag in tags {
        db.add_tag(paper_id, tag)?;
    }
    Ok(())
}

pub fn cmd_library_untag(db: &Database, paper_id: &str, tag: &str) -> Result<()> {
    db.remove_tag(paper_id, tag)
}

pub fn cmd_library_status(db: &Database, paper_id: &str, status: &str) -> Result<()> {
    // Validate status
    status.parse::<ReadStatus>()?;
    db.set_read_status(paper_id, status)
}

pub fn cmd_library_note(db: &Database, paper_id: &str, note: &str) -> Result<()> {
    db.set_notes(paper_id, note)
}

pub fn cmd_library_priority(db: &Database, paper_id: &str, priority: u8) -> Result<()> {
    if priority < 1 || priority > 5 {
        anyhow::bail!("Priority must be between 1 and 5");
    }
    db.set_priority(paper_id, priority)
}

pub fn cmd_library_stats(db: &Database) -> Result<()> {
    let stats = db.stats()?;
    println!("Total papers: {}", stats.total_papers);

    if !stats.by_source.is_empty() {
        println!("\nBy source:");
        let mut by_src: Vec<_> = stats.by_source.iter().collect();
        by_src.sort_by(|a, b| b.1.cmp(a.1));
        for (src, count) in by_src {
            println!("  {:<20} {}", src, count);
        }
    }

    if !stats.by_year.is_empty() {
        println!("\nBy year:");
        let mut by_year: Vec<_> = stats.by_year.iter().collect();
        by_year.sort_by_key(|(y, _)| std::cmp::Reverse(**y));
        for (year, count) in by_year.iter().take(10) {
            println!("  {}: {}", year, count);
        }
    }

    if !stats.by_tag.is_empty() {
        println!("\nTop tags:");
        let mut by_tag: Vec<_> = stats.by_tag.iter().collect();
        by_tag.sort_by(|a, b| b.1.cmp(a.1));
        for (tag, count) in by_tag.iter().take(10) {
            println!("  {:<20} {}", tag, count);
        }
    }

    println!("\nRead status:");
    for status in &["unread", "reading", "read", "reviewed"] {
        let count = stats.by_read_status.get(*status).copied().unwrap_or(0);
        println!("  {:<12} {}", status, count);
    }

    Ok(())
}

pub fn cmd_library_export_review(
    db: &Database,
    collection_name: Option<&str>,
    output: &std::path::Path,
    fmt: ExportFormat,
) -> Result<()> {
    let papers = if let Some(name) = collection_name {
        let coll_id = db
            .get_collection_id_by_name(name)?
            .ok_or_else(|| anyhow::anyhow!("Collection '{}' not found", name))?;
        db.get_collection_papers(&coll_id)?
    } else {
        db.list_papers(usize::MAX, 0)?
    };

    // Sort by priority (descending) then citation count
    let mut papers = papers;
    papers.sort_by(|a, b| {
        let pa = a.priority.unwrap_or(0);
        let pb = b.priority.unwrap_or(0);
        pb.cmp(&pa)
            .then_with(|| {
                b.citation_count.unwrap_or(0).cmp(&a.citation_count.unwrap_or(0))
            })
    });

    let mut file = std::fs::File::create(output)?;
    export_papers(&papers, &fmt, &mut file)?;
    println!("Exported {} papers to {:?}", papers.len(), output);
    Ok(())
}

pub fn cmd_library_duplicates(db: &Database) -> Result<Vec<(Paper, Paper)>> {
    use crate::dedup::fuzzy_dedup;

    let all = db.list_papers(usize::MAX, 0)?;
    let before = all.len();
    let deduped = fuzzy_dedup(all.clone());
    let after = deduped.len();

    println!("Library has {} papers, {} are potential duplicates", before, before - after);

    // Find which papers were merged (simplified: just show ones not in deduped)
    let kept_ids: std::collections::HashSet<&str> = deduped.iter().map(|p| p.id.as_str()).collect();
    let dupes: Vec<&Paper> = all.iter().filter(|p| !kept_ids.contains(p.id.as_str())).collect();

    for dupe in &dupes {
        println!("  Potential duplicate: [{}] {}", &dupe.id[..8], dupe.title);
    }

    Ok(Vec::new()) // Full pair reporting requires more complex matching; this is the summary form
}

pub fn cmd_create_collection(db: &Database, name: &str) -> Result<String> {
    let id = db.create_collection(name)?;
    println!("Created collection '{}' (id: {})", name, &id[..8]);
    Ok(id)
}

pub fn cmd_list_collections(db: &Database) -> Result<()> {
    let colls = db.list_collections()?;
    if colls.is_empty() {
        println!("No collections.");
        return Ok(());
    }
    println!("{:<8} {:<30} {}", "ID", "Name", "Created");
    println!("{}", "-".repeat(60));
    for (id, name, created_at) in colls {
        println!("{:<8} {:<30} {}", &id[..8], name, created_at);
    }
    Ok(())
}
