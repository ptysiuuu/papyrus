use std::path::PathBuf;
use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub default_sources: Vec<String>,
    pub default_limit: u32,
    pub timeout_seconds: u64,
    pub retries: u32,
    pub concurrent_requests: usize,
    pub default_sort: String,
    pub cache_ttl_minutes: u64,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_sources: vec!["arxiv".to_string(), "semantic".to_string()],
            default_limit: 20,
            timeout_seconds: 15,
            retries: 3,
            concurrent_requests: 4,
            default_sort: "relevance".to_string(),
            cache_ttl_minutes: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeysConfig {
    pub semantic_scholar: Option<String>,
    pub pubmed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub default_export_path: String,
    pub default_format: String,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            default_export_path: "~/papers".to_string(),
            default_format: "json".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub user_agent: String,
    pub polite_email: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            user_agent: "papyrus/0.1.0 (mailto:user@example.com)".to_string(),
            polite_email: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub show_abstracts_in_list: bool,
    pub color_theme: String,
    pub date_format: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_abstracts_in_list: false,
            color_theme: "dark".to_string(),
            date_format: "%Y-%m-%d".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub api_keys: ApiKeysConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

static FIRST_RUN_TEMPLATE: &str = r#"# papyrus configuration — ~/.config/papyrus/config.toml

[general]
default_sources = ["arxiv", "semantic"]
default_limit = 20
timeout_seconds = 15
retries = 3
concurrent_requests = 4
default_sort = "relevance"
cache_ttl_minutes = 60

[api_keys]
# Semantic Scholar key — https://www.semanticscholar.org/product/api
# semantic_scholar = ""
# PubMed key — https://www.ncbi.nlm.nih.gov/account/
# pubmed = ""

[output]
default_export_path = "~/papers"
default_format = "json"

[network]
# Include your email for CrossRef polite-pool priority access
user_agent = "papyrus/0.1.0 (mailto:user@example.com)"
polite_email = ""

[ui]
show_abstracts_in_list = false
color_theme = "dark"
date_format = "%Y-%m-%d"
"#;

impl Config {
    pub fn default_path() -> PathBuf {
        dirs_next::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("papyrus")
            .join("config.toml")
    }

    pub fn log_dir() -> PathBuf {
        dirs_next::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("papyrus")
    }

    pub fn cache_dir() -> PathBuf {
        Self::log_dir().join("cache")
    }

    /// Load config, creating a default template on first run.
    pub fn load(path: Option<&PathBuf>) -> anyhow::Result<Self> {
        let config_path = path.cloned().unwrap_or_else(Self::default_path);
        if !config_path.exists() {
            if path.is_none() {
                // First run — write template, then return defaults
                let _ = Self::write_template(&config_path);
            }
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Reading config at {:?}", config_path))?;
        toml::from_str(&content)
            .with_context(|| format!("Parsing config at {:?}", config_path))
    }

    fn write_template(path: &PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, FIRST_RUN_TEMPLATE)?;
        Ok(())
    }

    /// Persist the current config struct back to disk (overwrites comments).
    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .context("Serializing config")?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Set an API key for the given source name (`semantic` or `pubmed`).
    pub fn set_key(path: &PathBuf, source: &str, key: &str) -> anyhow::Result<()> {
        let mut config = Self::load(Some(path)).unwrap_or_default();
        match source.to_lowercase().as_str() {
            "semantic" | "semantic_scholar" | "s2" => {
                config.api_keys.semantic_scholar = Some(key.to_string());
            }
            "pubmed" => {
                config.api_keys.pubmed = Some(key.to_string());
            }
            other => anyhow::bail!("Unknown source '{}'. Valid values: semantic, pubmed", other),
        }
        config.save(path)
    }

    /// Remove an API key for the given source.
    pub fn remove_key(path: &PathBuf, source: &str) -> anyhow::Result<()> {
        let mut config = Self::load(Some(path)).unwrap_or_default();
        match source.to_lowercase().as_str() {
            "semantic" | "semantic_scholar" | "s2" => {
                config.api_keys.semantic_scholar = None;
            }
            "pubmed" => {
                config.api_keys.pubmed = None;
            }
            other => anyhow::bail!("Unknown source '{}'. Valid values: semantic, pubmed", other),
        }
        config.save(path)
    }

    /// Print configured keys with masked values.
    pub fn list_keys(config: &Config) {
        let mask = |opt: &Option<String>| match opt {
            None => "  (not configured)".to_string(),
            Some(k) if k.is_empty() => "  (empty)".to_string(),
            Some(k) => {
                let visible = k.len().min(4);
                format!("  {}{}…", &k[..visible], "*".repeat(8))
            }
        };
        println!("  semantic_scholar: {}", mask(&config.api_keys.semantic_scholar));
        println!("  pubmed:           {}", mask(&config.api_keys.pubmed));
    }

    /// Look up API key with priority: CLI flag → env var → config file.
    pub fn resolve_key(
        cli_override: Option<&str>,
        env_var: &str,
        config_val: Option<&str>,
    ) -> Option<String> {
        if let Some(k) = cli_override {
            if !k.is_empty() {
                return Some(k.to_string());
            }
        }
        if let Ok(k) = std::env::var(env_var) {
            if !k.is_empty() {
                return Some(k);
            }
        }
        config_val.filter(|k| !k.is_empty()).map(String::from)
    }
}

mod dirs_next {
    use std::path::PathBuf;

    pub fn config_dir() -> Option<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))
    }

    pub fn data_local_dir() -> Option<PathBuf> {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".local").join("share"))
            })
    }
}
