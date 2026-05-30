use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::Context;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub default_sources: Vec<String>,
    pub default_limit: u32,
    pub timeout_seconds: u64,
    pub retries: u32,
    pub concurrent_requests: usize,
    pub default_sort: String,
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

    pub fn load(path: Option<&PathBuf>) -> anyhow::Result<Self> {
        let config_path = path.cloned().unwrap_or_else(Self::default_path);
        if !config_path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Reading config at {:?}", config_path))?;
        toml::from_str(&content)
            .with_context(|| format!("Parsing config at {:?}", config_path))
    }
}

// Minimal dirs_next shim using env vars
mod dirs_next {
    use std::path::PathBuf;

    pub fn config_dir() -> Option<PathBuf> {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config"))
            })
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
