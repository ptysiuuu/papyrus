use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};

use crate::filters::FilterSet;
use crate::models::Paper;

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    timestamp_secs: u64,
    total_count: Option<u64>,
    papers: Vec<Paper>,
}

pub struct DiskCache {
    dir: PathBuf,
    ttl: Duration,
}

impl DiskCache {
    pub fn new(dir: PathBuf, ttl_minutes: u64) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            ttl: Duration::from_secs(ttl_minutes * 60),
        })
    }

    /// SHA-256 of `"source_name:json(filter_set)"` — stable cache key.
    pub fn cache_key(filters: &FilterSet, source_name: &str) -> String {
        let json = serde_json::to_string(filters).unwrap_or_default();
        let input = format!("{}:{}", source_name, json);
        let hash = Sha256::digest(input.as_bytes());
        hash.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn get(&self, key: &str) -> Option<(Vec<Paper>, Option<u64>)> {
        let path = self.dir.join(format!("{}.json.gz", key));
        if !path.exists() {
            return None;
        }

        let data = fs::read(&path).ok()?;
        let mut gz = GzDecoder::new(data.as_slice());
        let mut json = String::new();
        gz.read_to_string(&mut json).ok()?;
        let entry: CacheEntry = serde_json::from_str(&json).ok()?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now.saturating_sub(entry.timestamp_secs) > self.ttl.as_secs() {
            let _ = fs::remove_file(&path);
            return None;
        }

        Some((entry.papers, entry.total_count))
    }

    pub fn put(&self, key: &str, papers: &[Paper], total_count: Option<u64>) -> anyhow::Result<()> {
        let entry = CacheEntry {
            timestamp_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            total_count,
            papers: papers.to_vec(),
        };
        let json = serde_json::to_string(&entry)?;
        let path = self.dir.join(format!("{}.json.gz", key));
        let file = fs::File::create(&path)?;
        let mut gz = GzEncoder::new(file, Compression::default());
        gz.write_all(json.as_bytes())?;
        gz.finish()?;
        Ok(())
    }

    /// Remove all `.json.gz` files in the cache directory. Returns count deleted.
    pub fn clear(&self) -> anyhow::Result<usize> {
        let mut count = 0;
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.path().extension().map_or(false, |e| e == "gz") {
                fs::remove_file(entry.path())?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Returns (entry_count, total_bytes_on_disk).
    pub fn stats(&self) -> (usize, u64) {
        let mut count = 0;
        let mut total = 0u64;
        if let Ok(dir) = fs::read_dir(&self.dir) {
            for e in dir.flatten() {
                if e.path().extension().map_or(false, |x| x == "gz") {
                    count += 1;
                    total += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
        (count, total)
    }
}
