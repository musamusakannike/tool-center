use crate::preview::load_image_any;
use core_jobs::ToolError;
use core_media::locate_ffmpeg_binary;
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType as FirFilter, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::codecs::avif::AvifEncoder;
use image::{ColorType, DynamicImage, ImageEncoder, ImageFormat, RgbaImage};
use img_parts::jpeg::Jpeg;
use img_parts::png::Png;
use img_parts::webp::WebP;
use img_parts::{Bytes, ImageEXIF};
use mozjpeg_rs::{Encoder as MozJpegEncoder, Preset as MozPreset};
use std::fs::{self, File};
use std::io::{BufWriter, Cursor};
use std::path::Path;
use std::process::Stdio;

pub const IMAGE_INPUT_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "tiff", "tif", "avif", "heic", "heif",
];

/// High-speed SIMD + Rayon image resizer using `fast_image_resize` with premultiplied alpha.
pub fn simd_resize(
    src: &DynamicImage,
    dst_width: u32,
    dst_height: u32,
    filter: &str,
) -> Result<DynamicImage, ToolError> {
    let dst_w = dst_width.max(1);
    let dst_h = dst_height.max(1);
    if src.width() == dst_w && src.height() == dst_h {
        return Ok(src.clone());
    }

    let mut src_raw = src.to_rgba8().into_raw();
    let src_view =
        FirImage::from_slice_u8(src.width(), src.height(), &mut src_raw, PixelType::U8x4).map_err(
            |e| ToolError::failed("Failed to prepare source image buffer", Some(e.to_string())),
        )?;

    let mut dst_img = FirImage::new(dst_w, dst_h, PixelType::U8x4);

    let alg = match filter {
        "nearest" => ResizeAlg::Nearest,
        "bilinear" => ResizeAlg::Convolution(FirFilter::Bilinear),
        "catmull_rom" => ResizeAlg::Convolution(FirFilter::CatmullRom),
        _ => ResizeAlg::Convolution(FirFilter::Lanczos3),
    };

    let options = ResizeOptions::new().resize_alg(alg);
    let mut resizer = Resizer::new();
    resizer
        .resize(&src_view, &mut dst_img, Some(&options))
        .map_err(|e| ToolError::failed("SIMD image resize failed", Some(e.to_string())))?;

    let out_buf = RgbaImage::from_raw(dst_w, dst_h, dst_img.into_vec())
        .ok_or_else(|| ToolError::failed("Invalid resized image buffer dimensions", None))?;
    Ok(DynamicImage::ImageRgba8(out_buf))
}

/// Computes target `(width, height)` preserving aspect ratio when requested.
pub fn compute_target_dimensions(
    orig_w: u32,
    orig_h: u32,
    req_w: u32,
    req_h: u32,
    keep_aspect: bool,
) -> (u32, u32) {
    let w = req_w.max(1);
    let h = req_h.max(1);
    if !keep_aspect || orig_w == 0 || orig_h == 0 {
        return (w, h);
    }
    let scale_w = w as f64 / orig_w as f64;
    let scale_h = h as f64 / orig_h as f64;
    let scale = scale_w.min(scale_h);
    let out_w = ((orig_w as f64 * scale).round() as u32).max(1);
    let out_h = ((orig_h as f64 * scale).round() as u32).max(1);
    (out_w, out_h)
}

/// Normalizes output image extension (mapping `heic`/`heif`/`jpeg`/`tif` to supported output extensions).
pub fn normalize_output_extension(ext: &str) -> &'static str {
    match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "jpg",
        "png" => "png",
        "webp" => "webp",
        "avif" => "avif",
        "tiff" | "tif" => "tiff",
        "bmp" => "bmp",
        "gif" => "gif",
        // HEIC encoding is omitted to remain strictly GPL-free; default to PNG when source was HEIC.
        _ => "png",
    }
}

