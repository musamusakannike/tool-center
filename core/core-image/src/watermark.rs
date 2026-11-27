use crate::codecs::simd_resize;
use core_jobs::ToolError;
use image::{DynamicImage, Rgba, RgbaImage};

/// Computes `(x, y)` top-left placement for an overlay of `(ow, oh)` inside `(bw, bh)`
/// according to one of the 9 standard anchor positions.
pub fn anchor_coordinates(bw: u32, bh: u32, ow: u32, oh: u32, position: &str) -> (u32, u32) {
    let margin_x = ((bw as f32 * 0.03).round() as u32).clamp(8, 64);
    let margin_y = ((bh as f32 * 0.03).round() as u32).clamp(8, 64);

    let max_x = bw.saturating_sub(ow);
    let max_y = bh.saturating_sub(oh);

    let left_x = margin_x.min(max_x);
    let center_x = bw.saturating_sub(ow) / 2;
    let right_x = bw.saturating_sub(ow.saturating_add(margin_x));

    let top_y = margin_y.min(max_y);
    let center_y = bh.saturating_sub(oh) / 2;
    let bottom_y = bh.saturating_sub(oh.saturating_add(margin_y));

    match position {
        "top_left" => (left_x, top_y),
        "top_center" => (center_x, top_y),
        "top_right" => (right_x, top_y),
        "center_left" => (left_x, center_y),
        "center" => (center_x, center_y),
        "center_right" => (right_x, center_y),
        "bottom_left" => (left_x, bottom_y),
        "bottom_center" => (center_x, bottom_y),
        _ => (right_x, bottom_y),
    }
}

/// Composites an image watermark onto `base` at `position` with `scale_pct` (width % of base)
/// and `opacity_pct` (`1..=100`).
pub fn apply_image_watermark(
    base: &mut RgbaImage,
    overlay: &DynamicImage,
    position: &str,
    scale_pct: u32,
    opacity_pct: u32,
) -> Result<(), ToolError> {
    let (bw, bh) = (base.width(), base.height());
    if bw == 0 || bh == 0 {
        return Ok(());
    }
    let pct = scale_pct.clamp(5, 90) as f64 / 100.0;
    let target_w = ((bw as f64 * pct).round() as u32).max(16).min(bw);
    let aspect = overlay.height().max(1) as f64 / overlay.width().max(1) as f64;
    let target_h = ((target_w as f64 * aspect).round() as u32).max(1).min(bh);

    let scaled = simd_resize(overlay, target_w, target_h, "lanczos3")?.to_rgba8();
    let (ox, oy) = anchor_coordinates(bw, bh, scaled.width(), scaled.height(), position);
    let alpha_mul = opacity_pct.clamp(1, 100) as f32 / 100.0;

    alpha_blend_rect(base, &scaled, ox, oy, alpha_mul);
    Ok(())
}

/// Renders a crisp, legible text watermark badge onto `base` using an embedded 5×7 vector-grid
/// bitmap font scaled smoothly with a high-contrast dark translucent pill background.
pub fn apply_text_watermark(
    base: &mut RgbaImage,
    text: &str,
    position: &str,
    size_pct: u32,
    opacity_pct: u32,
) -> Result<(), ToolError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ToolError::invalid("Watermark text cannot be empty."));
    }

    let (bw, bh) = (base.width(), base.height());
    if bw == 0 || bh == 0 {
        return Ok(());
    }

    // Scale glyph pixel size proportionally to image size and requested size_pct (10..100)
    let base_scale = ((bw.min(bh) as f32 / 320.0) * (size_pct.clamp(10, 100) as f32 / 35.0))
        .round()
        .clamp(2.0, 18.0) as u32;

    let chars: Vec<char> = trimmed.chars().take(80).collect();
    let glyph_w = 5 * base_scale;
    let glyph_h = 7 * base_scale;
    let spacing = base_scale.max(1);
    let pad_x = 3 * base_scale;
    let pad_y = 2 * base_scale;

    let text_w = chars.len() as u32 * (glyph_w + spacing) - spacing;
    let box_w = (text_w + pad_x * 2).min(bw);
    let box_h = (glyph_h + pad_y * 2).min(bh);

    let mut badge = RgbaImage::from_pixel(box_w, box_h, Rgba([15, 23, 42, 165]));

    // Subtle 1px light border around badge for crispness on dark backgrounds
    for x in 0..box_w {
        badge.put_pixel(x, 0, Rgba([255, 255, 255, 90]));
        badge.put_pixel(x, box_h.saturating_sub(1), Rgba([255, 255, 255, 90]));
    }
    for y in 0..box_h {
        badge.put_pixel(0, y, Rgba([255, 255, 255, 90]));
        badge.put_pixel(box_w.saturating_sub(1), y, Rgba([255, 255, 255, 90]));
    }

    for (idx, ch) in chars.iter().enumerate() {
        let bitmap = glyph_5x7(*ch);
        let start_x = pad_x + idx as u32 * (glyph_w + spacing);
        let start_y = pad_y;

        for (row, row_bits) in bitmap.iter().enumerate() {
            for col in 0..5u32 {
                if (row_bits >> (4 - col)) & 1 == 1 {
                    for dy in 0..base_scale {
                        for dx in 0..base_scale {
                            let px = start_x + col * base_scale + dx;
                            let py = start_y + row as u32 * base_scale + dy;
                            if px < box_w && py < box_h {
                                badge.put_pixel(px, py, Rgba([255, 255, 255, 255]));
                            }
                        }
                    }
                }
            }
        }
    }

    let (ox, oy) = anchor_coordinates(bw, bh, box_w, box_h, position);
    let alpha_mul = opacity_pct.clamp(5, 100) as f32 / 100.0;
    alpha_blend_rect(base, &badge, ox, oy, alpha_mul);
    Ok(())
}

fn alpha_blend_rect(base: &mut RgbaImage, overlay: &RgbaImage, ox: u32, oy: u32, opacity_mul: f32) {
    let (bw, bh) = (base.width(), base.height());
    for y in 0..overlay.height() {
        let by = oy + y;
        if by >= bh {
            break;
        }
        for x in 0..overlay.width() {
            let bx = ox + x;
            if bx >= bw {
                break;
            }
            let src = overlay.get_pixel(x, y);
            let src_a = (src[3] as f32 / 255.0) * opacity_mul;
            if src_a <= 0.001 {
                continue;
            }
            let dst = base.get_pixel(bx, by);
            let dst_a = dst[3] as f32 / 255.0;
            let out_a = src_a + dst_a * (1.0 - src_a);
            if out_a <= 0.001 {
                continue;
            }
            let blend_channel = |sc: u8, dc: u8| -> u8 {
                let s = sc as f32 / 255.0;
                let d = dc as f32 / 255.0;
                let out = (s * src_a + d * dst_a * (1.0 - src_a)) / out_a;
                (out * 255.0).round().clamp(0.0, 255.0) as u8
            };
            base.put_pixel(
                bx,
                by,
                Rgba([
                    blend_channel(src[0], dst[0]),
                    blend_channel(src[1], dst[1]),
                    blend_channel(src[2], dst[2]),
                    (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
                ]),
            );
        }
    }
}

/// Compact 5×7 uppercase/ASCII/symbol bitmap font for dependency-free cross-platform watermarking.
fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' | '©' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '_' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100,
        ],
        ':' => [
            0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000,
        ],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        '@' => [
            0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110,
        ],
        '!' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
        ' ' => [0; 7],
        _ => [
            0b11111, 0b10001, 0b00010, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
    }
}
