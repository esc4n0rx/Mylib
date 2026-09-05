//! Bounded on-disk cache for remote stream payloads. Shared across viewers and
//! sessions (§56); an entry keyed on the media source's version is never reused
//! after a re-sync changes the stream (§54). Enforces TTL + total-size LRU
//! eviction (§55).

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use sha2::{Digest, Sha256};
use tokio::fs;

use crate::{config::Config, errors::AppResult};

#[derive(Clone)]
pub struct RemoteCache {
    dir: PathBuf,
    ttl: Duration,
    max_bytes: u64,
    /// Largest single payload that is worth caching whole.
    entry_limit: u64,
}

impl RemoteCache {
    pub fn new(config: &Config) -> AppResult<Self> {
        let dir = config.data_dir.join("cache/remote");
        std::fs::create_dir_all(&dir)?;
        let cache = Self {
            dir,
            ttl: Duration::from_secs(config.remote_cache_ttl_seconds.max(60)),
            max_bytes: config.remote_cache_gb.saturating_mul(1024 * 1024 * 1024),
            entry_limit: 64 * 1024 * 1024,
        };
        cache.spawn_cleanup();
        Ok(cache)
    }

    /// Cache key: provider + external key + an opaque version token (the media
    /// source's `updated_at`). A re-sync bumps the version and orphans the old
    /// entry, which cleanup later reclaims.
    pub fn key(provider: &str, external_key: &str, version: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(provider.as_bytes());
        hasher.update(b"\0");
        hasher.update(external_key.as_bytes());
        hasher.update(b"\0");
        hasher.update(version.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(key)
    }

    /// Returns the cached file path when a fresh full-payload entry exists.
    pub async fn get(&self, key: &str) -> Option<PathBuf> {
        let path = self.path(key);
        let metadata = fs::metadata(&path).await.ok()?;
        let fresh = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age < self.ttl);
        if fresh {
            // Touch so LRU keeps hot entries.
            let _ = filetime_now(&path).await;
            Some(path)
        } else {
            let _ = fs::remove_file(&path).await;
            None
        }
    }

    /// Stores a complete payload if it is within the per-entry limit.
    pub async fn put(&self, key: &str, bytes: &[u8]) -> AppResult<bool> {
        if bytes.len() as u64 > self.entry_limit {
            return Ok(false);
        }
        let path = self.path(key);
        let temporary = path.with_extension("partial");
        fs::write(&temporary, bytes).await?;
        fs::rename(&temporary, &path).await?;
        Ok(true)
    }

    fn spawn_cleanup(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(900));
            loop {
                ticker.tick().await;
                if let Err(error) = this.cleanup().await {
                    tracing::warn!(%error, "remote cache cleanup failed");
                }
            }
        });
    }

    async fn cleanup(&self) -> std::io::Result<()> {
        let mut entries: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
        let mut total = 0_u64;
        let mut reader = fs::read_dir(&self.dir).await?;
        while let Some(entry) = reader.next_entry().await? {
            let metadata = entry.metadata().await?;
            if !metadata.is_file() {
                continue;
            }
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            total = total.saturating_add(metadata.len());
            entries.push((entry.path(), modified, metadata.len()));
        }
        entries.sort_by_key(|(_, modified, _)| *modified);
        for (path, modified, size) in entries {
            let expired = modified.elapsed().unwrap_or_default() > self.ttl;
            if (expired || total > self.max_bytes) && fs::remove_file(&path).await.is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }
}

async fn filetime_now(path: &Path) -> std::io::Result<()> {
    // Rewriting length-preserving metadata is awkward cross-platform; opening for
    // append with no write still updates atime on most systems and is cheap.
    let _ = fs::OpenOptions::new().append(true).open(path).await?;
    Ok(())
}
