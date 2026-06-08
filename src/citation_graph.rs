use std::collections::{HashSet, VecDeque};

use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

use crate::db::Database;

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub s2id: String,
    pub title: String,
    pub citation_count: Option<u64>,
}

pub struct CitationGraphStore {
    db: Database,
}

impl CitationGraphStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn ensure_node(&self, s2id: &str, title: &str, citation_count: Option<u64>) -> Result<()> {
        self.db.ensure_graph_node(s2id, title, citation_count)
    }

    pub fn add_reference(
        &self,
        citing_s2id: &str,
        cited_s2id: &str,
        context: Option<&str>,
        is_influential: bool,
    ) -> Result<()> {
        self.db.add_citation_edge(citing_s2id, cited_s2id, context, is_influential)
    }

    pub fn get_references(&self, s2id: &str) -> Result<Vec<GraphNode>> {
        let edges = self.db.get_references(s2id)?;
        let mut nodes = Vec::new();
        for (cited_id, _, _) in edges {
            if let Some((title, citation_count)) = self.db.get_graph_node(&cited_id)? {
                nodes.push(GraphNode { s2id: cited_id, title, citation_count });
            } else {
                nodes.push(GraphNode { s2id: cited_id, title: "(unknown)".to_string(), citation_count: None });
            }
        }
        Ok(nodes)
    }

    pub fn get_citations(&self, s2id: &str) -> Result<Vec<GraphNode>> {
        let edges = self.db.get_citations(s2id)?;
        let mut nodes = Vec::new();
        for (citing_id, _, _) in edges {
            if let Some((title, citation_count)) = self.db.get_graph_node(&citing_id)? {
                nodes.push(GraphNode { s2id: citing_id, title, citation_count });
            } else {
                nodes.push(GraphNode { s2id: citing_id, title: "(unknown)".to_string(), citation_count: None });
            }
        }
        Ok(nodes)
    }

    /// Walk backward (through references) up to `depth` hops. Returns all visited nodes except the root.
    pub fn ancestors(&self, root_s2id: &str, depth: usize) -> Result<Vec<GraphNode>> {
        self.bfs_traverse(root_s2id, depth, TraverseDir::Backward)
    }

    /// Walk forward (through citations) up to `depth` hops. Returns all visited nodes except the root.
    pub fn descendants(&self, root_s2id: &str, depth: usize) -> Result<Vec<GraphNode>> {
        self.bfs_traverse(root_s2id, depth, TraverseDir::Forward)
    }

    fn bfs_traverse(&self, root: &str, max_depth: usize, dir: TraverseDir) -> Result<Vec<GraphNode>> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        let mut result: Vec<GraphNode> = Vec::new();

        visited.insert(root.to_string());
        queue.push_back((root.to_string(), 0));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            let neighbors = match dir {
                TraverseDir::Backward => self.db.get_references(&current)?,
                TraverseDir::Forward => self.db.get_citations(&current)?,
            };

            for (neighbor_id, _, _) in neighbors {
                if !visited.contains(&neighbor_id) {
                    visited.insert(neighbor_id.clone());
                    let node = if let Some((title, cc)) = self.db.get_graph_node(&neighbor_id)? {
                        GraphNode { s2id: neighbor_id.clone(), title, citation_count: cc }
                    } else {
                        GraphNode { s2id: neighbor_id.clone(), title: "(unknown)".to_string(), citation_count: None }
                    };
                    result.push(node);
                    queue.push_back((neighbor_id, depth + 1));
                }
            }
        }

        Ok(result)
    }

    /// Find papers cited by both id1 and id2.
    pub fn common_references(&self, id1: &str, id2: &str) -> Result<Vec<GraphNode>> {
        let refs1: HashSet<String> = self.db
            .get_references(id1)?
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();

        let refs2: HashSet<String> = self.db
            .get_references(id2)?
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();

        let common: HashSet<&String> = refs1.intersection(&refs2).collect();
        let mut nodes = Vec::new();
        for s2id in common {
            if let Some((title, cc)) = self.db.get_graph_node(s2id)? {
                nodes.push(GraphNode { s2id: s2id.clone(), title, citation_count: cc });
            }
        }
        Ok(nodes)
    }

    /// Find highest-cited root nodes (nodes with no known citing papers in the graph, ranked by citation count).
    pub fn seminal_nodes(&self, limit: usize) -> Result<Vec<GraphNode>> {
        // Get all nodes that have outgoing reference edges (i.e., things that ARE referenced)
        // and rank by citation count
        let conn_guard = self.db.conn.lock().unwrap();
        let mut stmt = conn_guard.prepare(
            "SELECT s2id, title, citation_count FROM citation_nodes
             ORDER BY COALESCE(citation_count, 0) DESC
             LIMIT ?1",
        )?;
        let nodes = stmt
            .query_map(rusqlite::params![limit as i64], |r| {
                Ok(GraphNode {
                    s2id: r.get(0)?,
                    title: r.get(1)?,
                    citation_count: r.get::<_, Option<i64>>(2)?.map(|c| c as u64),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(nodes)
    }
}

enum TraverseDir {
    Forward,
    Backward,
}

// ─── API client for fetching from Semantic Scholar ───────────────────────────

pub struct CitationGraphClient {
    client: Client,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2CitationResponse {
    data: Vec<S2CitationEntry>,
    next: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct S2CitationEntry {
    citing_paper: Option<S2CiteNode>,
    cited_paper: Option<S2CiteNode>,
    contexts: Option<Vec<String>>,
    is_influential: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct S2CiteNode {
    paper_id: Option<String>,
    title: Option<String>,
    citation_count: Option<u64>,
}

impl CitationGraphClient {
    pub fn new(client: Client, api_key: Option<String>) -> Self {
        Self { client, api_key }
    }

    /// Fetch references for a paper and store in the graph.
    pub async fn fetch_and_store_references(
        &self,
        s2id: &str,
        store: &CitationGraphStore,
        limit: u32,
    ) -> Result<usize> {
        let url = format!(
            "https://api.semanticscholar.org/graph/v1/paper/{}/references",
            s2id
        );
        let mut req = self.client.get(&url).query(&[
            ("fields", "paperId,title,citationCount,isInfluential,contexts"),
            ("limit", &limit.to_string()),
        ]);
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }
        let resp: S2CitationResponse = req.send().await?.json().await?;
        let count = resp.data.len();

        for entry in resp.data {
            if let Some(cited) = entry.cited_paper {
                let cited_id = match cited.paper_id {
                    Some(id) => id,
                    None => continue,
                };
                let title = cited.title.unwrap_or_default();
                store.ensure_node(&cited_id, &title, cited.citation_count)?;
                let context = entry.contexts.as_deref().and_then(|c| c.first()).map(String::as_str);
                store.add_reference(s2id, &cited_id, context, entry.is_influential.unwrap_or(false))?;
            }
        }
        Ok(count)
    }

    /// Fetch citations for a paper and store in the graph.
    pub async fn fetch_and_store_citations(
        &self,
        s2id: &str,
        store: &CitationGraphStore,
        limit: u32,
    ) -> Result<usize> {
        let url = format!(
            "https://api.semanticscholar.org/graph/v1/paper/{}/citations",
            s2id
        );
        let mut req = self.client.get(&url).query(&[
            ("fields", "paperId,title,citationCount,isInfluential,contexts"),
            ("limit", &limit.to_string()),
        ]);
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }
        let resp: S2CitationResponse = req.send().await?.json().await?;
        let count = resp.data.len();

        for entry in resp.data {
            if let Some(citing) = entry.citing_paper {
                let citing_id = match citing.paper_id {
                    Some(id) => id,
                    None => continue,
                };
                let title = citing.title.unwrap_or_default();
                store.ensure_node(&citing_id, &title, citing.citation_count)?;
                let context = entry.contexts.as_deref().and_then(|c| c.first()).map(String::as_str);
                store.add_reference(&citing_id, s2id, context, entry.is_influential.unwrap_or(false))?;
            }
        }
        Ok(count)
    }
}
