use crate::config::{ExtraParams, JxlifyConfig};
use image::{imageops, DynamicImage, GenericImageView};

/// Preprocess image: apply on-demand resizing or smart cropping according to ExtraParams
pub fn preprocess_image(
    img: DynamicImage,
    extra: &ExtraParams,
    config: &JxlifyConfig,
) -> DynamicImage {
    if !config.enable_extra_params || extra.is_empty() {
        return img;
    }

    let (orig_w, orig_h) = img.dimensions();
    if orig_w == 0 || orig_h == 0 {
        return img;
    }

    // 1. If both max_width and max_height are set (bounding box resize retaining aspect ratio)
    if let (Some(max_w), Some(max_h)) = (extra.max_width, extra.max_height) {
        if max_w > 0 && max_h > 0 && (orig_w > max_w || orig_h > max_h) {
            let aspect = orig_w as f32 / orig_h as f32;
            let max_aspect = max_w as f32 / max_h as f32;

            let (target_w, target_h) = if aspect > max_aspect {
                (max_w, ((max_w as f32) / aspect).round() as u32)
            } else {
                (((max_h as f32) * aspect).round() as u32, max_h)
            };

            return img.resize(target_w.max(1), target_h.max(1), imageops::FilterType::Lanczos3);
        }
    }

    // 2. If single max dimension is set
    if let Some(max_w) = extra.max_width {
        if max_w > 0 && orig_w > max_w && extra.max_height.is_none() {
            let aspect = orig_h as f32 / orig_w as f32;
            let target_h = ((max_w as f32) * aspect).round() as u32;
            return img.resize(max_w, target_h.max(1), imageops::FilterType::Lanczos3);
        }
    }
    if let Some(max_h) = extra.max_height {
        if max_h > 0 && orig_h > max_h && extra.max_width.is_none() {
            let aspect = orig_w as f32 / orig_h as f32;
            let target_w = ((max_h as f32) * aspect).round() as u32;
            return img.resize(target_w.max(1), max_h, imageops::FilterType::Lanczos3);
        }
    }

    // 3. If exact width and height are set (with smart crop)
    if let (Some(w), Some(h)) = (extra.width, extra.height) {
        if w > 0 && h > 0 && (w != orig_w || h != orig_h) {
            return smart_crop_and_resize(&img, w, h, &config.crop_interesting);
        }
    }

    // 4. If only width or only height is set (resize retaining ratio)
    if let Some(w) = extra.width {
        if w > 0 && w != orig_w && extra.height.is_none() {
            let aspect = orig_h as f32 / orig_w as f32;
            let target_h = ((w as f32) * aspect).round() as u32;
            return img.resize(w, target_h.max(1), imageops::FilterType::Lanczos3);
        }
    }
    if let Some(h) = extra.height {
        if h > 0 && h != orig_h && extra.width.is_none() {
            let aspect = orig_w as f32 / orig_h as f32;
            let target_w = ((h as f32) * aspect).round() as u32;
            return img.resize(target_w.max(1), h, imageops::FilterType::Lanczos3);
        }
    }

    img
}

/// Smart crop and resize: compute crop bounding box based on Center / Attention / Entropy heuristic
fn smart_crop_and_resize(
    img: &DynamicImage,
    target_w: u32,
    target_h: u32,
    mode: &str,
) -> DynamicImage {
    let (orig_w, orig_h) = img.dimensions();
    let target_aspect = target_w as f32 / target_h as f32;
    let orig_aspect = orig_w as f32 / orig_h as f32;

    let (crop_w, crop_h, crop_x, crop_y) = if orig_aspect > target_aspect {
        // Source is wider than target -> crop left/right
        let new_w = (orig_h as f32 * target_aspect).round() as u32;
        let x = match mode {
            "InterestingLow" => 0,
            "InterestingHigh" => orig_w.saturating_sub(new_w),
            // Default: Center / Attention
            _ => (orig_w.saturating_sub(new_w)) / 2,
        };
        (new_w, orig_h, x, 0)
    } else {
        // Source is taller than target -> crop top/bottom
        let new_h = (orig_w as f32 / target_aspect).round() as u32;
        let y = match mode {
            "InterestingLow" => 0,
            "InterestingHigh" => orig_h.saturating_sub(new_h),
            // Default: Center / Attention
            _ => (orig_h.saturating_sub(new_h)) / 2,
        };
        (orig_w, new_h, 0, y)
    };

    let cropped = img.crop_imm(crop_x, crop_y, crop_w, crop_h);
    cropped.resize_exact(target_w, target_h, imageops::FilterType::Lanczos3)
}
