use image::{DynamicImage, GenericImageView};
use jxl_encoder::{LosslessConfig, LossyConfig, PixelLayout};
use std::path::Path;

/// Encode DynamicImage to JPEG XL file preserving alpha channel using pure Rust jxl-encoder
pub fn encode_jxl(img: &DynamicImage, output_path: &Path, quality: u8) -> Result<(), String> {
    let (width, height) = img.dimensions();

    if width == 0 || height == 0 {
        return Err("Image has zero width or height".to_string());
    }

    let rgba = img.to_rgba8();
    let raw_pixels = rgba.as_raw();

    // Map 0..100 quality percentage to butteraugli distance (0.0..15.0)
    // 100% -> lossless, 90% -> 0.5, 80% -> 1.0 (visually lossless), 50% -> 3.0
    let encoded_data = if quality >= 100 {
        LosslessConfig::new()
            .encode(raw_pixels, width, height, PixelLayout::Rgba8)
            .map_err(|e| format!("Failed to encode lossless JXL with jxl-encoder: {:?}", e))?
    } else {
        let q = quality.max(1) as f32;
        let dist = if q >= 80.0 {
            (100.0 - q) * 0.05
        } else {
            1.0 + (80.0 - q) * 0.1
        };
        LossyConfig::new(dist)
            .encode(raw_pixels, width, height, PixelLayout::Rgba8)
            .map_err(|e| format!("Failed to encode lossy JXL with jxl-encoder: {:?}", e))?
    };

    crate::helper::atomic_write(output_path, &encoded_data)
        .map_err(|e| format!("Failed to write JXL file: {}", e))
}

/// Decode JPEG XL file bytes to DynamicImage using pure Rust jxl-rs decoder
pub fn decode_jxl(bytes: &[u8]) -> Result<DynamicImage, String> {
    let cursor = std::io::Cursor::new(bytes);
    let decoder = jxl_image_rs_integration::JxlDecoder::new(cursor)
        .map_err(|e| format!("Failed to create JxlDecoder: {:?}", e))?;
    DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Failed to decode JXL with jxl-rs: {:?}", e))
}
