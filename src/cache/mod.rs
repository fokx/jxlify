pub mod cleaner;
pub mod metadata;

use crate::config::JxlifyConfig;
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct CacheManager {
    /// In-flight locks by cache key (id + format) to prevent concurrent duplicate encoding
    in_flight_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl CacheManager {
    pub fn new() -> Self {
        Self {
            in_flight_locks: Arc::new(DashMap::new()),
        }
    }

    /// Obtain or create an async lock for a specific cache conversion key
    pub fn get_lock(&self, key: &str) -> Arc<Mutex<()>> {
        self.in_flight_locks
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Clean up lock entry after conversion completes
    pub fn remove_lock(&self, key: &str) {
        self.in_flight_locks.remove(key);
    }

    /// Build sharded destination path in cache directory: `<cache_path>/images/<subdir>/<id[0..2]>/<id[2..4]>/<id>.<ext>`
    pub fn get_cache_path(&self, id: &str, ext: &str, subdir: &str, config: &JxlifyConfig) -> PathBuf {
        crate::helper::get_sharded_path(&config.images_cache_path(), subdir, id, ext)
    }

    /// Check if target converted file exists in image cache
    pub fn is_cached(&self, cached_path: &Path) -> bool {
        crate::helper::image_exists(cached_path)
    }
}
