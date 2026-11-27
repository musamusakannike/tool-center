use core_jobs::{CancelHandle, ToolError};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use lzma_rust2::{XzOptions, XzReader, XzWriter};
use serde::{Deserialize, Serialize};
use sevenz_rust2::{ArchiveEntry, ArchiveReader, ArchiveWriter, Password};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntryItem {
    pub path: String,
    pub is_dir: bool,
    pub uncompressed_bytes: u64,
    pub compressed_bytes: Option<u64>,
}

/// Detects canonical archive kind from file name (handling compound `.tar.gz`, `.tar.zst`, `.tar.xz`).
pub fn detect_archive_kind(path: &Path) -> Option<&'static str> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())?;
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some("tar.gz")
    } else if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        Some("tar.zst")
    } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        Some("tar.xz")
    } else if name.ends_with(".zip") {
        Some("zip")
    } else if name.ends_with(".7z") {
        Some("7z")
    } else if name.ends_with(".tar") {
        Some("tar")
    } else if name.ends_with(".gz") {
        Some("gz")
    } else if name.ends_with(".zst") {
        Some("zst")
    } else if name.ends_with(".xz") {
        Some("xz")
    } else {
        None
    }
}

/// Guards against Zip-Slip path traversal by rejecting parent (`..`), root, or prefix components.
pub fn sanitize_entry_rel_path(raw: &str) -> Result<PathBuf, ToolError> {
    let normalized = raw.replace('\\', "/");
    let candidate = Path::new(&normalized);
    let mut clean = PathBuf::new();
    for comp in candidate.components() {
        match comp {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ToolError::invalid(format!(
                    "Blocked unsafe archive entry path '{raw}' (path traversal protection)."
                )));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(ToolError::invalid("Archive contains an empty entry path."));
    }
    Ok(clean)
}

/// Collects `(disk_path, archive_relative_path)` pairs for files and directories without reading file contents.
fn collect_input_entries(inputs: &[PathBuf]) -> Result<Vec<(PathBuf, String)>, ToolError> {
    let mut list = Vec::new();
    for root in inputs {
        let meta = fs::metadata(root)
            .map_err(|e| ToolError::invalid(format!("Cannot access '{}': {e}", root.display())))?;
        if meta.is_file() {
            let name = root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string();
            list.push((root.clone(), name));
        } else if meta.is_dir() {
            let base_parent = root.parent().unwrap_or(root);
            for entry in WalkDir::new(root).follow_links(false) {
                let entry = entry.map_err(|e| {
                    ToolError::failed("Failed while walking directory", Some(e.to_string()))
                })?;
                if entry.file_type().is_file() {
                    let rel = entry
                        .path()
                        .strip_prefix(base_parent)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                        .replace('\\', "/");
                    list.push((entry.path().to_path_buf(), rel));
                }
            }
        }
    }
    if list.is_empty() {
        return Err(ToolError::invalid(
            "Selected inputs contain no files to archive.",
        ));
    }
    Ok(list)
}

/// Streams a single reader into writer in 64 KiB chunks with cooperative cancellation checks.
fn stream_copy_cancellable<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    cancel: &CancelHandle,
) -> Result<u64, ToolError> {
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        cancel.check()?;
        let n = reader
            .read(&mut buf)
            .map_err(|e| ToolError::failed("Stream read error", Some(e.to_string())))?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&buf[..n])
            .map_err(|e| ToolError::failed("Stream write error", Some(e.to_string())))?;
        total += n as u64;
    }
    Ok(total)
}

