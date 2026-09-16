use crate::cache::metadata::read_or_build_metadata;
use crate::cache::CacheManager;
use crate::config::{ExtraParams, JxlifyConfig};
use crate::encoder::convert_to_target;
use crate::handler::remote::fetch_remote_image;
use crate::helper::mime::detect_mime_from_path;
use crate::helper::{get_compression_rate, sanitize_request_path};
use crate::negotiation::{negotiate_format, NegotiatedFormat};
use axum::extract::{Query, State};
use axum::http::header::{HeaderMap, HeaderValue, ACCEPT, CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH, SERVER, USER_AGENT, VARY};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, warn};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<JxlifyConfig>,
    pub cache: CacheManager,
    pub http_client: reqwest::Client,
}

pub async fn image_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let req_path = uri.path();

    // 1. Sanitize path & prevent traversal
    let sanitized_rel_path = match sanitize_request_path(req_path) {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => return (StatusCode::NOT_FOUND, "Not Found").into_response(),
    };

    let filename = sanitized_rel_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    let ext = crate::helper::get_extension(filename);

    // 2. Check allowed extension
    if !state.config.is_allowed_extension(&ext) {
        return (
            StatusCode::BAD_REQUEST,
            format!("File extension not allowed: {}", ext),
        )
            .into_response();
    }

    // 3. Parse ExtraParams from Query
    let extra = ExtraParams {
        width: query
            .get("width")
            .or_else(|| query.get("w"))
            .or_else(|| query.get("s"))
            .and_then(|v| v.parse().ok()),
        height: query
            .get("height")
            .or_else(|| query.get("h"))
            .and_then(|v| v.parse().ok()),
        max_width: query
            .get("max_width")
            .or_else(|| query.get("mw"))
            .and_then(|v| v.parse().ok()),
        max_height: query
            .get("max_height")
            .or_else(|| query.get("mh"))
            .and_then(|v| v.parse().ok()),
    };

    // 4. Resolve raw image path (Local or Remote)
    let is_remote_root = state.config.img_path.starts_with("http://")
        || state.config.img_path.starts_with("https://");

    let mut requested_ext_override: Option<&str> = None;

    let (raw_image_path, subdir, orig_ext) = if is_remote_root {
        let remote_url = format!(
            "{}/{}",
            state.config.img_path.trim_end_matches('/'),
            sanitized_rel_path.to_string_lossy().trim_start_matches('/')
        );
        match fetch_remote_image(&state.http_client, &remote_url, "remote", &state.config).await {
            Ok(path) => {
                let actual_ext = crate::helper::get_extension(
                    path.file_name().and_then(|s| s.to_str()).unwrap_or_default()
                );
                (path, "remote".to_string(), actual_ext)
            }
            Err(e) => {
                warn!("Remote fetch failed: {}", e);
                return (StatusCode::NOT_FOUND, "Image not found").into_response();
            }
        }
    } else {
        let local_path = Path::new(&state.config.img_path).join(&sanitized_rel_path);
        if crate::helper::image_exists(&local_path) {
            (local_path, "local".to_string(), ext.clone())
        } else {
            // Fuzzy stem resolution: look for sibling file with same stem and allowed extension
            let stem = Path::new(filename).file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let parent = local_path.parent().unwrap_or(Path::new(""));
            if let Some(candidate) = find_stem_candidate(parent, stem, &state.config) {
                let candidate_ext = crate::helper::get_extension(
                    candidate.file_name().and_then(|s| s.to_str()).unwrap_or_default()
                );
                requested_ext_override = Some(&ext);
                (candidate, "local".to_string(), candidate_ext)
            } else {
                return (StatusCode::NOT_FOUND, "Image not found").into_response();
            }
        }
    };

    // 5. Read / build metadata using canonical source path for consistent caching across aliases
    let canonical_path = if is_remote_root {
        req_path.to_string()
    } else {
        let rel = raw_image_path
            .strip_prefix(Path::new(&state.config.img_path))
            .unwrap_or(&raw_image_path)
            .to_string_lossy();
        format!("/{}", rel.trim_start_matches('/'))
    };

    let metadata = read_or_build_metadata(
        &canonical_path,
        &raw_image_path,
        &extra,
        &state.config,
        &subdir,
    );

    // 6. Handle ?meta=full endpoint
    if query.get("meta").map(|s| s.as_str()) == Some("full") {
        return Json(serde_json::to_value(&metadata.meta).unwrap_or_default()).into_response();
    }

    // 7. Check ETag & If-None-Match
    let weak_etag = format!("W/\"{}\"", metadata.checksum);
    if let Some(if_none_match) = headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok()) {
        if if_none_match.trim() == weak_etag {
            let mut res = StatusCode::NOT_MODIFIED.into_response();
            res.headers_mut().insert(ETAG, HeaderValue::from_str(&weak_etag).unwrap());
            res.headers_mut().insert(VARY, HeaderValue::from_static("Accept, User-Agent"));
            return res;
        }
    }

    // 8. Content Negotiation or Query Parameter Override
    let accept_header = headers.get(ACCEPT).and_then(|v| v.to_str().ok());
    let ua_header = headers.get(USER_AGENT).and_then(|v| v.to_str().ok());

    let target_format = if let Some(fmt_param) = query.get("format").or_else(|| query.get("f")) {
        match fmt_param.to_lowercase().as_str() {
            "jxl" if state.config.enable_jxl => NegotiatedFormat::Jxl,
            "avif" if state.config.enable_avif => NegotiatedFormat::Avif,
            "webp" if state.config.enable_webp => NegotiatedFormat::Webp,
            "raw" | "orig" | "original" => NegotiatedFormat::Raw,
            _ => negotiate_format(accept_header, ua_header, &state.config),
        }
    } else if let Some(req_ext) = requested_ext_override {
        match req_ext.to_lowercase().as_str() {
            "jxl" if state.config.enable_jxl => NegotiatedFormat::Jxl,
            "avif" if state.config.enable_avif => NegotiatedFormat::Avif,
            "webp" if state.config.enable_webp => NegotiatedFormat::Webp,
            "raw" => NegotiatedFormat::Raw,
            _ => negotiate_format(accept_header, ua_header, &state.config),
        }
    } else {
        negotiate_format(accept_header, ua_header, &state.config)
    };

    // 9. If raw image already matches the negotiated format and no resizing is requested, skip conversion!
    let is_already_target_format = match target_format {
        NegotiatedFormat::Jxl => orig_ext == "jxl",
        NegotiatedFormat::Avif => orig_ext == "avif",
        NegotiatedFormat::Webp => orig_ext == "webp",
        NegotiatedFormat::Raw => true,
    };

    if is_already_target_format && extra.is_empty() {
        debug!(
            "Raw image {} is already in target format ({}), serving directly without conversion",
            raw_image_path.display(),
            orig_ext
        );
        return serve_file_response(
            &raw_image_path,
            &raw_image_path,
            &weak_etag,
            &orig_ext,
            &orig_ext,
        ).await;
    }

    // 10. Check Image Cache or Convert On Demand
    let target_ext = target_format.extension();
    let cached_path = state.cache.get_cache_path(&metadata.id, target_ext, &subdir, &state.config);

    if !state.cache.is_cached(&cached_path) {
        // Lock this conversion key to prevent duplicate concurrent encoding
        let lock_key = format!("{}_{}", metadata.id, target_format);
        let lock = state.cache.get_lock(&lock_key);
        let _guard = lock.lock().await;

        if !state.cache.is_cached(&cached_path) {
            let start = std::time::Instant::now();
            let result = convert_to_target(
                &raw_image_path,
                target_format,
                &cached_path,
                &extra,
                &state.config,
            );
            if let Err(err) = result {
                warn!("Conversion to {} failed for {}: {}", target_format, req_path, err);
                // Fallback to serving raw file
                return serve_file_response(
                    &raw_image_path,
                    &raw_image_path,
                    &weak_etag,
                    &orig_ext,
                    &orig_ext,
                ).await;
            }
            debug!(
                "Converted {} to {} in {:?} (cached to {})",
                req_path,
                target_format,
                start.elapsed(),
                cached_path.display()
            );
        }
        state.cache.remove_lock(&lock_key);
    }

    // 11. Serve Cached File
    serve_file_response(
        &cached_path,
        &raw_image_path,
        &weak_etag,
        &orig_ext,
        target_ext,
    )
    .await
}

