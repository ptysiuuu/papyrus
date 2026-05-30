pub mod arxiv;
pub mod crossref;
pub mod pubmed;
pub mod semantic_scholar;

use async_trait::async_trait;

use crate::filters::FilterSet;
use crate::models::SearchResult;

#[async_trait]
pub trait PaperSource: Send + Sync {
    async fn fetch(&self, filters: &FilterSet, page: u32) -> anyhow::Result<SearchResult>;
    fn name(&self) -> &'static str;
}

pub use arxiv::ArxivSource;
pub use crossref::CrossRefSource;
pub use pubmed::PubMedSource;
pub use semantic_scholar::SemanticScholarSource;
