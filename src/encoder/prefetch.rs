use crate::cache::metadata::read_or_build_metadata;
use crate::config::{ExtraParams, JxlifyConfig};
use crate::encoder::convert_to_target;
use crate::negotiation::NegotiatedFormat;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
use walkdir::WalkDir;

pub async fn prefetch_images(config: JxlifyConfig, jobs: usize) {
    info!("Starting prefetch image scan on: {}", config.img_path);
    let img_dir = Path::new(&config.img_path);
    if !img_dir.exists() {
        warn!("Image directory {} does not exist, skipping prefetch.", config.img_path);
        return;
    }

    let mut image_files = Vec::new();
    for entry in WalkDir::new(img_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let path = entry.path().to_path_buf();
            let ext = crate::helper::get_extension(&path.to_string_lossy());
            if config.is_allowed_extension(&ext) {
                image_files.push(path);
            }
        }
    }

    let total = image_files.len();
    info!("Found {} candidate images for prefetching with {} workers", total, jobs);
    if total == 0 {
        return;
    }

    let config_arc = Arc::new(config);
    let counter = Arc::new(AtomicUsize::new(0));
    let chunk_size = (total / jobs).max(1);

    let mut handles = Vec::new();
    for chunk in image_files.chunks(chunk_size) {
        let chunk = chunk.to_vec();
        let cfg = Arc::clone(&config_arc);
        let cnt = Arc::clone(&counter);

        let handle = tokio::task::spawn_blocking(move || {
            let extra = ExtraParams::default();
            for file in chunk {
                let ext = crate::helper::get_extension(&file.to_string_lossy());
                let relative = file.strip_prefix(Path::new(&cfg.img_path))
                    .unwrap_or(&file)
                    .to_string_lossy();
                let req_path = format!("/{}", relative.trim_start_matches('/'));

                let meta = read_or_build_metadata(&req_path, &file, &extra, &cfg, "local");

                // Convert for each enabled convert type
                if cfg.enable_jxl && ext != "jxl" {
                    let dest = crate::helper::get_sharded_path(&cfg.images_cache_path(), "local", &meta.id, "jxl");
                    if !dest.exists() {
                        let _ = convert_to_target(&file, NegotiatedFormat::Jxl, &dest, &extra, &cfg);
                    }
                }
                if cfg.enable_avif && ext != "avif" {
                    let dest = crate::helper::get_sharded_path(&cfg.images_cache_path(), "local", &meta.id, "avif");
                    if !dest.exists() {
                        let _ = convert_to_target(&file, NegotiatedFormat::Avif, &dest, &extra, &cfg);
                    }
                }
                if cfg.enable_webp && ext != "webp" {
                    let dest = crate::helper::get_sharded_path(&cfg.images_cache_path(), "local", &meta.id, "webp");
                    if !dest.exists() {
                        let _ = convert_to_target(&file, NegotiatedFormat::Webp, &dest, &extra, &cfg);
                    }
                }

                let finished = cnt.fetch_add(1, Ordering::Relaxed) + 1;
                if finished % 50 == 0 || finished == total {
                    info!("Prefetch progress: {} / {} images processed", finished, total);
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    info!("Prefetching completed successfully ({} images processed).", total);
}
