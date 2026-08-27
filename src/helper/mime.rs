#![allow(dead_code)]
use std::path::Path;

/// Detect MIME type from raw bytes using `infer` and fallback to extension/default
pub fn detect_mime_from_bytes(buf: &[u8]) -> String {
    if let Some(kind) = infer::get(buf) {
        return kind.mime_type().to_string();
    }
    // Check if SVG (infer might not catch text/svg if no xml header)
    if is_svg(buf) {
        return "image/svg+xml".to_string();
    }
    "application/octet-stream".to_string()
}

/// Detect MIME type from file extension or content
pub fn detect_mime_from_path(path: &Path) -> String {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        match ext.to_lowercase().as_str() {
            "jxl" => return "image/jxl".to_string(),
            "avif" => return "image/avif".to_string(),
            "webp" => return "image/webp".to_string(),
            "jpg" | "jpeg" => return "image/jpeg".to_string(),
            "png" => return "image/png".to_string(),
            "gif" => return "image/gif".to_string(),
            "svg" => return "image/svg+xml".to_string(),
            "bmp" => return "image/bmp".to_string(),
            "heic" | "heif" => return "image/heic".to_string(),
            "nef" => return "image/x-nikon-nef".to_string(),
            _ => {}
        }
    }
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_string()
}

/// Simple heuristic to check if buffer contains SVG XML
pub fn is_svg(buf: &[u8]) -> bool {
    let limit = buf.len().min(1024);
    if let Ok(s) = std::str::from_utf8(&buf[..limit]) {
        let lower = s.to_lowercase();
        return lower.contains("<svg") || (lower.contains("<?xml") && lower.contains("<svg"));
    }
    false
}