/// Encodes `DynamicImage` to `out_path` using best-in-class encoders (`mozjpeg-rs`, `oxipng`, `ravif`).
pub fn encode_image_to_path(
    img: &DynamicImage,
    out_path: &Path,
    target_ext: &str,
    quality: u8,
    optimize_lossless: bool,
) -> Result<u64, ToolError> {
    let ext = normalize_output_extension(target_ext);
    let q = quality.clamp(1, 100);

    match ext {
        "jpg" => {
            let rgb = img.to_rgb8();
            let preset = if optimize_lossless {
                MozPreset::ProgressiveSmallest
            } else {
                MozPreset::ProgressiveBalanced
            };
            let jpeg_bytes = MozJpegEncoder::new(preset)
                .quality(q)
                .encode_rgb(rgb.as_raw(), rgb.width(), rgb.height())
                .map_err(|e| ToolError::failed("MozJPEG encoding failed", Some(e.to_string())))?;
            fs::write(out_path, &jpeg_bytes).map_err(|e| {
                ToolError::failed("Failed to write JPEG output", Some(e.to_string()))
            })?;
            Ok(jpeg_bytes.len() as u64)
        }
        "png" => {
            let mut raw_png = Cursor::new(Vec::new());
            img.write_to(&mut raw_png, ImageFormat::Png)
                .map_err(|e| ToolError::failed("PNG encoding failed", Some(e.to_string())))?;
            let png_bytes = raw_png.into_inner();

            let final_bytes = if optimize_lossless {
                let level = if q <= 60 { 4 } else { 2 };
                let mut opts = oxipng::Options::from_preset(level);
                opts.strip = oxipng::StripChunks::Safe;
                oxipng::optimize_from_memory(&png_bytes, &opts).unwrap_or(png_bytes)
            } else {
                png_bytes
            };

            fs::write(out_path, &final_bytes).map_err(|e| {
                ToolError::failed("Failed to write PNG output", Some(e.to_string()))
            })?;
            Ok(final_bytes.len() as u64)
        }
        "webp" => {
            if optimize_lossless && q < 100 {
                if let Ok(size) = try_encode_lossy_webp_via_ffmpeg(img, out_path, q) {
                    return Ok(size);
                }
            }
            img.save_with_format(out_path, ImageFormat::WebP)
                .map_err(|e| ToolError::failed("WebP encoding failed", Some(e.to_string())))?;
            fs::metadata(out_path)
                .map(|m| m.len())
                .map_err(|e| ToolError::failed("Failed to stat WebP output", Some(e.to_string())))
        }
        "avif" => {
            let rgba = img.to_rgba8();
            let file = File::create(out_path)
                .map_err(|e| ToolError::failed("Cannot create AVIF file", Some(e.to_string())))?;
            let writer = BufWriter::new(file);
            // Speed 7 keeps desktop interactive latency low while achieving strong AV1 compression.
            let encoder = AvifEncoder::new_with_speed_quality(writer, 7, q);
            encoder
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    ColorType::Rgba8.into(),
                )
                .map_err(|e| ToolError::failed("AVIF encoding failed", Some(e.to_string())))?;
            fs::metadata(out_path)
                .map(|m| m.len())
                .map_err(|e| ToolError::failed("Failed to stat AVIF output", Some(e.to_string())))
        }
        "tiff" => {
            img.save_with_format(out_path, ImageFormat::Tiff)
                .map_err(|e| ToolError::failed("TIFF encoding failed", Some(e.to_string())))?;
            fs::metadata(out_path)
                .map(|m| m.len())
                .map_err(|e| ToolError::failed("Failed to stat TIFF output", Some(e.to_string())))
        }
        "bmp" => {
            img.save_with_format(out_path, ImageFormat::Bmp)
                .map_err(|e| ToolError::failed("BMP encoding failed", Some(e.to_string())))?;
            fs::metadata(out_path)
                .map(|m| m.len())
                .map_err(|e| ToolError::failed("Failed to stat BMP output", Some(e.to_string())))
        }
        "gif" => {
            img.save_with_format(out_path, ImageFormat::Gif)
                .map_err(|e| ToolError::failed("GIF encoding failed", Some(e.to_string())))?;
            fs::metadata(out_path)
                .map(|m| m.len())
                .map_err(|e| ToolError::failed("Failed to stat GIF output", Some(e.to_string())))
        }
        _ => Err(ToolError::unsupported(format!(
            "Unsupported target image format '{ext}'."
        ))),
    }
}

