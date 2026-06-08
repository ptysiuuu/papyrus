use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::filters::FilterSet;
use crate::models::{Author, Paper, PaperSourceKind, SearchResult};
use crate::scraper::PaperSource;

/// Plugin manifest stored as `manifest.toml` inside the plugin directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    /// Binary name (must be on PATH or in plugin dir)
    pub binary: String,
    /// Source identifiers this plugin provides
    pub sources: Vec<String>,
}

/// JSON request sent to plugin binary on stdin.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginRequest {
    pub action: String,
    pub query: Option<String>,
    pub limit: u32,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// JSON response received from plugin binary on stdout.
#[derive(Debug, Deserialize)]
pub struct PluginResponse {
    pub papers: Vec<PluginPaper>,
    pub total: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PluginPaper {
    pub id: String,
    pub title: String,
    pub authors: Option<Vec<String>>,
    pub abstract_text: Option<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub pdf_url: Option<String>,
    pub citation_count: Option<u32>,
}

/// Discover all plugins in the plugin directory.
pub fn discover_plugins(plugin_dir: &Path) -> Result<Vec<PluginManifest>> {
    if !plugin_dir.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    for entry in std::fs::read_dir(plugin_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let manifest_path = entry.path().join("manifest.toml");
            if manifest_path.exists() {
                let content = std::fs::read_to_string(&manifest_path)
                    .with_context(|| format!("Reading {:?}", manifest_path))?;
                let manifest: PluginManifest = toml::from_str(&content)
                    .with_context(|| format!("Parsing {:?}", manifest_path))?;
                manifests.push(manifest);
            }
        }
    }
    Ok(manifests)
}

pub fn plugins_dir() -> PathBuf {
    crate::config::Config::default_path()
        .parent()
        .unwrap_or(Path::new("~/.config/papyrus"))
        .join("plugins")
}

/// PaperSource implementation that delegates to an external binary.
pub struct PluginSource {
    manifest: PluginManifest,
    binary_path: PathBuf,
}

impl PluginSource {
    pub fn new(manifest: PluginManifest, plugin_dir: &Path) -> Self {
        // Try to find binary in plugin dir first, then rely on PATH
        let local = plugin_dir.join(&manifest.name).join(&manifest.binary);
        let binary_path = if local.exists() { local } else { PathBuf::from(&manifest.binary) };
        Self { manifest, binary_path }
    }

    fn call(&self, request: &PluginRequest) -> Result<PluginResponse> {
        let json_in = serde_json::to_string(request).context("Serializing plugin request")?;

        let mut child = Command::new(&self.binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("Spawning plugin binary {:?}", self.binary_path))?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(json_in.as_bytes())?;
            stdin.write_all(b"\n")?;
        }

        let output = child.wait_with_output().context("Waiting for plugin")?;
        if !output.status.success() {
            anyhow::bail!(
                "Plugin '{}' exited with status {}",
                self.manifest.name,
                output.status
            );
        }

        let resp: PluginResponse = serde_json::from_slice(&output.stdout)
            .context("Parsing plugin response")?;
        Ok(resp)
    }
}

#[async_trait]
impl PaperSource for PluginSource {
    fn name(&self) -> &'static str {
        // Leak the string for 'static lifetime — acceptable since plugins live as long as the process
        Box::leak(self.manifest.name.clone().into_boxed_str())
    }

    async fn fetch(&self, filters: &FilterSet, _page: u32) -> anyhow::Result<SearchResult> {
        use chrono::NaiveDate;

        let request = PluginRequest {
            action: "search".to_string(),
            query: filters.query.clone(),
            limit: filters.limit,
            extra: {
                let mut m = HashMap::new();
                if let Some(from) = filters.date_from {
                    m.insert("date_from".to_string(), serde_json::Value::String(from.to_string()));
                }
                if let Some(to) = filters.date_to {
                    m.insert("date_to".to_string(), serde_json::Value::String(to.to_string()));
                }
                m
            },
        };

        let _source_name = self.manifest.sources.first().cloned().unwrap_or_default();
        let resp = self.call(&request)?;

        let papers: Vec<Paper> = resp
            .papers
            .into_iter()
            .map(|pp| {
                let mut p = Paper::new(PaperSourceKind::Arxiv, &pp.id, &pp.title);
                p.source_id = pp.id;
                p.abstract_text = pp.abstract_text;
                p.doi = pp.doi;
                p.html_url = pp.url;
                p.pdf_url = pp.pdf_url;
                p.citation_count = pp.citation_count;
                if let Some(year) = pp.year {
                    p.published_date = NaiveDate::from_ymd_opt(year, 1, 1);
                }
                if let Some(authors) = pp.authors {
                    p.authors = authors
                        .into_iter()
                        .map(|name| Author { name, affiliation: None, orcid: None })
                        .collect();
                }
                p
            })
            .collect();

        Ok(SearchResult {
            total_count: resp.total,
            papers,
            source: PaperSourceKind::Arxiv, // placeholder
        })
    }
}
