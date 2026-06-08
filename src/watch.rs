use anyhow::Result;

use crate::db::Database;
use crate::models::Paper;

pub struct WatchRunner {
    db: Database,
}

impl WatchRunner {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Filter a list of papers to only those not previously seen for this watch.
    /// Also marks new papers as seen.
    pub fn filter_new_papers(&self, watch_id: &str, papers: &[Paper]) -> Result<Vec<Paper>> {
        let mut new_papers = Vec::new();
        for paper in papers {
            let key = format!("{}:{}", crate::db::source_kind_to_str(&paper.source), paper.source_id);
            if !self.db.was_watch_paper_seen(watch_id, &key)? {
                self.db.mark_watch_paper_seen(watch_id, &key)?;
                new_papers.push(paper.clone());
            }
        }
        Ok(new_papers)
    }

    pub fn update_last_run(&self, watch_id: &str) -> Result<()> {
        self.db.update_watch_last_run(watch_id)
    }
}

/// Print watch results as JSONL to stdout — suitable for piping or cron use.
pub fn emit_jsonl(papers: &[Paper], watch_name: Option<&str>, watch_query: &str) {
    for paper in papers {
        let mut val = serde_json::to_value(paper).unwrap_or_default();
        if let serde_json::Value::Object(ref mut m) = val {
            m.insert("__watch_query".to_string(), serde_json::Value::String(watch_query.to_string()));
            if let Some(name) = watch_name {
                m.insert("__watch_name".to_string(), serde_json::Value::String(name.to_string()));
            }
        }
        println!("{}", serde_json::to_string(&val).unwrap_or_default());
    }
}
