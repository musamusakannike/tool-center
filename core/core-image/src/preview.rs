use core_jobs::ToolError;
use core_media::{decode_still_image_to_png, probe_media_file};
use exif::{In, Reader as ExifReader, Tag};
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

/// Extracted image header & EXIF metadata without decoding full pixel buffers when possible.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageInspectInfo {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<String>,
    pub exif_entries: Vec<(String, String)>,
}

/// Inspects image dimensions and EXIF tags using header-only streaming reads.
pub fn inspect_image_header(path: &Path) -> Result<ImageInspectInfo, ToolError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let mut info = ImageInspectInfo {
        exif_entries: extract_exif_summary(path),
        ..Default::default()
    };

    if matches!(ext.as_str(), "heic" | "heif" | "avif") {
        if let Ok(probe) = probe_media_file(path) {
            info.width = probe.width;
            info.height = probe.height;
            info.format = Some(ext.to_uppercase());
            if info.width.is_some() {
                return Ok(info);
            }
        }
    }

    match ImageReader::open(path).and_then(|r| r.with_guessed_format()) {
        Ok(reader) => {
            info.format = reader.format().map(|f| format!("{f:?}").to_uppercase());
            if let Ok((w, h)) = reader.into_dimensions() {
                info.width = Some(w);
                info.height = Some(h);
            }
            Ok(info)
        }
        Err(e) => {
            if matches!(ext.as_str(), "heic" | "heif") {
                info.format = Some("HEIC".to_string());
                Ok(info)
            } else {
                Err(ToolError::corrupt(
                    path,
                    "Unable to read image header.",
                    Some(e.to_string()),
                ))
            }
        }
    }
}

/// Reads common human-meaningful EXIF fields from a file stream without loading the whole file.
pub fn extract_exif_summary(path: &Path) -> Vec<(String, String)> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(file);
    let Ok(exif_data) = ExifReader::new().read_from_container(&mut reader) else {
        return Vec::new();
    };

    let interesting_tags: &[(Tag, &str)] = &[
        (Tag::Make, "Camera Make"),
        (Tag::Model, "Camera Model"),
        (Tag::DateTimeOriginal, "Date Taken"),
        (Tag::ExposureTime, "Exposure"),
        (Tag::FNumber, "Aperture"),
        (Tag::PhotographicSensitivity, "ISO"),
        (Tag::FocalLength, "Focal Length"),
        (Tag::Orientation, "Orientation"),
        (Tag::Software, "Software"),
    ];

    let mut out = Vec::new();
    for (tag, label) in interesting_tags {
        if let Some(field) = exif_data.get_field(*tag, In::PRIMARY) {
            let val = field.display_value().with_unit(&exif_data).to_string();
            let trimmed = val.trim().trim_matches('"').trim();
            if !trimmed.is_empty() {
                out.push(((*label).to_string(), trimmed.to_string()));
            }
        }
    }

    if exif_data.get_field(Tag::GPSLatitude, In::PRIMARY).is_some() {
        out.push(("GPS Location".to_string(), "Present in EXIF".to_string()));
    }

    out
}

/// Decodes any supported image (including `.heic`/`.heif`/`.avif` via the LGPL sidecar fallback)
/// into a `DynamicImage`.
pub fn load_image_any(path: &Path) -> Result<DynamicImage, ToolError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if matches!(ext.as_str(), "heic" | "heif") {
        let png_bytes = decode_still_image_to_png(path, None)?;
        return image::load_from_memory_with_format(&png_bytes, ImageFormat::Png).map_err(|e| {
            ToolError::corrupt(
                path,
                "Failed to decode HEIC image stream.",
                Some(e.to_string()),
            )
        });
    }

    match ImageReader::open(path)
        .map_err(|e| ToolError::invalid(format!("Cannot open '{}': {e}", path.display())))?
        .with_guessed_format()
        .map_err(|e| ToolError::invalid(format!("Cannot inspect '{}': {e}", path.display())))?
        .decode()
    {
        Ok(img) => Ok(img),
        Err(primary_err) => {
            // Fallback to LGPL FFmpeg decoder for AVIF or uncommon TIFF/WebP variants
            if let Ok(png_bytes) = decode_still_image_to_png(path, None) {
                if let Ok(img) = image::load_from_memory_with_format(&png_bytes, ImageFormat::Png) {
                    return Ok(img);
                }
            }
            Err(ToolError::corrupt(
                path,
                format!(
                    "Corrupt or unsupported image '{}'.",
                    path.file_name().and_then(|s| s.to_str()).unwrap_or("file")
                ),
                Some(primary_err.to_string()),
            ))
        }
    }
}

/// Renders an image (such as HEIC, TIFF, or AVIF) into bounded PNG bytes for fast UI preview.
pub fn render_image_preview_png(path: &Path, max_dimension: u32) -> Result<Vec<u8>, ToolError> {
    let bound = max_dimension.clamp(128, 4096);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if matches!(ext.as_str(), "heic" | "heif" | "avif") {
        if let Ok(bytes) = decode_still_image_to_png(path, Some(bound)) {
            return Ok(bytes);
        }
    }

    let img = load_image_any(path)?;
    let (w, h) = (img.width(), img.height());
    let resized = if w > bound || h > bound {
        img.resize(bound, bound, FilterType::Triangle)
    } else {
        img
    };

    let mut out = Cursor::new(Vec::new());
    resized
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| ToolError::failed("Failed to encode preview PNG", Some(e.to_string())))?;
    Ok(out.into_inner())
}
