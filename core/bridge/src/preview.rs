use chrono::{DateTime, Local};
use core_archive::{list_archive_entries, ArchiveEntryItem};
use core_image::inspect_image_header;
use core_media::probe_media_file;
use core_pdf::inspect_pdf_file;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataItem {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInspectionPayload {
    pub path: String,
    pub file_name: String,
    pub extension: String,
    pub preview_kind: String,
    pub language_hint: Option<String>,
    pub size_bytes: u64,
    pub modified_iso: Option<String>,
    pub created_iso: Option<String>,
    pub readonly: bool,
    pub page_count: Option<u32>,
    pub pdf_encrypted: bool,
    pub metadata: Vec<MetadataItem>,
    pub archive_entries: Option<Vec<ArchiveEntryItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPreviewPayload {
    pub text: String,
    pub truncated: bool,
    pub total_bytes: u64,
    pub line_count_estimate: usize,
    pub csv_headers: Option<Vec<String>>,
    pub csv_rows: Option<Vec<Vec<String>>>,
}

pub fn inspect_file_full(path: &Path) -> Result<FileInspectionPayload, String> {
    let meta =
        fs::metadata(path).map_err(|e| format!("Cannot read file '{}': {e}", path.display()))?;
    if meta.is_dir() {
        return Err(format!("'{}' is a folder, not a file.", path.display()));
    }

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let lower_name = file_name.to_ascii_lowercase();
    let ext = if lower_name.ends_with(".tar.gz") {
        "tar.gz".to_string()
    } else if lower_name.ends_with(".tar.zst") {
        "tar.zst".to_string()
    } else if lower_name.ends_with(".tar.xz") {
        "tar.xz".to_string()
    } else {
        path.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default()
    };

    let (mut preview_kind, lang_hint) = classify_extension(&ext, &lower_name);

    // Sniff unknown extensions (e.g. README, LICENSE, Makefile, config files) using first 4 KiB
    if preview_kind == "binary" && meta.len() > 0 && looks_like_utf8_text(path) {
        preview_kind = "text".to_string();
    }

    let modified_iso = meta.modified().ok().map(|t| {
        DateTime::<Local>::from(t)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    });
    let created_iso = meta.created().ok().map(|t| {
        DateTime::<Local>::from(t)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    });

    let mut items = vec![
        MetadataItem {
            label: "File Name".to_string(),
            value: file_name.clone(),
        },
        MetadataItem {
            label: "Type".to_string(),
            value: if ext.is_empty() {
                preview_kind.to_uppercase()
            } else {
                format!(".{} ({})", ext.to_uppercase(), preview_kind)
            },
        },
        MetadataItem {
            label: "Size".to_string(),
            value: format!("{} ({} bytes)", human_bytes(meta.len()), meta.len()),
        },
    ];

    if let Some(m) = &modified_iso {
        items.push(MetadataItem {
            label: "Modified".to_string(),
            value: m.clone(),
        });
    }
    if let Some(c) = &created_iso {
        items.push(MetadataItem {
            label: "Created".to_string(),
            value: c.clone(),
        });
    }
    items.push(MetadataItem {
        label: "Permissions".to_string(),
        value: if meta.permissions().readonly() {
            "Read-only".to_string()
        } else {
            "Read & Write".to_string()
        },
    });

    let mut page_count = None;
    let mut pdf_encrypted = false;

    match preview_kind.as_str() {
        "image_native" | "image_decoded" => {
            if let Ok(img_info) = inspect_image_header(path) {
                if let (Some(w), Some(h)) = (img_info.width, img_info.height) {
                    items.push(MetadataItem {
                        label: "Dimensions".to_string(),
                        value: format!("{w} × {h} px"),
                    });
                }
                if let Some(fmt) = img_info.format {
                    items.push(MetadataItem {
                        label: "Image Format".to_string(),
                        value: fmt,
                    });
                }
                for (k, v) in img_info.exif_entries {
                    items.push(MetadataItem { label: k, value: v });
                }
            }
        }
        "pdf" => {
            if let Ok(pdf_info) = inspect_pdf_file(path) {
                page_count = Some(pdf_info.page_count);
                pdf_encrypted = pdf_info.encrypted;
                if pdf_info.page_count > 0 {
                    items.push(MetadataItem {
                        label: "Pages".to_string(),
                        value: pdf_info.page_count.to_string(),
                    });
                }
                items.push(MetadataItem {
                    label: "PDF Version".to_string(),
                    value: pdf_info.pdf_version,
                });
                items.push(MetadataItem {
                    label: "Encrypted".to_string(),
                    value: if pdf_info.encrypted { "Yes" } else { "No" }.to_string(),
                });
                if let Some(title) = pdf_info.title {
                    items.push(MetadataItem {
                        label: "Title".to_string(),
                        value: title,
                    });
                }
                if let Some(author) = pdf_info.author {
                    items.push(MetadataItem {
                        label: "Author".to_string(),
                        value: author,
                    });
                }
                if let Some(producer) = pdf_info.producer {
                    items.push(MetadataItem {
                        label: "Producer".to_string(),
                        value: producer,
                    });
                }
            }
        }
        "video" | "audio" => {
            if let Ok(media) = probe_media_file(path) {
                if let Some(dur) = media.duration_secs {
                    items.push(MetadataItem {
                        label: "Duration".to_string(),
                        value: format_duration(dur),
                    });
                }
                if let (Some(w), Some(h)) = (media.width, media.height) {
                    items.push(MetadataItem {
                        label: "Resolution".to_string(),
                        value: format!("{w} × {h}"),
                    });
                }
                if let Some(vcodec) = media.video_codec {
                    items.push(MetadataItem {
                        label: "Video Codec".to_string(),
                        value: vcodec,
                    });
                }
                if let Some(fps) = media.fps {
                    items.push(MetadataItem {
                        label: "Frame Rate".to_string(),
                        value: fps,
                    });
                }
                if let Some(acodec) = media.audio_codec {
                    items.push(MetadataItem {
                        label: "Audio Codec".to_string(),
                        value: acodec,
                    });
                }
                if let Some(sr) = media.sample_rate_hz {
                    items.push(MetadataItem {
                        label: "Sample Rate".to_string(),
                        value: format!("{sr} Hz"),
                    });
                }
                if let Some(br) = media.bit_rate_kbps {
                    items.push(MetadataItem {
                        label: "Bitrate".to_string(),
                        value: format!("{br} kbps"),
                    });
                }
            }
        }
        _ => {}
    }

    let archive_entries = if preview_kind == "archive" {
        match list_archive_entries(path, 500) {
            Ok(entries) => {
                let total_uncompressed: u64 = entries.iter().map(|e| e.uncompressed_bytes).sum();
                items.push(MetadataItem {
                    label: "Archive Entries".to_string(),
                    value: entries.len().to_string(),
                });
                if total_uncompressed > 0 {
                    items.push(MetadataItem {
                        label: "Uncompressed Size".to_string(),
                        value: human_bytes(total_uncompressed),
                    });
                }
                Some(entries)
            }
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(FileInspectionPayload {
        path: path.display().to_string(),
        file_name,
        extension: ext,
        preview_kind,
        language_hint: lang_hint,
        size_bytes: meta.len(),
        modified_iso,
        created_iso,
        readonly: meta.permissions().readonly(),
        page_count,
        pdf_encrypted,
        metadata: items,
        archive_entries,
    })
}

/// Streams at most `max_bytes` from a text, code, Markdown, CSV, or JSON file so large multi-GB
/// files are never fully loaded into memory.
pub fn read_bounded_text_preview(
    path: &Path,
    max_bytes: u32,
) -> Result<TextPreviewPayload, String> {
    let meta = fs::metadata(path).map_err(|e| format!("Cannot read '{}': {e}", path.display()))?;
    let total_bytes = meta.len();
    let cap = max_bytes.clamp(4096, 1_048_576) as usize;

    let file = File::open(path).map_err(|e| format!("Cannot open '{}': {e}", path.display()))?;
    let mut chunk = Vec::with_capacity(cap.min(total_bytes as usize));
    file.take(cap as u64)
        .read_to_end(&mut chunk)
        .map_err(|e| format!("Read error: {e}"))?;

    let truncated = total_bytes > (chunk.len() as u64);
    let mut text = decode_utf8_gracefully(&chunk, truncated);

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let mut csv_headers = None;
    let mut csv_rows = None;

    if matches!(ext.as_str(), "csv" | "tsv") {
        let delim = if ext == "tsv" { b'\t' } else { b',' };
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(delim)
            .flexible(true)
            .from_reader(text.as_bytes());
        if let Ok(hdrs) = rdr.headers() {
            csv_headers = Some(hdrs.iter().map(|s| s.to_string()).collect());
        }
        let mut rows = Vec::new();
        for rec in rdr.records().take(200).flatten() {
            rows.push(rec.iter().map(|s| s.to_string()).collect());
        }
        csv_rows = Some(rows);
    } else if ext == "json" && !truncated {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                text = pretty;
            }
        }
    }

    let line_count_estimate = text.lines().count();
    Ok(TextPreviewPayload {
        text,
        truncated,
        total_bytes,
        line_count_estimate,
        csv_headers,
        csv_rows,
    })
}

fn decode_utf8_gracefully(bytes: &[u8], truncated: bool) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(err) if truncated && err.error_len().is_none() => {
            // Cut cleanly at the incomplete multi-byte UTF-8 sequence at the end of the chunk
            String::from_utf8_lossy(&bytes[..err.valid_up_to()]).to_string()
        }
        Err(_) => String::from_utf8_lossy(bytes).to_string(),
    }
}

