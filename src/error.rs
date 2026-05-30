use thiserror::Error;

#[derive(Debug, Error)]
pub enum PapyrusError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Rate limited by {src}, backing off")]
    RateLimited { src: String },

    #[error("Timeout contacting {src}")]
    Timeout { src: String },

    #[error("Config error: {0}")]
    Config(String),

    #[error("Export error: {0}")]
    Export(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, PapyrusError>;
