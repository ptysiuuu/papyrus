use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{Author, Paper, PaperSourceKind, ReadStatus};

pub struct Database {
    pub(crate) conn: Mutex<Connection>,
}

#[derive(Debug)]
pub struct LibraryStats {
    pub total_papers: usize,
    pub by_source: HashMap<String, usize>,
    pub by_year: HashMap<i32, usize>,
    pub by_tag: HashMap<String, usize>,
    pub by_read_status: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct WatchRecord {
    pub id: String,
    pub query: String,
    pub sources: Vec<String>,
    pub name: Option<String>,
    pub last_run_at: Option<String>,
    pub notify: bool,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Creating database directory {:?}", parent))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("Opening database {:?}", path))?;
        let db = Self { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_default() -> Result<Self> {
        let path = crate::config::Config::log_dir().join("papyrus.db");
        Self::open(path)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(())
    }

    // ─── Paper CRUD ──────────────────────────────────────────────────────────

    pub fn upsert_paper(&self, paper: &Paper) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let source = source_kind_to_str(&paper.source);
        let authors_json = serde_json::to_string(&paper.authors).unwrap_or_default();
        let categories_json = serde_json::to_string(&paper.categories).unwrap_or_default();
        let tags_json = serde_json::to_string(&paper.tags).unwrap_or_default();
        let published = paper.published_date.map(|d| d.to_string());
        let updated = paper.updated_date.map(|d| d.to_string());

        conn.execute(
            "INSERT INTO papers (
                id, source, source_id, title, authors_json, abstract_text,
                published_date, updated_date, categories_json, journal,
                doi, arxiv_id, pubmed_id, semantic_scholar_id,
                pdf_url, html_url, code_url,
                citation_count, reference_count, is_open_access, is_peer_reviewed,
                tags_json, tldr, pdf_path, full_text
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)
            ON CONFLICT(source, source_id) DO UPDATE SET
                title = COALESCE(excluded.title, title),
                authors_json = CASE WHEN excluded.authors_json != '[]' THEN excluded.authors_json ELSE authors_json END,
                abstract_text = COALESCE(excluded.abstract_text, abstract_text),
                published_date = COALESCE(excluded.published_date, published_date),
                updated_date = COALESCE(excluded.updated_date, updated_date),
                categories_json = CASE WHEN excluded.categories_json != '[]' THEN excluded.categories_json ELSE categories_json END,
                journal = COALESCE(excluded.journal, journal),
                doi = COALESCE(excluded.doi, doi),
                arxiv_id = COALESCE(excluded.arxiv_id, arxiv_id),
                pubmed_id = COALESCE(excluded.pubmed_id, pubmed_id),
                semantic_scholar_id = COALESCE(excluded.semantic_scholar_id, semantic_scholar_id),
                pdf_url = COALESCE(excluded.pdf_url, pdf_url),
                html_url = COALESCE(excluded.html_url, html_url),
                code_url = COALESCE(excluded.code_url, code_url),
                citation_count = COALESCE(excluded.citation_count, citation_count),
                reference_count = COALESCE(excluded.reference_count, reference_count),
                is_open_access = CASE WHEN excluded.is_open_access = 1 THEN 1 ELSE is_open_access END,
                is_peer_reviewed = CASE WHEN excluded.is_peer_reviewed = 1 THEN 1 ELSE is_peer_reviewed END,
                tldr = COALESCE(excluded.tldr, tldr),
                pdf_path = COALESCE(excluded.pdf_path, pdf_path),
                full_text = COALESCE(excluded.full_text, full_text)",
            params![
                paper.id, source, paper.source_id, paper.title, authors_json,
                paper.abstract_text, published, updated, categories_json, paper.journal,
                paper.doi, paper.arxiv_id, paper.pubmed_id, paper.semantic_scholar_id,
                paper.pdf_url, paper.html_url, paper.code_url,
                paper.citation_count, paper.reference_count,
                paper.is_open_access as i32, paper.is_peer_reviewed as i32,
                tags_json, paper.tldr, paper.pdf_path, paper.full_text,
            ],
        )?;

        // Sync FTS
        let fts_rowid: i64 = conn.query_row(
            "SELECT rowid FROM papers WHERE source=?1 AND source_id=?2",
            params![source, paper.source_id],
            |r| r.get(0),
        )?;
        let authors_text = paper.authors.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(" ");
        conn.execute(
            "INSERT OR REPLACE INTO paper_fts(rowid, paper_id, title, abstract_text, authors_text, full_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                fts_rowid, paper.id, paper.title,
                paper.abstract_text.as_deref().unwrap_or(""),
                authors_text,
                paper.full_text.as_deref().unwrap_or(""),
            ],
        )?;