/// Creates an archive (`zip`, `tar.gz`, `7z`, `tar.zst`, `tar.xz`, or single-file `.zst`/`.xz`)
/// using streaming 64 KiB I/O so multi-GB archives never load into memory.
pub fn create_archive_streaming<F>(
    inputs: &[PathBuf],
    format: &str,
    out_path: &Path,
    level: u32,
    cancel: &CancelHandle,
    report_progress: F,
) -> Result<u64, ToolError>
where
    F: Fn(f32, String),
{
    let entries = collect_input_entries(inputs)?;
    let total = entries.len();

    match format {
        "zip" => {
            let file = File::create(out_path)
                .map_err(|e| ToolError::failed("Cannot create ZIP file", Some(e.to_string())))?;
            let mut zip = ZipWriter::new(BufWriter::new(file));
            let opts = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(level.clamp(1, 9) as i64))
                .large_file(true);

            for (idx, (disk_path, rel_name)) in entries.iter().enumerate() {
                cancel.check()?;
                report_progress(
                    idx as f32 / total as f32,
                    format!("Compressing {} ({}/{total})", rel_name, idx + 1),
                );
                zip.start_file(rel_name, opts).map_err(|e| {
                    ToolError::failed("Failed to add entry to ZIP", Some(e.to_string()))
                })?;
                let mut src = BufReader::new(File::open(disk_path).map_err(|e| {
                    ToolError::invalid(format!("Cannot open '{}': {e}", disk_path.display()))
                })?);
                stream_copy_cancellable(&mut src, &mut zip, cancel)?;
            }
            let mut inner = zip
                .finish()
                .map_err(|e| ToolError::failed("Failed to finalize ZIP", Some(e.to_string())))?;
            inner
                .flush()
                .map_err(|e| ToolError::failed("Flush error", Some(e.to_string())))?;
        }
        "tar.gz" => {
            let file = File::create(out_path).map_err(|e| {
                ToolError::failed("Cannot create .tar.gz file", Some(e.to_string()))
            })?;
            let enc = GzEncoder::new(BufWriter::new(file), Compression::new(level.clamp(1, 9)));
            write_tar_entries(enc, &entries, cancel, &report_progress)?
                .finish()
                .map_err(|e| {
                    ToolError::failed("Failed finalizing gzip stream", Some(e.to_string()))
                })?;
        }
        "tar.zst" => {
            let file = File::create(out_path).map_err(|e| {
                ToolError::failed("Cannot create .tar.zst file", Some(e.to_string()))
            })?;
            let enc = zstd::Encoder::new(BufWriter::new(file), level.clamp(1, 19) as i32).map_err(
                |e| ToolError::failed("Failed initializing Zstandard encoder", Some(e.to_string())),
            )?;
            write_tar_entries(enc, &entries, cancel, &report_progress)?
                .finish()
                .map_err(|e| {
                    ToolError::failed("Failed finalizing Zstandard stream", Some(e.to_string()))
                })?;
        }
        "tar.xz" => {
            let file = File::create(out_path).map_err(|e| {
                ToolError::failed("Cannot create .tar.xz file", Some(e.to_string()))
            })?;
            let enc = XzWriter::new(
                BufWriter::new(file),
                XzOptions::with_preset(level.clamp(1, 9)),
            )
            .map_err(|e| {
                ToolError::failed("Failed initializing XZ encoder", Some(e.to_string()))
            })?;
            write_tar_entries(enc, &entries, cancel, &report_progress)?
                .finish()
                .map_err(|e| {
                    ToolError::failed("Failed finalizing XZ stream", Some(e.to_string()))
                })?;
        }
        "zst" => {
            if entries.len() != 1 {
                return Err(ToolError::invalid(
                    "Single-stream .zst compresses 1 file; choose .tar.zst for multiple files or folders.",
                ));
            }
            let file = File::create(out_path)
                .map_err(|e| ToolError::failed("Cannot create .zst file", Some(e.to_string())))?;
            let mut enc = zstd::Encoder::new(BufWriter::new(file), level.clamp(1, 19) as i32)
                .map_err(|e| ToolError::failed("Zstd init error", Some(e.to_string())))?;
            let mut src = BufReader::new(File::open(&entries[0].0).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", entries[0].0.display()))
            })?);
            stream_copy_cancellable(&mut src, &mut enc, cancel)?;
            enc.finish()
                .map_err(|e| ToolError::failed("Zstd finish error", Some(e.to_string())))?;
        }
        "7z" => {
            let file = File::create(out_path)
                .map_err(|e| ToolError::failed("Cannot create .7z file", Some(e.to_string())))?;
            let mut sz = ArchiveWriter::new(BufWriter::new(file)).map_err(|e| {
                ToolError::failed("Failed initializing 7z writer", Some(e.to_string()))
            })?;
            for (idx, (disk_path, rel_name)) in entries.iter().enumerate() {
                cancel.check()?;
                report_progress(
                    idx as f32 / total as f32,
                    format!("Compressing {} ({}/{total})", rel_name, idx + 1),
                );
                let entry = ArchiveEntry::from_path(disk_path, rel_name.clone());
                let src = File::open(disk_path).map_err(|e| {
                    ToolError::invalid(format!("Cannot open '{}': {e}", disk_path.display()))
                })?;
                sz.push_archive_entry(entry, Some(BufReader::new(src)))
                    .map_err(|e| ToolError::failed("7z compression error", Some(e.to_string())))?;
            }
            sz.finish().map_err(|e| {
                ToolError::failed("Failed finalizing 7z archive", Some(e.to_string()))
            })?;
        }
        _ => {
            return Err(ToolError::unsupported(format!(
                "Unsupported archive format '{format}'."
            )));
        }
    }

    Ok(fs::metadata(out_path).map(|m| m.len()).unwrap_or(0))
}

