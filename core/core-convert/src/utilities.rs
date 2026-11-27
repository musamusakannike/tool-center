use core_jobs::{
    resolve_safe_output_path, CancelHandle, OptionField, OptionKind, OutputArtifact, SelectChoice,
    Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError, ToolInputSpec, ToolRegistry,
    ToolRunResult,
};
use md5::Md5;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn register_utility_tools(registry: &ToolRegistry) {
    registry.register(BulkRenameTool);
    registry.register(FileChecksumTool);
    registry.register(DuplicateFinderTool);
}

// ============================================================================
// 1. Bulk Rename (`util.bulk_rename`) — Non-destructive!
// ============================================================================
pub struct BulkRenameTool;

impl Tool for BulkRenameTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "util.bulk_rename".into(),
            name: "Bulk Rename".into(),
            name_key: "tool_bulk_rename".into(),
            description:
                "Batch-rename files using find/replace, prefixes, suffixes, case rules, and sequence numbers (writes safe copies — originals untouched)."
                    .into(),
            description_key: "tool_bulk_rename_desc".into(),
            category: ToolCategory::Files,
            icon: "rename".into(),
            keywords: vec![
                "rename".into(),
                "sequence".into(),
                "prefix".into(),
                "suffix".into(),
                "replace".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(&[]),
            options_schema: vec![
                OptionField {
                    key: "prefix".into(),
                    label: "Prefix".into(),
                    label_key: "opt_rename_prefix".into(),
                    hint: "Text added to the start of each file name.".into(),
                    hint_key: "opt_rename_prefix_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("e.g. trip_2026_".into()),
                        secret: false,
                    },
                    default_value: json!(""),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "suffix".into(),
                    label: "Suffix".into(),
                    label_key: "opt_rename_suffix".into(),
                    hint: "Text added right before the file extension.".into(),
                    hint_key: "opt_rename_suffix_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("e.g. _final".into()),
                        secret: false,
                    },
                    default_value: json!(""),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "find_text".into(),
                    label: "Find Text".into(),
                    label_key: "opt_rename_find".into(),
                    hint: "Substring in the current file stem to replace.".into(),
                    hint_key: "opt_rename_find_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("IMG_".into()),
                        secret: false,
                    },
                    default_value: json!(""),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "replace_text".into(),
                    label: "Replace With".into(),
                    label_key: "opt_rename_replace".into(),
                    hint: "Replacement text (leave empty to delete matched text).".into(),
                    hint_key: "opt_rename_replace_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("Photo_".into()),
                        secret: false,
                    },
                    default_value: json!(""),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "case_mode".into(),
                    label: "Letter Case".into(),
                    label_key: "opt_rename_case".into(),
                    hint: "Normalize capitalization across all selected files.".into(),
                    hint_key: "opt_rename_case_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "keep".into(),
                                label: "Keep Existing Case".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "lowercase".into(),
                                label: "lowercase".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "uppercase".into(),
                                label: "UPPERCASE".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "snake_case".into(),
                                label: "snake_case (spaces → underscores)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("keep"),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "add_numbering".into(),
                    label: "Add Sequence Numbers (001, 002, ...)".into(),
                    label_key: "opt_rename_seq".into(),
                    hint: "Appends a padded counter to keep files ordered.".into(),
                    hint_key: "opt_rename_seq_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(false),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "dry_run".into(),
                    label: "Preview Only (Dry Run)".into(),
                    label_key: "opt_rename_dry_run".into(),
                    hint: "Preview the exact before → after names without writing files yet."
                        .into(),
                    hint_key: "opt_rename_dry_run_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(false),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_renamed".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let prefix = ctx.options.get_string_or("prefix", "");
        let suffix = ctx.options.get_string_or("suffix", "");
        let find_text = ctx.options.get_string_or("find_text", "");
        let replace_text = ctx.options.get_string_or("replace_text", "");
        let case_mode = ctx.options.get_string_or("case_mode", "keep");
        let numbering = ctx.options.get_bool("add_numbering", false);
        let dry_run = ctx.options.get_bool("dry_run", false);

        // Place renamed copies inside user's chosen output folder or a safe `<source_dir>/renamed_files` directory
        let target_dir = match &ctx.output_dir {
            Some(dir) => dir.clone(),
            None => ctx.output_dir_for(&ctx.inputs[0], "_renamed_batch"),
        };

        if !dry_run {
            fs::create_dir_all(&target_dir).map_err(|e| {
                ToolError::failed(
                    "Cannot create output folder for renamed files",
                    Some(e.to_string()),
                )
            })?;
        }

        let mut mappings = Vec::with_capacity(ctx.inputs.len());
        let mut outputs = Vec::new();

        for (idx, src) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                idx as f32 / ctx.inputs.len() as f32,
                format!("Processing file {} of {}", idx + 1, ctx.inputs.len()),
            );

            let orig_name = src
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string();
            let mut stem = src
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string();
            let ext = src
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            if !find_text.is_empty() {
                stem = stem.replace(&find_text, &replace_text);
            }
            stem = match case_mode.as_str() {
                "lowercase" => stem.to_lowercase(),
                "uppercase" => stem.to_uppercase(),
                "snake_case" => stem.trim().replace([' ', '-'], "_").to_lowercase(),
                _ => stem,
            };

            let seq_part = if numbering {
                format!("_{:03}", idx + 1)
            } else {
                String::new()
            };
            let new_stem = sanitize_filename(&format!("{prefix}{stem}{suffix}{seq_part}"));
            let desired_file = if ext.is_empty() {
                target_dir.join(&new_stem)
            } else {
                target_dir.join(format!("{new_stem}.{ext}"))
            };

            let final_dest = if !dry_run && (desired_file.exists() || &desired_file == src) {
                resolve_safe_output_path(
                    &desired_file,
                    Some(&target_dir),
                    "",
                    (!ext.is_empty()).then_some(ext.as_str()),
                )
            } else {
                desired_file
            };

            let final_name = final_dest
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&new_stem)
                .to_string();

            if !dry_run {
                let bytes = stream_copy_file(src, &final_dest, &ctx.cancel)?;
                outputs.push(OutputArtifact {
                    path: final_dest.display().to_string(),
                    label: format!("{orig_name} → {final_name}"),
                    size_bytes: bytes,
                    original_size_bytes: Some(bytes),
                });
            }

            mappings.push(json!({
                "from": orig_name,
                "to": final_name,
                "output_path": final_dest.display().to_string(),
            }));
        }

        ctx.report_progress(1.0, "Completed");
        let summary = if dry_run {
            format!("Dry-run preview generated for {} file(s).", mappings.len())
        } else {
            format!(
                "Created {} renamed file copy/copies in '{}' (originals untouched).",
                outputs.len(),
                target_dir.display()
            )
        };

        Ok(ToolRunResult {
            summary,
            outputs,
            warnings: Vec::new(),
            report_json: Some(json!({
                "dry_run": dry_run,
                "target_directory": target_dir.display().to_string(),
                "mappings": mappings,
            })),
        })
    }
}

