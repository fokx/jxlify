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

/// Convert an animated GIF to an animated JPEG XL file preserving frame timing and transparency using official libjxl
pub fn convert_animated_gif_to_jxl(
    input_path: &Path,
    output_path: &Path,
    quality: u8,
) -> Result<(), String> {
    let decoded = decode_gif_frames(input_path)?;

    use jpegxl_sys::common::types::{JxlDataType, JxlEndianness, JxlPixelFormat};
    use jpegxl_sys::encoder::encode::{
        JxlColorEncodingSetToSRGB, JxlEncoderAddImageFrame, JxlEncoderCloseInput, JxlEncoderCreate,
        JxlEncoderDestroy, JxlEncoderFrameSettingId, JxlEncoderFrameSettingsCreate,
        JxlEncoderFrameSettingsSetOption, JxlEncoderInitBasicInfo, JxlEncoderInitExtraChannelInfo,
        JxlEncoderInitFrameHeader, JxlEncoderProcessOutput, JxlEncoderSetBasicInfo,
        JxlEncoderSetColorEncoding, JxlEncoderSetExtraChannelInfo, JxlEncoderSetFrameDistance,
        JxlEncoderSetFrameHeader, JxlEncoderSetFrameLossless, JxlEncoderSetParallelRunner,
        JxlEncoderStatus,
    };
    use jpegxl_sys::metadata::codestream_header::JxlExtraChannelType;
    use jpegxl_sys::threads::thread_parallel_runner::{
        JxlThreadParallelRunner, JxlThreadParallelRunnerCreate,
        JxlThreadParallelRunnerDefaultNumWorkerThreads, JxlThreadParallelRunnerDestroy,
    };
    use std::ptr::null;

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

    unsafe {
        let enc = JxlEncoderCreate(null());
        if enc.is_null() {
            return Err("Failed to create JxlEncoder".to_string());
        }

        struct JxlGuard(*mut jpegxl_sys::encoder::encode::JxlEncoder, *mut std::ffi::c_void);
        impl Drop for JxlGuard {
            fn drop(&mut self) {
                unsafe {
                    if !self.0.is_null() {
                        JxlEncoderDestroy(self.0);
                    }
                    if !self.1.is_null() {
                        JxlThreadParallelRunnerDestroy(self.1);
                    }
                }
            }
        }

        let num_threads = JxlThreadParallelRunnerDefaultNumWorkerThreads();
        let runner = JxlThreadParallelRunnerCreate(null(), num_threads);
        let _guard = JxlGuard(enc, runner);

        if !runner.is_null() {
            let status = JxlEncoderSetParallelRunner(
                enc,
                JxlThreadParallelRunner,
                runner,
            );
            if status != JxlEncoderStatus::Success {
                return Err("Failed to set parallel runner".to_string());
            }
        }

        let mut basic_info = {
            let mut info = std::mem::MaybeUninit::uninit();
            JxlEncoderInitBasicInfo(info.as_mut_ptr());
            info.assume_init()
        };
        basic_info.xsize = decoded.width;
        basic_info.ysize = decoded.height;
        basic_info.bits_per_sample = 8;
        basic_info.num_color_channels = 3;
        basic_info.have_animation = true.into();
        basic_info.animation.tps_numerator = 1000;
        basic_info.animation.tps_denominator = 1;
        basic_info.animation.num_loops = 0;
        basic_info.num_extra_channels = 1;
        basic_info.alpha_bits = 8;

        if JxlEncoderSetBasicInfo(enc, &basic_info) != JxlEncoderStatus::Success {
            return Err("Failed to set basic info in libjxl".to_string());
        }

        let extra_info = {
            let mut info = std::mem::MaybeUninit::uninit();
            JxlEncoderInitExtraChannelInfo(JxlExtraChannelType::Alpha, info.as_mut_ptr());
            let mut info = info.assume_init();
            info.bits_per_sample = 8;
            info
        };
        if JxlEncoderSetExtraChannelInfo(enc, 0, &extra_info) != JxlEncoderStatus::Success {
            return Err("Failed to set extra channel info in libjxl".to_string());
        }

        let color_encoding = {
            let mut enc_struct = std::mem::MaybeUninit::uninit();
            JxlColorEncodingSetToSRGB(enc_struct.as_mut_ptr(), false.into());
            enc_struct.assume_init()
        };
        if JxlEncoderSetColorEncoding(enc, &color_encoding) != JxlEncoderStatus::Success {
            return Err("Failed to set color encoding in libjxl".to_string());
        }

        let num_frames = decoded.frames.len();
        let pixel_format = JxlPixelFormat {
            num_channels: 4,
            data_type: JxlDataType::Uint8,
            endianness: JxlEndianness::Native,
            align: 0,
        };

        for (i, (rgba_buf, delay_ms)) in decoded.frames.iter().enumerate() {
            let frame_settings = JxlEncoderFrameSettingsCreate(enc, null());
            if frame_settings.is_null() {
                return Err("Failed to create frame settings in libjxl".to_string());
            }

            if lossless {
                JxlEncoderSetFrameLossless(frame_settings, true.into());
            } else {
                JxlEncoderSetFrameDistance(frame_settings, distance);
            }

            JxlEncoderFrameSettingsSetOption(
                frame_settings,
                JxlEncoderFrameSettingId::Effort,
                7,
            );

            let mut frame_header = {
                let mut header = std::mem::MaybeUninit::uninit();
                JxlEncoderInitFrameHeader(header.as_mut_ptr());
                header.assume_init()
            };
            frame_header.duration = *delay_ms;
            frame_header.is_last = (i == num_frames - 1).into();

            if JxlEncoderSetFrameHeader(frame_settings, &frame_header) != JxlEncoderStatus::Success {
                return Err("Failed to set frame header in libjxl".to_string());
            }

            let raw_slice = rgba_buf.as_raw();
            let status = JxlEncoderAddImageFrame(
                frame_settings,
                &pixel_format,
                raw_slice.as_ptr().cast(),
                raw_slice.len(),
            );
            if status != JxlEncoderStatus::Success {
                return Err(format!("Failed to add image frame {} in libjxl: {:?}", i, status));
            }
        }

        JxlEncoderCloseInput(enc);

        let mut buffer = vec![0u8; 128 * 1024];
        let mut next_out = buffer.as_mut_ptr();
        let mut avail_out = buffer.len();

        loop {
            let status = JxlEncoderProcessOutput(enc, &mut next_out, &mut avail_out);
            if status == JxlEncoderStatus::Success {
                break;
            } else if status == JxlEncoderStatus::NeedMoreOutput {
                let offset = next_out.offset_from(buffer.as_ptr()) as usize;
                buffer.resize(buffer.len() * 2, 0);
                next_out = buffer.as_mut_ptr().add(offset);
                avail_out = buffer.len() - offset;
            } else {
                return Err(format!("JxlEncoderProcessOutput failed with status {:?}", status));
            }
        }

        let total_size = next_out.offset_from(buffer.as_ptr()) as usize;
        buffer.truncate(total_size);

        crate::helper::atomic_write(output_path, &buffer)
            .map_err(|e| format!("Failed to write animated JXL file: {}", e))
    }
}