fn write_tar_entries<W: Write, F: Fn(f32, String)>(
    writer: W,
    entries: &[(PathBuf, String)],
    cancel: &CancelHandle,
    report_progress: &F,
) -> Result<W, ToolError> {
    let mut builder = tar::Builder::new(writer);
    let total = entries.len();
    for (idx, (disk_path, rel_name)) in entries.iter().enumerate() {
        cancel.check()?;
        report_progress(
            idx as f32 / total as f32,
            format!("Packing {} ({}/{total})", rel_name, idx + 1),
        );
        let mut f = File::open(disk_path).map_err(|e| {
            ToolError::invalid(format!("Cannot open '{}': {e}", disk_path.display()))
        })?;
        builder
            .append_file(rel_name, &mut f)
            .map_err(|e| ToolError::failed("Failed appending tar entry", Some(e.to_string())))?;
    }
    builder
        .into_inner()
        .map_err(|e| ToolError::failed("Failed finishing tar archive", Some(e.to_string())))
}

/// Extracts any supported archive (`zip`, `tar.gz`, `7z`, `tar.zst`, `tar.xz`, `tar`, `gz`, `zst`, `xz`)
/// into `dest_dir` with streaming I/O and Zip-Slip path sanitization.
pub fn extract_archive_streaming<F>(
    archive_path: &Path,
    dest_dir: &Path,
    cancel: &CancelHandle,
    report_progress: F,
) -> Result<(usize, u64), ToolError>
where
    F: Fn(f32, String),
{
    let kind = detect_archive_kind(archive_path).ok_or_else(|| {
        ToolError::unsupported(format!(
            "Unsupported archive extension for '{}'.",
            archive_path.display()
        ))
    })?;

    fs::create_dir_all(dest_dir)
        .map_err(|e| ToolError::failed("Cannot create extraction folder", Some(e.to_string())))?;

    match kind {
        "zip" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| {
                ToolError::corrupt(
                    archive_path,
                    "Invalid or corrupted ZIP archive.",
                    Some(e.to_string()),
                )
            })?;
            let total = zip.len().max(1);
            let mut files_extracted = 0usize;
            let mut bytes_written = 0u64;

            for i in 0..zip.len() {
                cancel.check()?;
                let mut entry = zip.by_index(i).map_err(|e| {
                    ToolError::corrupt(archive_path, "Corrupted ZIP entry.", Some(e.to_string()))
                })?;
                let entry_name = entry
                    .name()
                    .map_err(|e| {
                        ToolError::corrupt(
                            archive_path,
                            "Invalid ZIP entry name.",
                            Some(e.to_string()),
                        )
                    })?
                    .into_owned();
                let safe_rel = sanitize_entry_rel_path(&entry_name)?;
                let out_target = dest_dir.join(&safe_rel);

                report_progress(
                    i as f32 / total as f32,
                    format!("Extracting {}", safe_rel.display()),
                );

                if entry.is_dir() || entry_name.ends_with('/') {
                    fs::create_dir_all(&out_target).map_err(|e| {
                        ToolError::failed("Cannot create directory", Some(e.to_string()))
                    })?;
                } else {
                    if let Some(parent) = out_target.parent() {
                        fs::create_dir_all(parent).map_err(|e| {
                            ToolError::failed("Cannot create parent directory", Some(e.to_string()))
                        })?;
                    }
                    let mut out_file = BufWriter::new(File::create(&out_target).map_err(|e| {
                        ToolError::failed("Cannot create extracted file", Some(e.to_string()))
                    })?);
                    bytes_written += stream_copy_cancellable(&mut entry, &mut out_file, cancel)?;
                    out_file
                        .flush()
                        .map_err(|e| ToolError::failed("Flush error", Some(e.to_string())))?;
                    files_extracted += 1;
                }
            }
            Ok((files_extracted, bytes_written))
        }
        "tar.gz" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            let dec = GzDecoder::new(BufReader::new(file));
            extract_tar_stream(dec, archive_path, dest_dir, cancel, &report_progress)
        }
        "tar.zst" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            let dec = zstd::Decoder::new(BufReader::new(file)).map_err(|e| {
                ToolError::corrupt(
                    archive_path,
                    "Invalid Zstandard stream.",
                    Some(e.to_string()),
                )
            })?;
            extract_tar_stream(dec, archive_path, dest_dir, cancel, &report_progress)
        }
        "tar.xz" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            let dec = XzReader::new(BufReader::new(file), true);
            extract_tar_stream(dec, archive_path, dest_dir, cancel, &report_progress)
        }
        "tar" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            extract_tar_stream(
                BufReader::new(file),
                archive_path,
                dest_dir,
                cancel,
                &report_progress,
            )
        }
        "7z" => {
            let mut files_extracted = 0usize;
            let mut bytes_written = 0u64;
            sevenz_rust2::decompress_file_with_extract_fn(
                archive_path,
                dest_dir,
                |entry, reader, _default_dest| {
                    if cancel.is_cancelled() {
                        return Err(sevenz_rust2::Error::Other("Cancelled".into()));
                    }
                    let safe_rel = sanitize_entry_rel_path(entry.name())
                        .map_err(|e| sevenz_rust2::Error::Other(e.user_message().into()))?;
                    let target = dest_dir.join(&safe_rel);
                    report_progress(0.5, format!("Extracting {}", safe_rel.display()));
                    if entry.is_directory() {
                        fs::create_dir_all(&target)?;
                    } else {
                        if let Some(parent) = target.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        let mut out_f = BufWriter::new(File::create(&target)?);
                        bytes_written += io::copy(reader, &mut out_f)?;
                        out_f.flush()?;
                        files_extracted += 1;
                    }
                    Ok(true)
                },
            )
            .map_err(|e| {
                if cancel.is_cancelled() {
                    ToolError::Cancelled
                } else {
                    ToolError::corrupt(
                        archive_path,
                        "Invalid or corrupted 7z archive.",
                        Some(e.to_string()),
                    )
                }
            })?;
            Ok((files_extracted, bytes_written))
        }
        "gz" | "zst" | "xz" => {
            let stem = archive_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("decompressed.bin");
            let out_file_path = dest_dir.join(sanitize_entry_rel_path(stem)?);
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            let mut out_f = BufWriter::new(File::create(&out_file_path).map_err(|e| {
                ToolError::failed("Cannot create decompressed file", Some(e.to_string()))
            })?);
            let written = match kind {
                "gz" => {
                    let mut dec = GzDecoder::new(BufReader::new(file));
                    stream_copy_cancellable(&mut dec, &mut out_f, cancel)?
                }
                "zst" => {
                    let mut dec = zstd::Decoder::new(BufReader::new(file)).map_err(|e| {
                        ToolError::corrupt(
                            archive_path,
                            "Invalid Zstandard file.",
                            Some(e.to_string()),
                        )
                    })?;
                    stream_copy_cancellable(&mut dec, &mut out_f, cancel)?
                }
                _ => {
                    let mut dec = XzReader::new(BufReader::new(file), true);
                    stream_copy_cancellable(&mut dec, &mut out_f, cancel)?
                }
            };
            out_f
                .flush()
                .map_err(|e| ToolError::failed("Flush error", Some(e.to_string())))?;
            Ok((1, written))
        }
        _ => Err(ToolError::unsupported("Unsupported archive type.")),
    }
}

