use image::{DynamicImage, GenericImageView};
use std::path::Path;

pub const WEBP_MAX_DIMENSION: u32 = 16383;

/// Encode DynamicImage to WebP file preserving alpha channel
pub fn encode_webp(img: &DynamicImage, output_path: &Path, quality: u8) -> Result<(), String> {
    let (width, height) = img.dimensions();

    if width == 0 || height == 0 {
        return Err("Image has zero width or height".to_string());
    }

    if width > WEBP_MAX_DIMENSION || height > WEBP_MAX_DIMENSION {
        return Err(format!(
            "Image dimensions ({}x{}) exceed WebP maximum of {}px",
            width, height, WEBP_MAX_DIMENSION
        ));
    }

    let rgba = img.to_rgba8();

    // Catch unwind or check safety
    let webp_data = std::panic::catch_unwind(|| {
        let encoder = webp::Encoder::from_rgba(rgba.as_raw(), width, height);
        if quality >= 100 {
            encoder.encode_lossless()
        } else {
            encoder.encode(quality.clamp(1, 100) as f32)
        }
    })
    .map_err(|_| "WebP encoding failed internally".to_string())?;

    crate::helper::atomic_write(output_path, &webp_data)
        .map_err(|e| format!("Failed to write WebP file: {}", e))
}
