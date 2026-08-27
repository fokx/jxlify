use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Check if a file is an animated GIF with multiple frames
pub fn is_animated_gif(path: &Path) -> bool {
    if let Ok(file) = File::open(path) {
        let reader = BufReader::new(file);
        if let Ok(decoder) = GifDecoder::new(reader) {
            if let Ok(frames) = decoder.into_frames().collect_frames() {
                return frames.len() > 1;
            }
        }
    }
    false
}

/// Convert an animated GIF to an animated WebP file preserving frame timing and transparency
pub fn convert_animated_gif_to_webp(
    input_path: &Path,
    output_path: &Path,
    _quality: u8,
) -> Result<(), String> {
    let file = File::open(input_path)
        .map_err(|e| format!("Failed to open animated GIF: {}", e))?;
    let reader = BufReader::new(file);

    let decoder = GifDecoder::new(reader)
        .map_err(|e| format!("Failed to create GifDecoder: {}", e))?;

    let frames = decoder
        .into_frames()
        .collect_frames()
        .map_err(|e| format!("Failed to decode GIF frames: {}", e))?;

    if frames.is_empty() {
        return Err("GIF has no frames".to_string());
    }

    let first_frame = &frames[0];
    let (width, height) = first_frame.buffer().dimensions();

    let mut encoder = webp_animation::Encoder::new((width, height))
        .map_err(|e| format!("Failed to initialize WebP animation encoder: {:?}", e))?;

    let mut current_timestamp_ms: i32 = 0;

    for frame in frames {
        let (num, denom) = frame.delay().numer_denom_ms();
        let delay_ms = if denom == 0 { 100 } else { (num / denom).max(10) } as i32;

        let rgba_buf = frame.into_buffer();
        encoder
            .add_frame(rgba_buf.as_raw(), current_timestamp_ms)
            .map_err(|e| format!("Failed to add frame to WebP animation: {:?}", e))?;

        current_timestamp_ms += delay_ms;
    }

    let webp_data = encoder
        .finalize(current_timestamp_ms)
        .map_err(|e| format!("Failed to finalize WebP animation: {:?}", e))?;

    crate::helper::atomic_write(output_path, &webp_data)
        .map_err(|e| format!("Failed to write animated WebP file: {}", e))
}