        Ok(())
    }

    pub fn get_paper_by_id(&self, id: &str) -> Result<Option<Paper>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT * FROM papers WHERE id=?1",
            params![id],
            |row| row_to_paper(row),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn get_paper_by_source(&self, source: &str, source_id: &str) -> Result<Option<Paper>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT * FROM papers WHERE source=?1 AND source_id=?2",
            params![source, source_id],
            |row| row_to_paper(row),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn paper_count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM papers", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn list_papers(&self, limit: usize, offset: usize) -> Result<Vec<Paper>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT * FROM papers ORDER BY added_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let papers = stmt
            .query_map(params![limit as i64, offset as i64], |row| row_to_paper(row))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(papers)
    }

    // ─── FTS Search ──────────────────────────────────────────────────────────

    pub fn fts_search(&self, query: &str, include_fulltext: bool) -> Result<Vec<Paper>> {
        let conn = self.conn.lock().unwrap();
        let fts_query = if include_fulltext {
            format!("\"{}\"", query.replace('"', "\"\""))
        } else {
            // Search only title and abstract (columns 2,3)
            format!("{{title abstract_text}}: \"{}\"", query.replace('"', "\"\""))
        };
        let mut stmt = conn.prepare(
            "SELECT p.* FROM papers p
             JOIN paper_fts f ON p.rowid = f.rowid
             WHERE paper_fts MATCH ?1
             ORDER BY rank",
        )?;
        let papers = stmt
            .query_map(params![fts_query], |row| row_to_paper(row))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(papers)
    }

    // ─── Tags ────────────────────────────────────────────────────────────────

    pub fn add_tag(&self, paper_id: &str, tag: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO tags(paper_id, tag) VALUES(?1, ?2)",
            params![paper_id, tag],
        )?;
        Ok(())
    }

    pub fn remove_tag(&self, paper_id: &str, tag: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM tags WHERE paper_id=?1 AND tag=?2",
            params![paper_id, tag],
        )?;
        Ok(())
    }

    pub fn get_tags(&self, paper_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT tag FROM tags WHERE paper_id=?1 ORDER BY tag")?;
        let tags = stmt
            .query_map(params![paper_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(tags)
    }

    pub fn get_papers_by_tag(&self, tag: &str) -> Result<Vec<Paper>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.* FROM papers p JOIN tags t ON p.id=t.paper_id WHERE t.tag=?1",
        )?;
        let papers = stmt
            .query_map(params![tag], |row| row_to_paper(row))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(papers)
    }

    // ─── Collections ─────────────────────────────────────────────────────────

    pub fn create_collection(&self, name: &str) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO collections(id, name) VALUES(?1, ?2)",
            params![id, name],
        )?;
        Ok(id)
    }

    pub fn delete_collection(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM collections WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn list_collections(&self) -> Result<Vec<(String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, created_at FROM collections ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn add_to_collection(&self, collection_id: &str, paper_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO collection_papers(collection_id, paper_id) VALUES(?1, ?2)",
            params![collection_id, paper_id],
        )?;
        Ok(())
    }

    pub fn remove_from_collection(&self, collection_id: &str, paper_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM collection_papers WHERE collection_id=?1 AND paper_id=?2",
            params![collection_id, paper_id],
        )?;
        Ok(())
    }

    pub fn get_collection_papers(&self, collection_id: &str) -> Result<Vec<Paper>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.* FROM papers p
             JOIN collection_papers cp ON p.id=cp.paper_id
             WHERE cp.collection_id=?1",
        )?;
        let papers = stmt
            .query_map(params![collection_id], |row| row_to_paper(row))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(papers)
    }

    pub fn get_collection_id_by_name(&self, name: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id FROM collections WHERE name=?1",
            params![name],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    // ─── Library read status / notes / priority ───────────────────────────────

    pub fn set_read_status(&self, paper_id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE papers SET read_status=?1 WHERE id=?2",
            params![status, paper_id],
        )?;
        Ok(())
    }

    pub fn get_read_status(&self, paper_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT read_status FROM papers WHERE id=?1",
            params![paper_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn set_notes(&self, paper_id: &str, notes: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE papers SET notes=?1 WHERE id=?2",
            params![notes, paper_id],
        )?;
        Ok(())
    }

    pub fn set_priority(&self, paper_id: &str, priority: u8) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE papers SET priority=?1 WHERE id=?2",
            params![priority as i32, paper_id],
        )?;
        Ok(())
    }

    pub fn set_pdf_path(&self, paper_id: &str, path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE papers SET pdf_path=?1 WHERE id=?2",
            params![path, paper_id],
        )?;
        Ok(())
    }

    pub fn set_full_text(&self, paper_id: &str, text: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE papers SET full_text=?1 WHERE id=?2",
            params![text, paper_id],
        )?;
        // Re-sync FTS
        conn.execute(
            "INSERT OR REPLACE INTO paper_fts(rowid, paper_id, title, abstract_text, authors_text, full_text)
             SELECT rowid, id, title, COALESCE(abstract_text,''), authors_json, ?1
             FROM papers WHERE id=?2",
            params![text, paper_id],
        )?;
        Ok(())
    }

    // ─── Stats ───────────────────────────────────────────────────────────────

    pub fn stats(&self) -> Result<LibraryStats> {
        let conn = self.conn.lock().unwrap();

        let total: i64 = conn.query_row("SELECT COUNT(*) FROM papers", [], |r| r.get(0))?;

        let mut by_source = HashMap::new();
        let mut stmt = conn.prepare("SELECT source, COUNT(*) FROM papers GROUP BY source")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (src, cnt) = row?;
            by_source.insert(src, cnt as usize);
        }

        let mut by_year = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT CAST(substr(published_date,1,4) AS INTEGER), COUNT(*)
             FROM papers WHERE published_date IS NOT NULL
             GROUP BY substr(published_date,1,4)",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i64>(1)?)))? {
            let (yr, cnt) = row?;
            by_year.insert(yr, cnt as usize);
        }

        let mut by_tag = HashMap::new();
        let mut stmt = conn.prepare("SELECT tag, COUNT(*) FROM tags GROUP BY tag ORDER BY COUNT(*) DESC")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (tag, cnt) = row?;
            by_tag.insert(tag, cnt as usize);
        }

        let mut by_read_status = HashMap::new();
        let mut stmt = conn.prepare("SELECT read_status, COUNT(*) FROM papers GROUP BY read_status")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (status, cnt) = row?;
            by_read_status.insert(status, cnt as usize);
        }

        Ok(LibraryStats {
            total_papers: total as usize,
            by_source,
            by_year,
            by_tag,
            by_read_status,
        })
    }

    // ─── Citation edges ───────────────────────────────────────────────────────

    pub fn add_citation_edge(
        &self,
        citing_s2id: &str,
        cited_s2id: &str,
        context: Option<&str>,
        is_influential: bool,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO citation_edges(citing_s2id, cited_s2id, context, is_influential)
             VALUES(?1, ?2, ?3, ?4)",
            params![citing_s2id, cited_s2id, context, is_influential as i32],
        )?;
        Ok(())
    }

    pub fn get_references(&self, s2id: &str) -> Result<Vec<(String, Option<String>, bool)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT cited_s2id, context, is_influential FROM citation_edges WHERE citing_s2id=?1",
        )?;
        let rows = stmt
            .query_map(params![s2id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, bool>(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_citations(&self, s2id: &str) -> Result<Vec<(String, Option<String>, bool)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT citing_s2id, context, is_influential FROM citation_edges WHERE cited_s2id=?1",
        )?;
        let rows = stmt
            .query_map(params![s2id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, bool>(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ─── Citation graph nodes ─────────────────────────────────────────────────

    pub fn ensure_graph_node(&self, s2id: &str, title: &str, citation_count: Option<u64>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO citation_nodes(s2id, title, citation_count)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(s2id) DO UPDATE SET
                title = COALESCE(excluded.title, title),
                citation_count = COALESCE(excluded.citation_count, citation_count)",
            params![s2id, title, citation_count.map(|c| c as i64)],
        )?;
        Ok(())
    }

    pub fn get_graph_node(&self, s2id: &str) -> Result<Option<(String, Option<u64>)>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT title, citation_count FROM citation_nodes WHERE s2id=?1",
            params![s2id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?.map(|c| c as u64))),
        )
        .optional()
        .map_err(Into::into)
    }

    // ─── Watches ─────────────────────────────────────────────────────────────

    pub fn add_watch(
        &self,
        query: &str,
        sources: &[&str],
        name: Option<&str>,
        notify: bool,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let sources_json = serde_json::to_string(sources).unwrap_or_default();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO watches(id, query, sources_json, name, notify) VALUES(?1,?2,?3,?4,?5)",
            params![id, query, sources_json, name, notify as i32],
        )?;
        Ok(id)
    }

    pub fn remove_watch(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM watches WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn list_watches(&self) -> Result<Vec<WatchRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, query, sources_json, name, last_run_at, notify FROM watches ORDER BY rowid",
        )?;
        let records = stmt
            .query_map([], |r| {
                let sources_json: String = r.get(2)?;
                let sources: Vec<String> =
                    serde_json::from_str(&sources_json).unwrap_or_default();
                Ok(WatchRecord {
                    id: r.get(0)?,
                    query: r.get(1)?,
                    sources,
                    name: r.get(3)?,
                    last_run_at: r.get(4)?,
                    notify: r.get::<_, i32>(5)? != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(records)
    }

    pub fn update_watch_last_run(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE watches SET last_run_at=datetime('now') WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn was_watch_paper_seen(&self, watch_id: &str, paper_key: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM watch_seen WHERE watch_id=?1 AND paper_key=?2",
            params![watch_id, paper_key],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn mark_watch_paper_seen(&self, watch_id: &str, paper_key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO watch_seen(watch_id, paper_key) VALUES(?1, ?2)",
            params![watch_id, paper_key],
        )?;
        Ok(())
    }
}

// ─── Row deserialization ──────────────────────────────────────────────────────

fn row_to_paper(row: &rusqlite::Row<'_>) -> rusqlite::Result<Paper> {
    let source_str: String = row.get("source")?;
    let source = str_to_source_kind(&source_str);

    let authors_json: String = row.get("authors_json")?;
    let authors: Vec<Author> = serde_json::from_str(&authors_json).unwrap_or_default();

    let categories_json: String = row.get("categories_json")?;
    let categories: Vec<String> = serde_json::from_str(&categories_json).unwrap_or_default();

    let tags_json: String = row.get("tags_json")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    let published_date = row
        .get::<_, Option<String>>("published_date")?
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());
    let updated_date = row
        .get::<_, Option<String>>("updated_date")?
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());

    let read_status_str: String = row.get("read_status")?;
    let read_status = read_status_str.parse::<ReadStatus>().ok();

    let priority: Option<i32> = row.get("priority")?;

    Ok(Paper {
        id: row.get("id")?,
        source,
        source_id: row.get("source_id")?,
        title: row.get("title")?,
        authors,
        abstract_text: row.get("abstract_text")?,
        published_date,
        updated_date,
        categories,
        journal: row.get("journal")?,
        doi: row.get("doi")?,
        arxiv_id: row.get("arxiv_id")?,
        pubmed_id: row.get("pubmed_id")?,
        semantic_scholar_id: row.get("semantic_scholar_id")?,
        pdf_url: row.get("pdf_url")?,
        html_url: row.get("html_url")?,
        code_url: row.get("code_url")?,
        citation_count: row.get::<_, Option<i64>>("citation_count")?.map(|c| c as u32),
        reference_count: row.get::<_, Option<i64>>("reference_count")?.map(|c| c as u32),
        is_open_access: row.get::<_, i32>("is_open_access")? != 0,
        is_peer_reviewed: row.get::<_, i32>("is_peer_reviewed")? != 0,
        tags,
        tldr: row.get("tldr")?,
        read_status,
        notes: row.get("notes")?,
        priority: priority.map(|p| p as u8),
        pdf_path: row.get("pdf_path")?,
        full_text: row.get("full_text")?,
    })
}

pub(crate) fn source_kind_to_str(kind: &PaperSourceKind) -> &'static str {
    match kind {
        PaperSourceKind::Arxiv => "arxiv",
        PaperSourceKind::SemanticScholar => "semantic_scholar",
        PaperSourceKind::PubMed => "pubmed",
        PaperSourceKind::CrossRef => "crossref",
    }
}