fn looks_like_utf8_text(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut sample = Vec::with_capacity(4096);
    if file.take(4096).read_to_end(&mut sample).is_err() || sample.is_empty() {
        return false;
    }
    if sample.contains(&0u8) {
        return false;
    }
    std::str::from_utf8(&sample).is_ok()
}

fn classify_extension(ext: &str, lower_name: &str) -> (String, Option<String>) {
    match ext {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "ico" => ("image_native".into(), None),
        "heic" | "heif" | "tiff" | "tif" | "avif" => ("image_decoded".into(), None),
        "svg" => ("svg".into(), Some("xml".into())),
        "mp4" | "mkv" | "mov" | "webm" | "avi" | "m4v" => ("video".into(), None),
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "opus" => ("audio".into(), None),
        "pdf" => ("pdf".into(), None),
        "docx" | "xlsx" | "pptx" | "doc" | "xls" | "ppt" => ("office_coming_soon".into(), None),
        "zip" | "tar" | "gz" | "tgz" | "tar.gz" | "7z" | "zst" | "tar.zst" | "xz" | "tar.xz" => {
            ("archive".into(), None)
        }
        "md" | "markdown" => ("markdown".into(), Some("markdown".into())),
        "csv" | "tsv" => ("csv".into(), None),
        "json" => ("json".into(), Some("json".into())),
        "rs" => ("code".into(), Some("rust".into())),
        "dart" => ("code".into(), Some("dart".into())),
        "py" => ("code".into(), Some("python".into())),
        "js" | "mjs" | "cjs" | "jsx" => ("code".into(), Some("javascript".into())),
        "ts" | "tsx" => ("code".into(), Some("typescript".into())),
        "go" => ("code".into(), Some("go".into())),
        "c" | "h" | "cpp" | "hpp" | "cc" => ("code".into(), Some("cpp".into())),
        "java" => ("code".into(), Some("java".into())),
        "kt" | "kts" => ("code".into(), Some("kotlin".into())),
        "swift" => ("code".into(), Some("swift".into())),
        "sh" | "bash" | "zsh" => ("code".into(), Some("bash".into())),
        "yaml" | "yml" => ("code".into(), Some("yaml".into())),
        "toml" | "ini" | "cfg" => ("code".into(), Some("ini".into())),
        "xml" | "html" | "htm" => ("code".into(), Some("xml".into())),
        "css" | "scss" => ("code".into(), Some("css".into())),
        "sql" => ("code".into(), Some("sql".into())),
        "txt" | "log" | "env" | "conf" => ("text".into(), None),
        _ => {
            if matches!(
                lower_name,
                "dockerfile" | "makefile" | "cmakelists.txt" | ".gitignore"
            ) {
                ("code".into(), Some("bash".into()))
            } else if lower_name.starts_with("license") || lower_name.starts_with("readme") {
                ("text".into(), None)
            } else {
                ("binary".into(), None)
            }
        }
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < MB {
        format!("{:.1} KB", b / KB)
    } else if b < GB {
        format!("{:.2} MB", b / MB)
    } else {
        format!("{:.2} GB", b / GB)
    }
}

fn format_duration(secs: f64) -> String {
    let total = secs.round().max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