fn sanitize_filename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "renamed".to_string()
    } else {
        trimmed.to_string()
    }
}

fn stream_copy_file(src: &Path, dst: &Path, cancel: &CancelHandle) -> Result<u64, ToolError> {
    let mut reader = BufReader::new(
        File::open(src)
            .map_err(|e| ToolError::invalid(format!("Cannot open '{}': {e}", src.display())))?,
    );
    let mut writer =
        std::io::BufWriter::new(File::create(dst).map_err(|e| {
            ToolError::failed("Cannot create destination file", Some(e.to_string()))
        })?);
    let mut buf = [0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        cancel.check()?;
        let n = reader
            .read(&mut buf)
            .map_err(|e| ToolError::failed("Read error while copying", Some(e.to_string())))?;
        if n == 0 {
            break;
        }
        use std::io::Write;
        writer
            .write_all(&buf[..n])
            .map_err(|e| ToolError::failed("Write error while copying", Some(e.to_string())))?;
        copied += n as u64;
    }
    Ok(copied)
}

// ============================================================================
// 2. File Hash / Checksum (`util.checksum`) — 64 KiB Streaming!
// ============================================================================
pub struct FileChecksumTool;

impl Tool for FileChecksumTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "util.checksum".into(),
            name: "File Hash / Checksum".into(),
            name_key: "tool_file_checksum".into(),
            description:
                "Compute or verify SHA-256, BLAKE3, SHA-512, SHA-1, and MD5 checksums for files of any size."
                    .into(),
            description_key: "tool_file_checksum_desc".into(),
            category: ToolCategory::Files,
            icon: "hash".into(),
            keywords: vec![
                "sha256".into(),
                "blake3".into(),
                "md5".into(),
                "sha512".into(),
                "sha1".into(),
                "verify".into(),
                "digest".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(&[]),
            options_schema: vec![
                OptionField {
                    key: "algorithm".into(),
                    label: "Hash Algorithm".into(),
                    label_key: "opt_hash_algo".into(),
                    hint: "SHA-256 is standard for downloads; BLAKE3 is ultra-fast; All computes every digest in one pass."
                        .into(),
                    hint_key: "opt_hash_algo_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "sha256".into(),
                                label: "SHA-256 (Standard)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "blake3".into(),
                                label: "BLAKE3 (Fastest)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "sha512".into(),
                                label: "SHA-512".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "sha1".into(),
                                label: "SHA-1 (Legacy)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "md5".into(),
                                label: "MD5 (Legacy)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "all".into(),
                                label: "All Algorithms (Single Streaming Pass)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("sha256"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "expected_hash".into(),
                    label: "Expected Checksum (Optional Verification)".into(),
                    label_key: "opt_expected_hash".into(),
                    hint: "Paste a published checksum here to verify whether your file matches."
                        .into(),
                    hint_key: "opt_expected_hash_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("Optional hex digest to compare against...".into()),
                        secret: false,
                    },
                    default_value: json!(""),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: ".sha256".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let algo = ctx.options.get_string_or("algorithm", "sha256");
        let expected = ctx
            .options
            .get_string_or("expected_hash", "")
            .trim()
            .to_ascii_lowercase();

        let mut records = Vec::with_capacity(ctx.inputs.len());
        let mut mismatch_found = false;

        for (i, path) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            let file_len = fs::metadata(path)
                .map_err(|e| ToolError::invalid(format!("Cannot stat '{}': {e}", path.display())))?
                .len();

            let mut file = File::open(path).map_err(|e| {
                ToolError::invalid(format!("Cannot open '{}': {e}", path.display()))
            })?;

            let mut sha256 = (algo == "sha256" || algo == "all").then(Sha256::new);
            let mut sha512 = (algo == "sha512" || algo == "all").then(Sha512::new);
            let mut sha1 = (algo == "sha1" || algo == "all").then(Sha1::new);
            let mut md5 = (algo == "md5" || algo == "all").then(Md5::new);
            let mut b3 = (algo == "blake3" || algo == "all").then(blake3::Hasher::new);

            let mut buf = [0u8; 64 * 1024];
            let mut read_so_far = 0u64;
            loop {
                ctx.check_cancelled()?;
                let n = file
                    .read(&mut buf)
                    .map_err(|e| ToolError::failed("Disk read error", Some(e.to_string())))?;
                if n == 0 {
                    break;
                }
                let slice = &buf[..n];
                if let Some(h) = sha256.as_mut() {
                    h.update(slice);
                }
                if let Some(h) = sha512.as_mut() {
                    h.update(slice);
                }
                if let Some(h) = sha1.as_mut() {
                    h.update(slice);
                }
                if let Some(h) = md5.as_mut() {
                    h.update(slice);
                }
                if let Some(h) = b3.as_mut() {
                    h.update(slice);
                }
                read_so_far += n as u64;
                if file_len > 0 && read_so_far % (4 * 1024 * 1024) < 65536 {
                    let file_frac = read_so_far as f32 / file_len as f32;
                    let overall = (i as f32 + file_frac) / ctx.inputs.len() as f32;
                    ctx.report_progress(
                        overall,
                        format!("Hashing file {} of {}", i + 1, ctx.inputs.len()),
                    );
                }
            }

            let mut digests = serde_json::Map::new();
            if let Some(h) = sha256 {
                digests.insert("sha256".into(), json!(format!("{:x}", h.finalize())));
            }
            if let Some(h) = b3 {
                digests.insert("blake3".into(), json!(h.finalize().to_hex().to_string()));
            }
            if let Some(h) = sha512 {
                digests.insert("sha512".into(), json!(format!("{:x}", h.finalize())));
            }
            if let Some(h) = sha1 {
                digests.insert("sha1".into(), json!(format!("{:x}", h.finalize())));
            }
            if let Some(h) = md5 {
                digests.insert("md5".into(), json!(format!("{:x}", h.finalize())));
            }

            let matched = if !expected.is_empty() {
                let any_match = digests
                    .values()
                    .filter_map(|v| v.as_str())
                    .any(|d| d.eq_ignore_ascii_case(&expected));
                if !any_match {
                    mismatch_found = true;
                }
                Some(any_match)
            } else {
                None
            };

            records.push(json!({
                "file": path.file_name().and_then(|s| s.to_str()).unwrap_or("file"),
                "path": path.display().to_string(),
                "size_bytes": file_len,
                "hashes": digests,
                "verified_match": matched,
            }));
        }

        ctx.report_progress(1.0, "Completed");
        let summary = if !expected.is_empty() {
            if mismatch_found {
                "Checksum MISMATCH — at least one file did NOT match the expected digest!".into()
            } else {
                "Checksum VERIFIED — file digest matches expected checksum!".into()
            }
        } else if records.len() == 1 && algo != "all" {
            let single_hash = records[0]["hashes"][&algo].as_str().unwrap_or("");
            format!("{}: {single_hash}", algo.to_uppercase())
        } else {
            format!("Computed checksums for {} file(s).", records.len())
        };

        let warnings = if mismatch_found {
            vec!["Expected checksum did NOT match the computed file digest.".to_string()]
        } else {
            Vec::new()
        };

        Ok(ToolRunResult {
            summary,
            outputs: Vec::new(),
            warnings,
            report_json: Some(json!({ "files": records })),
        })
    }
}