pub(crate) fn str_to_source_kind(s: &str) -> PaperSourceKind {
    match s {
        "semantic_scholar" => PaperSourceKind::SemanticScholar,
        "pubmed" => PaperSourceKind::PubMed,
        "crossref" => PaperSourceKind::CrossRef,
        _ => PaperSourceKind::Arxiv,
    }
}

// ─── Schema ──────────────────────────────────────────────────────────────────

const SCHEMA_SQL: &str = "
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS papers (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    source_id TEXT NOT NULL,
    title TEXT NOT NULL,
    authors_json TEXT NOT NULL DEFAULT '[]',
    abstract_text TEXT,
    published_date TEXT,
    updated_date TEXT,
    categories_json TEXT NOT NULL DEFAULT '[]',
    journal TEXT,
    doi TEXT,
    arxiv_id TEXT,
    pubmed_id TEXT,
    semantic_scholar_id TEXT,
    pdf_url TEXT,
    html_url TEXT,
    code_url TEXT,
    citation_count INTEGER,
    reference_count INTEGER,
    is_open_access INTEGER NOT NULL DEFAULT 0,
    is_peer_reviewed INTEGER NOT NULL DEFAULT 0,
    tags_json TEXT NOT NULL DEFAULT '[]',
    tldr TEXT,
    read_status TEXT NOT NULL DEFAULT 'unread',
    notes TEXT,
    priority INTEGER,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    pdf_path TEXT,
    full_text TEXT,
    UNIQUE(source, source_id)
);

