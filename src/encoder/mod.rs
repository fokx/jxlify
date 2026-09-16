pub mod animated;
pub mod avif;
pub mod jxl;
pub mod prefetch;
pub mod process;
pub mod webp;

use crate::config::{ExtraParams, JxlifyConfig};
use crate::negotiation::NegotiatedFormat;
use image::ImageReader;
use std::path::Path;
use tracing::debug;

/// Orchestrate conversion of raw image file to the negotiated target format
pub fn convert_to_target(
    raw_path: &Path,
    target_format: NegotiatedFormat,
    output_path: &Path,
    extra: &ExtraParams,
    config: &JxlifyConfig,
) -> Result<(), String> {
    let ext = crate::helper::get_extension(&raw_path.to_string_lossy());

    // 1. Check for animated GIF
    if ext == "gif" && animated::is_animated_gif(raw_path) {
        debug!("Processing animated GIF: {}", raw_path.display());
        match target_format {
            NegotiatedFormat::Webp => {
                return animated::convert_animated_gif_to_webp(raw_path, output_path, config.quality);
            }
            NegotiatedFormat::Avif => {
                return animated::convert_animated_gif_to_avif(raw_path, output_path, config.quality);
            }
            NegotiatedFormat::Jxl => {
                return animated::convert_animated_gif_to_jxl(raw_path, output_path, config.quality);
            }
            NegotiatedFormat::Raw => {
                return std::fs::copy(raw_path, output_path)
                    .map(|_| ())
                    .map_err(|e| format!("Failed to copy raw image: {}", e));
            }
        }
    }

    // 2. Load static image
    let dynamic_img = load_dynamic_image(raw_path)?;

    // 3. Preprocess (resize / smart crop / orientation)
    let processed_img = process::preprocess_image(dynamic_img, extra, config);

    // 4. Encode to target format
    match target_format {
        NegotiatedFormat::Jxl => {
            jxl::encode_jxl(&processed_img, output_path, config.quality)
        }
        NegotiatedFormat::Avif => {
            avif::encode_avif(&processed_img, output_path, config.quality)
        }
        NegotiatedFormat::Webp => {
            webp::encode_webp(&processed_img, output_path, config.quality)
        }
        NegotiatedFormat::Raw => {
            // Save resized raw image
            if !extra.is_empty() {
                let img_format = match ext.as_str() {
                    "png" => image::ImageFormat::Png,
                    "gif" => image::ImageFormat::Gif,
                    "bmp" => image::ImageFormat::Bmp,
                    _ => image::ImageFormat::Jpeg,
                };
                let mut out_bytes = Vec::new();
                processed_img
                    .write_to(&mut std::io::Cursor::new(&mut out_bytes), img_format)
                    .map_err(|e| format!("Failed to encode resized raw image: {}", e))?;
                crate::helper::atomic_write(output_path, &out_bytes)
                    .map_err(|e| format!("Failed to write resized raw image: {}", e))
            } else {
                // No resizing needed, just copy
                std::fs::copy(raw_path, output_path)
                    .map(|_| ())
                    .map_err(|e| format!("Failed to copy raw image: {}", e))
            }
        }
    }
}

pub fn load_dynamic_image(raw_path: &Path) -> Result<image::DynamicImage, String> {
    let bytes = std::fs::read(raw_path)
        .map_err(|e| format!("Failed to read image {}: {}", raw_path.display(), e))?;
    let ext = crate::helper::get_extension(&raw_path.to_string_lossy());
    load_dynamic_image_from_bytes(&bytes, Some(&ext))
}

pub fn load_dynamic_image_from_bytes(
    bytes: &[u8],
    ext_hint: Option<&str>,
) -> Result<image::DynamicImage, String> {
    // 1. Detect actual image format by magic bytes using infer
    let guessed_ext = infer::get(bytes).map(|t| t.extension());
    let effective_ext = guessed_ext
        .or(ext_hint)
        .unwrap_or_default()
        .to_lowercase();

    // 2. Try format-specific decoder based on effective extension
    match effective_ext.as_str() {
        "avif" => {
            if let Ok(img) = decode_avif_bytes(bytes) {
                return Ok(img);
            }
        }
        "jxl" => {
            if let Ok(img) = jxl::decode_jxl(bytes) {
                return Ok(img);
            }
        }
        _ => {}
    }

    // 3. Try standard image reader with guessed format (handles JPEG, PNG, GIF, WebP, BMP, etc.)
    let reader_res = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format();

    if let Ok(reader) = reader_res {
        if let Ok(img) = reader.decode() {
            return Ok(img);
        }
    }

    // 4. If standard reader failed, try remaining specialized decoders as fallback
    if effective_ext != "avif" {
        if let Ok(img) = decode_avif_bytes(bytes) {
            return Ok(img);
        }
    }
    if effective_ext != "jxl" {
        if let Ok(img) = jxl::decode_jxl(bytes) {
            return Ok(img);
        }
    }

    Err("Failed to decode image: unsupported format or corrupted data".to_string())
}

pub fn decode_avif_bytes(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    use zenpixels_convert::PixelBufferConvertTypedExt;
    let decoded = zenavif::decode(bytes)
        .map_err(|e| format!("zenavif failed to decode AVIF: {:?}", e))?;
    let rgba_buf = decoded.to_rgba8();
    let width = rgba_buf.width();
    let height = rgba_buf.height();
    let raw_bytes = rgba_buf.copy_to_contiguous_bytes();
    let img_buffer = image::RgbaImage::from_raw(width, height, raw_bytes)
        .ok_or_else(|| "Failed to construct RgbaImage from decoded AVIF buffer".to_string())?;
    Ok(image::DynamicImage::ImageRgba8(img_buffer))
}