fn extract_tar_stream<R: Read, F: Fn(f32, String)>(
    reader: R,
    archive_path: &Path,
    dest_dir: &Path,
    cancel: &CancelHandle,
    report_progress: &F,
) -> Result<(usize, u64), ToolError> {
    let mut tar = tar::Archive::new(reader);
    let entries = tar.entries().map_err(|e| {
        ToolError::corrupt(
            archive_path,
            "Invalid or corrupted tar archive.",
            Some(e.to_string()),
        )
    })?;

    let mut files_extracted = 0usize;
    let mut bytes_written = 0u64;

    for (idx, item) in entries.enumerate() {
        cancel.check()?;
        let mut entry = item.map_err(|e| {
            ToolError::corrupt(
                archive_path,
                "Corrupted tar stream entry.",
                Some(e.to_string()),
            )
        })?;
        let raw_path = entry
            .path()
            .map_err(|e| {
                ToolError::corrupt(archive_path, "Invalid tar path.", Some(e.to_string()))
            })?
            .to_string_lossy()
            .to_string();
        let safe_rel = sanitize_entry_rel_path(&raw_path)?;
        let out_target = dest_dir.join(&safe_rel);

        report_progress(
            ((idx % 100) as f32) / 100.0,
            format!("Extracting {}", safe_rel.display()),
        );

        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            fs::create_dir_all(&out_target)
                .map_err(|e| ToolError::failed("Cannot create directory", Some(e.to_string())))?;
        } else if entry_type.is_file() {
            if let Some(parent) = out_target.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    ToolError::failed("Cannot create parent directory", Some(e.to_string()))
                })?;
            }
            let mut out_f = BufWriter::new(File::create(&out_target).map_err(|e| {
                ToolError::failed("Cannot create extracted file", Some(e.to_string()))
            })?);
            bytes_written += stream_copy_cancellable(&mut entry, &mut out_f, cancel)?;
            out_f
                .flush()
                .map_err(|e| ToolError::failed("Flush error", Some(e.to_string())))?;
            files_extracted += 1;
        }
    }

    Ok((files_extracted, bytes_written))
}