CREATE VIRTUAL TABLE IF NOT EXISTS paper_fts USING fts5(
    paper_id UNINDEXED,
    title,
    abstract_text,
    authors_text,
    full_text
);

CREATE TABLE IF NOT EXISTS tags (
    paper_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (paper_id, tag),
    FOREIGN KEY (paper_id) REFERENCES papers(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS collection_papers (
    collection_id TEXT NOT NULL,
    paper_id TEXT NOT NULL,
    PRIMARY KEY (collection_id, paper_id),
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (paper_id) REFERENCES papers(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS citation_nodes (
    s2id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    citation_count INTEGER
);

CREATE TABLE IF NOT EXISTS citation_edges (
    citing_s2id TEXT NOT NULL,
    cited_s2id TEXT NOT NULL,
    context TEXT,
    is_influential INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (citing_s2id, cited_s2id)
);
CREATE INDEX IF NOT EXISTS idx_ce_cited ON citation_edges(cited_s2id);

CREATE TABLE IF NOT EXISTS watches (
    id TEXT PRIMARY KEY,
    query TEXT NOT NULL,
    sources_json TEXT NOT NULL DEFAULT '[\"arxiv\",\"semantic_scholar\"]',
    name TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_run_at TEXT,
    notify INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS watch_seen (
    watch_id TEXT NOT NULL,
    paper_key TEXT NOT NULL,
    seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (watch_id, paper_key),
    FOREIGN KEY (watch_id) REFERENCES watches(id) ON DELETE CASCADE
);
";