fn find_stem_candidate(parent: &Path, stem: &str, config: &JxlifyConfig) -> Option<std::path::PathBuf> {
    if stem.is_empty() {
        return None;
    }
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(entry_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if entry_stem == stem {
                        let entry_ext = crate::helper::get_extension(
                            path.file_name().and_then(|s| s.to_str()).unwrap_or_default()
                        );
                        if config.is_allowed_extension(&entry_ext) {
                            return Some(path);
                        }
                    }
                }
            }
        }
    }
    None
}

async fn serve_file_response(
    file_path: &Path,
    raw_path: &Path,
    weak_etag: &str,
    orig_ext: &str,
    served_ext: &str,
) -> Response {
    let bytes = match tokio::fs::read(file_path).await {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read file {}: {}", file_path.display(), e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read image").into_response();
        }
    };

    let raw_len = tokio::fs::metadata(raw_path).await.map(|m| m.len()).unwrap_or(bytes.len() as u64);
    let opt_len = bytes.len() as u64;
    let comp_rate = get_compression_rate(raw_len, opt_len);

    let mime_type = detect_mime_from_path(file_path);

    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();

    headers.insert(CONTENT_TYPE, HeaderValue::from_str(&mime_type).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    headers.insert(VARY, HeaderValue::from_static("Accept, User-Agent"));
    headers.insert(ETAG, HeaderValue::from_str(weak_etag).unwrap_or(HeaderValue::from_static("W/\"0\"")));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable"));
    headers.insert(SERVER, HeaderValue::from_static("JXLify"));

    if let Ok(val) = HeaderValue::from_str(orig_ext) {
        headers.insert("X-Original-Format", val);
    }
    if let Ok(val) = HeaderValue::from_str(served_ext) {
        headers.insert("X-Served-Format", val);
    }
    if let Ok(val) = HeaderValue::from_str(&comp_rate) {
        headers.insert("X-Compression-Rate", val);
    }

    response
}