fn try_encode_lossy_webp_via_ffmpeg(
    img: &DynamicImage,
    out_path: &Path,
    quality: u8,
) -> Result<u64, ()> {
    let sidecar = locate_ffmpeg_binary().map_err(|_| ())?;
    let rgba = img.to_rgba8();
    let size_arg = format!("{}x{}", rgba.width(), rgba.height());
    let q_arg = quality.to_string();

    let mut child = sidecar
        .command_ffmpeg()
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &size_arg,
            "-i",
            "pipe:0",
            "-frames:v",
            "1",
            "-quality",
            &q_arg,
        ])
        .arg(out_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(rgba.as_raw());
    }
    let status = child.wait().map_err(|_| ())?;
    if !status.success() {
        return Err(());
    }
    fs::metadata(out_path).map(|m| m.len()).map_err(|_| ())
}

/// Losslessly strips EXIF & metadata chunks from JPEG/PNG/WebP containers without re-encoding pixels,
/// falling back to clean pixel re-save for other formats.
pub fn strip_metadata_preserving_pixels(input: &Path, out_path: &Path) -> Result<u64, ToolError> {
    let ext = input
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "jpg" | "jpeg" => {
            let raw = fs::read(input).map_err(|e| {
                ToolError::invalid(format!("Cannot read '{}': {e}", input.display()))
            })?;
            let mut jpeg = Jpeg::from_bytes(Bytes::from(raw)).map_err(|e| {
                ToolError::corrupt(input, "Invalid JPEG structure.", Some(e.to_string()))
            })?;
            jpeg.set_exif(None);
            // Also strip APP1 XMP / APP13 IPTC / COM comment segments
            jpeg.segments_mut().retain(|seg| {
                let m = seg.marker();
                // Keep SOF, DHT, DQT, DRI, SOS, APP0 (JFIF), APP14 (Adobe color transform)
                !matches!(m, 0xFE | 0xE1 | 0xED)
            });
            let out_file = File::create(out_path)
                .map_err(|e| ToolError::failed("Cannot write output JPEG", Some(e.to_string())))?;
            jpeg.encoder().write_to(out_file).map_err(|e| {
                ToolError::failed("Failed to save stripped JPEG", Some(e.to_string()))
            })?;
            Ok(fs::metadata(out_path).map(|m| m.len()).unwrap_or(0))
        }
        "png" => {
            let raw = fs::read(input).map_err(|e| {
                ToolError::invalid(format!("Cannot read '{}': {e}", input.display()))
            })?;
            let mut png = Png::from_bytes(Bytes::from(raw)).map_err(|e| {
                ToolError::corrupt(input, "Invalid PNG structure.", Some(e.to_string()))
            })?;
            png.set_exif(None);
            png.chunks_mut().retain(|chunk| {
                !matches!(
                    &chunk.kind(),
                    b"eXIf" | b"tEXt" | b"zTXt" | b"iTXt" | b"tIME"
                )
            });
            let out_file = File::create(out_path)
                .map_err(|e| ToolError::failed("Cannot write output PNG", Some(e.to_string())))?;
            png.encoder().write_to(out_file).map_err(|e| {
                ToolError::failed("Failed to save stripped PNG", Some(e.to_string()))
            })?;
            Ok(fs::metadata(out_path).map(|m| m.len()).unwrap_or(0))
        }
        "webp" => {
            let raw = fs::read(input).map_err(|e| {
                ToolError::invalid(format!("Cannot read '{}': {e}", input.display()))
            })?;
            let mut webp = WebP::from_bytes(Bytes::from(raw)).map_err(|e| {
                ToolError::corrupt(input, "Invalid WebP structure.", Some(e.to_string()))
            })?;
            webp.set_exif(None);
            let out_file = File::create(out_path)
                .map_err(|e| ToolError::failed("Cannot write output WebP", Some(e.to_string())))?;
            webp.encoder().write_to(out_file).map_err(|e| {
                ToolError::failed("Failed to save stripped WebP", Some(e.to_string()))
            })?;
            Ok(fs::metadata(out_path).map(|m| m.len()).unwrap_or(0))
        }
        _ => {
            let decoded = load_image_any(input)?;
            let target_ext = normalize_output_extension(&ext);
            encode_image_to_path(&decoded, out_path, target_ext, 90, false)
        }
    }
}