// ============================================================================
// 3. Duplicate File Finder (`util.duplicate_finder`) — 3-Stage Streaming!
// ============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub blake3_hash: String,
    pub file_size_bytes: u64,
    pub wasted_bytes: u64,
    pub files: Vec<String>,
}

pub struct DuplicateFinderTool;

impl Tool for DuplicateFinderTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "util.duplicate_finder".into(),
            name: "Duplicate Finder".into(),
            name_key: "tool_duplicate_finder".into(),
            description:
                "Scan folders or file lists to find exact byte-identical duplicates quickly (size filter → partial hash → full BLAKE3)."
                    .into(),
            description_key: "tool_duplicate_finder_desc".into(),
            category: ToolCategory::Files,
            icon: "duplicate".into(),
            keywords: vec![
                "duplicates".into(),
                "dedupe".into(),
                "clean".into(),
                "identical".into(),
                "space".into(),
            ],
            input_spec: ToolInputSpec {
                accepted_extensions: vec![],
                min_files: 1,
                max_files: None,
                allow_directories: true,
            },
            options_schema: vec![OptionField {
                key: "min_size_bytes".into(),
                label: "Ignore Files Smaller Than".into(),
                label_key: "opt_dup_min_size".into(),
                hint: "Set to 1 byte to include tiny files, or 1024 to skip empty/tiny placeholders."
                    .into(),
                hint_key: "opt_dup_min_size_hint".into(),
                kind: OptionKind::Integer {
                    min: Some(1),
                    max: Some(100_000_000),
                    step: Some(1024),
                    unit: Some("bytes".into()),
                },
                default_value: json!(1),
                required: false,
                visible_when: None,
            }],
            default_suffix: "_duplicates".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let min_size = ctx.options.get_i64("min_size_bytes", 1).max(1) as u64;

        ctx.report_progress(0.1, "Scanning files and grouping by byte size...");
        let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        let mut scanned_files = 0usize;

        for root in &ctx.inputs {
            ctx.check_cancelled()?;
            if root.is_file() {
                if let Ok(m) = fs::metadata(root) {
                    if m.len() >= min_size {
                        scanned_files += 1;
                        by_size.entry(m.len()).or_default().push(root.clone());
                    }
                }
            } else if root.is_dir() {
                for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
                    if entry.file_type().is_file() {
                        if let Ok(m) = entry.metadata() {
                            if m.len() >= min_size {
                                scanned_files += 1;
                                by_size
                                    .entry(m.len())
                                    .or_default()
                                    .push(entry.path().to_path_buf());
                            }
                        }
                    }
                }
            }
        }

        // Stage 1: Keep only size buckets with >= 2 files
        let size_candidates: Vec<(u64, Vec<PathBuf>)> = by_size
            .into_iter()
            .filter(|(_, paths)| paths.len() >= 2)
            .collect();

        // Stage 2: Fast 8 KiB head + 8 KiB tail pre-hash
        ctx.report_progress(0.45, "Computing fast head/tail pre-hashes...");
        let mut full_hash_candidates: Vec<(u64, Vec<PathBuf>)> = Vec::new();
        for (sz, group) in size_candidates {
            ctx.check_cancelled()?;
            if sz <= 16 * 1024 {
                full_hash_candidates.push((sz, group));
                continue;
            }
            let mut by_prehash: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
            for p in group {
                if let Ok(ph) = head_tail_blake3(&p, sz) {
                    by_prehash.entry(ph).or_default().push(p);
                }
            }
            for (_, sub) in by_prehash {
                if sub.len() >= 2 {
                    full_hash_candidates.push((sz, sub));
                }
            }
        }

        // Stage 3: Full streamed 64 KiB BLAKE3 verification
        ctx.report_progress(0.75, "Verifying exact full-file BLAKE3 hashes...");
        let mut duplicate_groups: Vec<DuplicateGroup> = Vec::new();
        let total_buckets = full_hash_candidates.len().max(1);

        for (b_idx, (sz, group)) in full_hash_candidates.into_iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                0.75 + 0.24 * (b_idx as f32 / total_buckets as f32),
                format!("Verifying candidate group {}/{total_buckets}", b_idx + 1),
            );
            let mut by_full: HashMap<String, Vec<String>> = HashMap::new();
            for p in group {
                ctx.check_cancelled()?;
                if let Ok(hex) = full_streamed_blake3(&p, &ctx.cancel) {
                    by_full
                        .entry(hex)
                        .or_default()
                        .push(p.display().to_string());
                }
            }
            for (hash, mut paths) in by_full {
                if paths.len() >= 2 {
                    paths.sort();
                    let wasted = sz * (paths.len() as u64 - 1);
                    duplicate_groups.push(DuplicateGroup {
                        blake3_hash: hash,
                        file_size_bytes: sz,
                        wasted_bytes: wasted,
                        files: paths,
                    });
                }
            }
        }

        duplicate_groups.sort_by_key(|a| std::cmp::Reverse(a.wasted_bytes));
        let total_wasted: u64 = duplicate_groups.iter().map(|g| g.wasted_bytes).sum();

        ctx.report_progress(1.0, "Completed");
        let summary = if duplicate_groups.is_empty() {
            format!("Scanned {scanned_files} file(s) — no duplicates found!")
        } else {
            format!(
                "Found {} duplicate group(s) across {scanned_files} scanned files ({total_wasted} bytes reclaimable).",
                duplicate_groups.len()
            )
        };

        Ok(ToolRunResult {
            summary,
            outputs: Vec::new(),
            warnings: Vec::new(),
            report_json: Some(json!({
                "scanned_files": scanned_files,
                "duplicate_group_count": duplicate_groups.len(),
                "total_reclaimable_bytes": total_wasted,
                "groups": duplicate_groups,
            })),
        })
    }
}

fn head_tail_blake3(path: &Path, size: u64) -> std::io::Result<[u8; 32]> {
    let mut f = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut head = [0u8; 8192];
    let n = f.read(&mut head)?;
    hasher.update(&head[..n]);
    if size > 8192 {
        f.seek(SeekFrom::End(-8192))?;
        let mut tail = [0u8; 8192];
        let n2 = f.read(&mut tail)?;
        hasher.update(&tail[..n2]);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn full_streamed_blake3(path: &Path, cancel: &CancelHandle) -> Result<String, ToolError> {
    let mut f = File::open(path)
        .map_err(|e| ToolError::invalid(format!("Cannot open '{}': {e}", path.display())))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        cancel.check()?;
        let n = f
            .read(&mut buf)
            .map_err(|e| ToolError::failed("Disk read error", Some(e.to_string())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
