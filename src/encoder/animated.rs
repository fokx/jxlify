use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, RgbaImage};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Check if a file is an animated GIF with multiple frames
pub fn is_animated_gif(path: &Path) -> bool {
    if let Ok(file) = File::open(path) {
        let reader = BufReader::new(file);
        if let Ok(decoder) = GifDecoder::new(reader) {
            let mut frames = decoder.into_frames();
            if frames.next().is_some() && frames.next().is_some() {
                return true;
            }
        }
    }
    false
}

struct DecodedFrames {
    width: u32,
    height: u32,
    frames: Vec<(RgbaImage, u32)>,
}

fn decode_gif_frames(input_path: &Path) -> Result<DecodedFrames, String> {
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
    if width == 0 || height == 0 {
        return Err("GIF has zero width or height".to_string());
    }

    let mut decoded = Vec::with_capacity(frames.len());
    for frame in frames {
        let (num, denom) = frame.delay().numer_denom_ms();
        let delay_ms = if denom == 0 { 100 } else { (num / denom).max(10) } as u32;
        let rgba_buf = frame.into_buffer();
        decoded.push((rgba_buf, delay_ms));
    }

    Ok(DecodedFrames {
        width,
        height,
        frames: decoded,
    })
}

/// Convert an animated GIF to an animated WebP file preserving frame timing and transparency
pub fn convert_animated_gif_to_webp(
    input_path: &Path,
    output_path: &Path,
    quality: u8,
) -> Result<(), String> {
    let decoded = decode_gif_frames(input_path)?;

    let encoder_options = webp_animation::EncoderOptions {
        encoding_config: Some(webp_animation::EncodingConfig::new_lossy(
            quality.clamp(1, 100) as f32,
        )),
        ..Default::default()
    };

    let mut encoder = webp_animation::Encoder::new_with_options(
        (decoded.width, decoded.height),
        encoder_options,
    )
    .map_err(|e| format!("Failed to initialize WebP animation encoder: {:?}", e))?;

    let mut current_timestamp_ms: i32 = 0;

    for (rgba_buf, delay_ms) in &decoded.frames {
        encoder
            .add_frame(rgba_buf.as_raw(), current_timestamp_ms)
            .map_err(|e| format!("Failed to add frame to WebP animation: {:?}", e))?;

        current_timestamp_ms += *delay_ms as i32;
    }

    let webp_data = encoder
        .finalize(current_timestamp_ms)
        .map_err(|e| format!("Failed to finalize WebP animation: {:?}", e))?;

    crate::helper::atomic_write(output_path, &webp_data)
        .map_err(|e| format!("Failed to write animated WebP file: {}", e))
}

/// Convert an animated GIF to an animated AVIF file preserving frame timing and transparency
pub fn convert_animated_gif_to_avif(
    input_path: &Path,
    output_path: &Path,
    quality: u8,
) -> Result<(), String> {
    let decoded = decode_gif_frames(input_path)?;

    let anim_frames: Vec<zenravif::AnimFrameRgba<'_>> = decoded
        .frames
        .iter()
        .map(|(rgba_buf, delay_ms)| {
            let pixels: &[zenravif::RGBA8] = bytemuck::cast_slice(rgba_buf.as_raw());
            let img_ref =
                zenravif::Img::new(pixels, decoded.width as usize, decoded.height as usize);
            zenravif::AnimFrameRgba {
                rgba: img_ref,
                duration_ms: *delay_ms,
            }
        })
        .collect();

    // Map quality (1..100) and set speed to 7 (balanced high-speed for servers)
    let encoder = zenravif::Encoder::new()
        .with_quality(quality.clamp(1, 100) as f32)
        .with_speed(7)
        .with_alpha_quality(quality.clamp(1, 100) as f32);

    let result = encoder
        .encode_animation_rgba(&anim_frames)
        .map_err(|e| format!("zenravif animated AVIF encode error: {:?}", e))?;

    crate::helper::atomic_write(output_path, &result.avif_file)
        .map_err(|e| format!("Failed to write animated AVIF file: {}", e))
}

/// Convert an animated GIF to an animated JPEG XL file preserving frame timing and transparency using pure Rust jxl-encoder
pub fn convert_animated_gif_to_jxl(
    input_path: &Path,
    output_path: &Path,
    quality: u8,
) -> Result<(), String> {
    let decoded = decode_gif_frames(input_path)?;

    let anim_params = jxl_encoder::AnimationParams {
        tps_numerator: 1000,
        tps_denominator: 1,
        num_loops: 0,
    };

    let anim_frames: Vec<jxl_encoder::AnimationFrame<'_>> = decoded
        .frames
        .iter()
        .map(|(rgba_buf, delay_ms)| jxl_encoder::AnimationFrame {
            pixels: rgba_buf.as_raw(),
            duration: *delay_ms,
        })
        .collect();

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

    let jxl_data = if lossless {
        jxl_encoder::LosslessConfig::new()
            .encode_animation(
                decoded.width,
                decoded.height,
                jxl_encoder::PixelLayout::Rgba8,
                &anim_params,
                &anim_frames,
            )
            .map_err(|e| format!("Failed to encode lossless animated JXL: {:?}", e))?
    } else {
        jxl_encoder::LossyConfig::new(distance)
            .encode_animation(
                decoded.width,
                decoded.height,
                jxl_encoder::PixelLayout::Rgba8,
                &anim_params,
                &anim_frames,
            )
            .map_err(|e| format!("Failed to encode lossy animated JXL: {:?}", e))?
    };

    crate::helper::atomic_write(output_path, &jxl_data)
        .map_err(|e| format!("Failed to write animated JXL file: {}", e))
}
