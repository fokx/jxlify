use crate::config::JxlifyConfig;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, warn};
use walkdir::WalkDir;

struct FileItem {
    path: PathBuf,
    mod_time: SystemTime,
    size: u64,
}

/// Calculate total size in bytes of regular files under directory
fn get_dir_size(dir: &Path) -> u64 {
    if !dir.exists() {
        return 0;
    }
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// List all files with modification time and size
fn list_files(dir: &Path) -> Vec<FileItem> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                let mod_time = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                files.push(FileItem {
                    path: entry.path().to_path_buf(),
                    mod_time,
                    size: meta.len(),
                });
            }
        }
    }
    files
}

/// Remove oldest files until total dir size is within max_bytes
fn enforce_dir_size_limit(dir: &Path, max_bytes: u64) {
    let current_size = get_dir_size(dir);
    if current_size <= max_bytes {
        return;
    }

    let mut files = list_files(dir);
    // Sort ascending: oldest first
    files.sort_by_key(|f| f.mod_time);

    let mut remaining_size = current_size;
    for file in files {
        if remaining_size <= max_bytes {
            break;
        }
        if let Err(e) = std::fs::remove_file(&file.path) {
            warn!("Failed to delete cached file {}: {}", file.path.display(), e);
        } else {
            debug!("Purged cached file: {}", file.path.display());
            remaining_size = remaining_size.saturating_sub(file.size);
        }
    }
}

/// Clean stale temporary files (.tmp.*) older than 10 minutes
fn clean_stale_temp_files(dir: &Path) {
    if !dir.exists() {
        return;
    }
    let threshold = SystemTime::now() - Duration::from_secs(600);
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.contains(".tmp.") {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(mod_time) = meta.modified() {
                            if mod_time < threshold {
                                let _ = std::fs::remove_file(entry.path());
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Background task to clean caches periodically
pub async fn start_cache_cleaner(config: JxlifyConfig) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;

        let cache_root = Path::new(&config.cache_path);

        clean_stale_temp_files(cache_root);

        if config.max_cache_size > 0 {
            let max_bytes = config.max_cache_size * 1024 * 1024;
            enforce_dir_size_limit(cache_root, max_bytes);
        }
    }
}