/// Lists archive entries without extracting to disk (bounded to `max_entries`).
pub fn list_archive_entries(
    archive_path: &Path,
    max_entries: usize,
) -> Result<Vec<ArchiveEntryItem>, ToolError> {
    let kind = detect_archive_kind(archive_path).ok_or_else(|| {
        ToolError::unsupported(format!(
            "Unsupported archive extension for '{}'.",
            archive_path.display()
        ))
    })?;

    let mut items = Vec::new();
    match kind {
        "zip" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| {
                ToolError::corrupt(archive_path, "Corrupted ZIP archive.", Some(e.to_string()))
            })?;
            for i in 0..zip.len().min(max_entries) {
                if let Ok(entry) = zip.by_index_raw(i) {
                    let name = entry
                        .name()
                        .map(|c| c.into_owned())
                        .unwrap_or_else(|_| format!("entry_{i}"));
                    items.push(ArchiveEntryItem {
                        path: name,
                        is_dir: entry.is_dir(),
                        uncompressed_bytes: entry.size(),
                        compressed_bytes: Some(entry.compressed_size()),
                    });
                }
            }
        }
        "7z" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            let reader =
                ArchiveReader::new(BufReader::new(file), Password::empty()).map_err(|e| {
                    ToolError::corrupt(archive_path, "Corrupted 7z archive.", Some(e.to_string()))
                })?;
            for entry in reader.archive().files.iter().take(max_entries) {
                items.push(ArchiveEntryItem {
                    path: entry.name().to_string(),
                    is_dir: entry.is_directory(),
                    uncompressed_bytes: entry.size(),
                    compressed_bytes: Some(entry.compressed_size),
                });
            }
        }
        "tar.gz" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            list_tar_stream(
                GzDecoder::new(BufReader::new(file)),
                archive_path,
                max_entries,
                &mut items,
            )?;
        }
        "tar.zst" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            let dec = zstd::Decoder::new(BufReader::new(file)).map_err(|e| {
                ToolError::corrupt(archive_path, "Corrupted Zstd archive.", Some(e.to_string()))
            })?;
            list_tar_stream(dec, archive_path, max_entries, &mut items)?;
        }
        "tar.xz" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            list_tar_stream(
                XzReader::new(BufReader::new(file), true),
                archive_path,
                max_entries,
                &mut items,
            )?;
        }
        "tar" => {
            let file = File::open(archive_path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", archive_path.display()))
            })?;
            list_tar_stream(BufReader::new(file), archive_path, max_entries, &mut items)?;
        }
        "gz" | "zst" | "xz" => {
            let stem = archive_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("stream")
                .to_string();
            let comp_len = fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);
            items.push(ArchiveEntryItem {
                path: stem,
                is_dir: false,
                uncompressed_bytes: 0,
                compressed_bytes: Some(comp_len),
            });
        }
        _ => {}
    }

    Ok(items)
}

fn list_tar_stream<R: Read>(
    reader: R,
    archive_path: &Path,
    max_entries: usize,
    items: &mut Vec<ArchiveEntryItem>,
) -> Result<(), ToolError> {
    let mut tar = tar::Archive::new(reader);
    let entries = tar.entries().map_err(|e| {
        ToolError::corrupt(archive_path, "Corrupted tar archive.", Some(e.to_string()))
    })?;
    for entry in entries.take(max_entries) {
        let entry = entry.map_err(|e| {
            ToolError::corrupt(
                archive_path,
                "Corrupted tar header while listing.",
                Some(e.to_string()),
            )
        })?;
        let path_str = entry
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".into());
        items.push(ArchiveEntryItem {
            path: path_str,
            is_dir: entry.header().entry_type().is_dir(),
            uncompressed_bytes: entry.size(),
            compressed_bytes: None,
        });
    }
    Ok(())
}
