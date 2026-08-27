use image::{DynamicImage, GenericImageView};
use zenravif::{Encoder, Img, RGBA8};
use std::path::Path;

pub const AVIF_MAX_DIMENSION: u32 = 65536;

/// Encode DynamicImage to AVIF file preserving alpha channel using pure Rust ravif
pub fn encode_avif(img: &DynamicImage, output_path: &Path, quality: u8) -> Result<(), String> {
    let (width, height) = img.dimensions();

    if width == 0 || height == 0 {
        return Err("Image has zero width or height".to_string());
    }

    if width > AVIF_MAX_DIMENSION || height > AVIF_MAX_DIMENSION {
        return Err(format!(
            "Image dimensions ({}x{}) exceed AVIF maximum of {}px",
            width, height, AVIF_MAX_DIMENSION
        ));
    }

    let rgba = img.to_rgba8();
    let raw_slice = rgba.as_raw();
    let pixels: &[RGBA8] = bytemuck::cast_slice(raw_slice);
    let img_ref = Img::new(pixels, width as usize, height as usize);

    // Map quality (1..100) and set speed to 7 (balanced high-speed for servers)
    let encoder = Encoder::new()
        .with_quality(quality.clamp(1, 100) as f32)
        .with_speed(7)
        .with_alpha_quality(quality.clamp(1, 100) as f32);

    let encoded = encoder
        .encode_rgba(img_ref)
        .map_err(|e| format!("zenravif AVIF encode error: {:?}", e))?;

    crate::helper::atomic_write(output_path, &encoded.avif_file)
        .map_err(|e| format!("Failed to write AVIF file: {}", e))
}
