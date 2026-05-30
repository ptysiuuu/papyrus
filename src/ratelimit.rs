use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};

pub type Limiter = DefaultDirectRateLimiter;

pub fn make(quota: Quota) -> Arc<Limiter> {
    Arc::new(RateLimiter::direct(quota))
}

// arXiv: 1 req / 3 sec
pub fn arxiv() -> Arc<Limiter> {
    make(Quota::with_period(Duration::from_secs(3)).expect("valid period"))
}

// Semantic Scholar with key: 1 req / sec
pub fn semantic_keyed() -> Arc<Limiter> {
    make(Quota::per_second(NonZeroU32::new(1).unwrap()))
}

// Semantic Scholar without key: 100 req / 5 min ≈ 20 req / min
pub fn semantic_unkeyed() -> Arc<Limiter> {
    make(Quota::per_minute(NonZeroU32::new(20).unwrap()))
}

// PubMed with key: 10 req / sec
pub fn pubmed_keyed() -> Arc<Limiter> {
    make(Quota::per_second(NonZeroU32::new(10).unwrap()))
}

// PubMed without key: 3 req / sec
pub fn pubmed_unkeyed() -> Arc<Limiter> {
    make(Quota::per_second(NonZeroU32::new(3).unwrap()))
}

// CrossRef polite pool: 4 req / sec
pub fn crossref() -> Arc<Limiter> {
    make(Quota::per_second(NonZeroU32::new(4).unwrap()))
}

/// Async throttle — checks every 50ms until a token is available.
pub async fn throttle(limiter: &Limiter) {
    while limiter.check().is_err() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
