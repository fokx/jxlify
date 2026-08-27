use image::{DynamicImage, GenericImageView};
use jpegxl_rs::encode::{EncoderFrame, EncoderResult, JxlEncoder};
use std::path::Path;

/// Encode DynamicImage to JPEG XL file preserving alpha channel using libjxl bindings
pub fn encode_jxl(img: &DynamicImage, output_path: &Path, quality: u8) -> Result<(), String> {
    let (width, height) = img.dimensions();

    if width == 0 || height == 0 {
        return Err("Image has zero width or height".to_string());
    }

    let rgba = img.to_rgba8();

    // Map 0..100 quality percentage to butteraugli distance (0.0..15.0)
    // 100% -> 0.0 (lossless), 90% -> 0.5, 80% -> 1.0 (visually lossless), 50% -> 3.0
    let (lossless, distance) = if quality >= 100 {
        (true, 0.0)
    } else {
        let q = quality.max(1) as f32;
        let dist = if q >= 80.0 {
            (100.0 - q) * 0.05
        } else {
            1.0 + (80.0 - q) * 0.1
        };
        (false, dist)
    };

    let mut encoder = JxlEncoder::builder()
        .quality(distance)
        .lossless(lossless)
        .has_alpha(true)
        .build()
        .map_err(|e| format!("Failed to create JxlEncoder: {:?}", e))?;

    let frame = EncoderFrame::new(rgba.as_raw()).num_channels(4);

    let result: EncoderResult<u8> = encoder
        .encode_frame(&frame, width, height)
        .map_err(|e| format!("Failed to encode JXL with libjxl: {:?}", e))?;

    crate::helper::atomic_write(output_path, &result.data)
        .map_err(|e| format!("Failed to write JXL file: {}", e))
}
