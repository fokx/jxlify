#![allow(dead_code)]
use crate::config::{ExtraParams, JxlifyConfig};
use crate::helper::{hash_bytes, hash_file, hash_string};
use image::GenericImageView;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageMeta {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub size: usize,
    pub num_pages: usize,
    pub blurhash: String,
    pub colorspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetaFile {
    pub id: String,
    pub path: String,
    pub checksum: String,
    #[serde(flatten)]
    pub meta: ImageMeta,
}

/// Compute metadata ID and file paths for request
pub fn get_metadata_id(
    req_path: &str,
    extra: &ExtraParams,
    config: &JxlifyConfig,
    subdir: &str,
) -> (String, PathBuf) {
    let sanitized_query = format!(
        "{}?width={}&height={}&max_width={}&max_height={}",
        req_path,
        extra.width.map(|v| v.to_string()).unwrap_or_default(),
        extra.height.map(|v| v.to_string()).unwrap_or_default(),
        extra.max_width.map(|v| v.to_string()).unwrap_or_default(),
        extra.max_height.map(|v| v.to_string()).unwrap_or_default(),
    );

    let id = hash_string(&sanitized_query);
    let meta_path = crate::helper::get_sharded_path(
        &config.metadata_cache_path(),
        subdir,
        &id,
        "json",
    );

    (id, meta_path)
}

/// Read existing metadata from disk or build it from raw file
pub fn read_or_build_metadata(
    req_path: &str,
    raw_path: &Path,
    extra: &ExtraParams,
    config: &JxlifyConfig,
    subdir: &str,
) -> MetaFile {
    let (id, meta_json_path) = get_metadata_id(req_path, extra, config, subdir);

    if meta_json_path.exists() {
        if let Ok(file) = File::open(&meta_json_path) {
            let reader = BufReader::new(file);
            if let Ok(meta_file) = serde_json::from_reader::<_, MetaFile>(reader) {
                // Verify checksum if file exists
                if let Ok(current_checksum) = hash_file(raw_path) {
                    if meta_file.checksum == current_checksum {
                        return meta_file;
                    }
                    debug!("Source file checksum changed for {}, rebuilding metadata", req_path);
                } else {
                    return meta_file;
                }
            }
        }
    }

    // Build new metadata
    let meta_file = build_metadata(&id, req_path, raw_path);
    save_metadata(&meta_json_path, &meta_file);
    meta_file
}

/// Build image metadata struct by inspecting image buffer / dimensions
pub fn build_metadata(id: &str, req_path: &str, raw_path: &Path) -> MetaFile {
    let checksum = hash_file(raw_path).unwrap_or_default();
    let file_size = std::fs::metadata(raw_path).map(|m| m.len() as usize).unwrap_or(0);

    let mut image_meta = ImageMeta {
        size: file_size,
        format: crate::helper::get_extension(&raw_path.to_string_lossy()),
        colorspace: "sRGB".to_string(),
        num_pages: 1,
        ..Default::default()
    };

    if let Ok(img) = crate::encoder::load_dynamic_image(raw_path) {
        let (w, h) = img.dimensions();
        image_meta.width = w;
        image_meta.height = h;

        // Generate blurhash
        let small = img.thumbnail_exact(32, 32);
        let rgba = small.to_rgba8();
        if let Ok(hash) = blurhash::encode(4, 3, 32, 32, rgba.as_raw()) {
            image_meta.blurhash = hash;
        }
    }

    MetaFile {
        id: id.to_string(),
        path: req_path.to_string(),
        checksum,
        meta: image_meta,
    }
}

/// Build metadata directly from in-memory bytes (for remote downloads)
pub fn build_metadata_from_bytes(id: &str, req_path: &str, bytes: &[u8]) -> MetaFile {
    let checksum = hash_bytes(bytes);
    let ext = crate::helper::get_extension(req_path);
    let mut image_meta = ImageMeta {
        size: bytes.len(),
        format: ext.clone(),
        colorspace: "sRGB".to_string(),
        num_pages: 1,
        ..Default::default()
    };

    let decoded_img = if ext == "avif" {
        crate::encoder::decode_avif_bytes(bytes).ok()
    } else {
        image::load_from_memory(bytes).ok()
    };

    if let Some(img) = decoded_img {
        let (w, h) = img.dimensions();
        image_meta.width = w;
        image_meta.height = h;

        let small = img.thumbnail_exact(32, 32);
        let rgba = small.to_rgba8();
        if let Ok(hash) = blurhash::encode(4, 3, 32, 32, rgba.as_raw()) {
            image_meta.blurhash = hash;
        }
    }

    MetaFile {
        id: id.to_string(),
        path: req_path.to_string(),
        checksum,
        meta: image_meta,
    }
}

/// Save metadata to disk
pub fn save_metadata(meta_path: &Path, meta_file: &MetaFile) {
    if let Some(parent) = meta_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json_str) = serde_json::to_string(meta_file) {
        let _ = std::fs::write(meta_path, json_str.as_bytes());
    }
}

/// Delete metadata file if raw file is missing or corrupted
pub fn delete_metadata(meta_path: &Path) {
    if meta_path.exists() {
        let _ = std::fs::remove_file(meta_path);
    }
}
