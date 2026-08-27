#![allow(dead_code)]
pub mod mime;

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use xxhash_rust::xxh64::xxh64;

/// Hash a string using xxhash64 for ultra fast collision-resistant hashing
pub fn hash_string(s: &str) -> String {
    format!("{:016x}", xxh64(s.as_bytes(), 0))
}

/// Hash a file's contents using xxhash64
pub fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = xxhash_rust::xxh64::Xxh64::new(0);
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:016x}", hasher.digest()))
}

/// Hash raw bytes using xxhash64
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:016x}", xxh64(bytes, 0))
}

/// Sanitize a request path to prevent path traversal attacks (e.g. `../`)
pub fn sanitize_request_path(raw_path: &str) -> Option<PathBuf> {
    let unescaped = urlencoding::decode(raw_path).ok()?;
    let path = Path::new(unescaped.as_ref());
    let mut clean_path = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(c) => clean_path.push(c),
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    Some(clean_path)
}

/// Check if a path contains directory traversal segments
pub fn has_traversal(p: &str) -> bool {
    p.contains("..") || p.contains("./") && p.starts_with('.')
}

/// Safely check if image file exists and is non-empty
pub fn image_exists(path: &Path) -> bool {
    if let Ok(metadata) = std::fs::metadata(path) {
        metadata.is_file() && metadata.len() > 0
    } else {
        false
    }
}

/// Atomic file copy (writes to temp file in same directory and renames)
pub fn atomic_write(dst: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_name = format!("{}.tmp.{}", dst.to_string_lossy(), fastrand::u64(..));
    let temp_path = PathBuf::from(temp_name);
    std::fs::write(&temp_path, content)?;
    std::fs::rename(&temp_path, dst)
}

/// Calculate compression rate: float string representation e.g. "0.42"
pub fn get_compression_rate(raw_size: u64, opt_size: u64) -> String {
    if raw_size == 0 {
        return "1.00".to_string();
    }
    format!("{:.2}", opt_size as f64 / raw_size as f64)
}

/// Extract image file extension without leading dot in lowercase
pub fn get_extension(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default()
}

/// Generate a 2-level sharded file path to prevent millions of files in a single directory
/// e.g. <base>/<subdir>/<id[0..2]>/<id[2..4]>/<id>.<ext>
pub fn get_sharded_path(base_dir: &Path, subdir: &str, id: &str, ext: &str) -> PathBuf {
    if id.len() >= 4 {
        base_dir
            .join(subdir)
            .join(&id[0..2])
            .join(&id[2..4])
            .join(format!("{}.{}", id, ext))
    } else {
        base_dir
            .join(subdir)
            .join(format!("{}.{}", id, ext))
    }
}
