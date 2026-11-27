use crate::engine::{create_archive_streaming, extract_archive_streaming, list_archive_entries};
use core_jobs::{
    OptionField, OptionKind, OutputArtifact, SelectChoice, Tool, ToolCategory, ToolContext,
    ToolDescriptor, ToolError, ToolInputSpec, ToolRegistry, ToolRunResult,
};
use serde_json::json;
use std::fs;

pub const ARCHIVE_EXTENSIONS: &[&str] =
    &["zip", "gz", "tgz", "7z", "zst", "tzst", "xz", "txz", "tar"];

pub fn register_archive_tools(registry: &ToolRegistry) {
    registry.register(ArchiveCreateTool);
    registry.register(ArchiveExtractTool);
    registry.register(ArchiveInspectTool);
}

// ============================================================================
// 1. Create Archive (`archive.create`)
// ============================================================================
pub struct ArchiveCreateTool;

impl Tool for ArchiveCreateTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "archive.create".into(),
            name: "Create Archive".into(),
            name_key: "tool_archive_create".into(),
            description:
                "Pack files or folders into a compressed .zip, .tar.gz, .7z, .tar.zst, or .tar.xz archive."
                    .into(),
            description_key: "tool_archive_create_desc".into(),
            category: ToolCategory::Files,
            icon: "archive".into(),
            keywords: vec![
                "zip".into(),
                "7z".into(),
                "tar".into(),
                "zstd".into(),
                "xz".into(),
                "compress".into(),
            ],
            input_spec: ToolInputSpec {
                accepted_extensions: vec![],
                min_files: 1,
                max_files: None,
                allow_directories: true,
            },
            options_schema: vec![
                OptionField {
                    key: "format".into(),
                    label: "Archive Format".into(),
                    label_key: "opt_archive_format".into(),
                    hint: "ZIP opens everywhere; Zstd (.tar.zst) is fastest; 7z and .tar.xz compress smallest."
                        .into(),
                    hint_key: "opt_archive_format_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "zip".into(),
                                label: "ZIP (.zip — Universal)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "tar.gz".into(),
                                label: "TAR.GZ (.tar.gz — Standard Linux/macOS)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "7z".into(),
                                label: "7-Zip (.7z — High Compression)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "tar.zst".into(),
                                label: "TAR.ZST (.tar.zst — Zstandard Ultra-Fast)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "tar.xz".into(),
                                label: "TAR.XZ (.tar.xz — Maximum LZMA2)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("zip"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "level".into(),
                    label: "Compression Level".into(),
                    label_key: "opt_archive_level".into(),
                    hint: "6 is balanced; 9 maximizes compression at slower speed.".into(),
                    hint_key: "opt_archive_level_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(1),
                        max: Some(9),
                        step: Some(1),
                        unit: None,
                    },
                    default_value: json!(6),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_archive".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let fmt = ctx.options.get_string_or("format", "zip");
        let level = ctx.options.get_i64("level", 6).clamp(1, 9) as u32;

        let out_path = ctx.output_path_for(&ctx.inputs[0], "_archive", Some(&fmt));
        let written = create_archive_streaming(
            &ctx.inputs,
            &fmt,
            &out_path,
            level,
            &ctx.cancel,
            |frac, msg| ctx.report_progress(frac, msg),
        )?;

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Created {} archive.", fmt.to_uppercase()),
            vec![OutputArtifact {
                path: out_path.display().to_string(),
                label: out_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("archive")
                    .to_string(),
                size_bytes: written,
                original_size_bytes: None,
            }],
        ))
    }
}

// ============================================================================
// 2. Extract Archive (`archive.extract`)
// ============================================================================
pub struct ArchiveExtractTool;

impl Tool for ArchiveExtractTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "archive.extract".into(),
            name: "Extract Archive".into(),
            name_key: "tool_archive_extract".into(),
            description:
                "Safely unpack .zip, .tar.gz, .7z, .tar.zst, .zst, .tar.xz, and .xz archives into a new folder."
                    .into(),
            description_key: "tool_archive_extract_desc".into(),
            category: ToolCategory::Files,
            icon: "unarchive".into(),
            keywords: vec![
                "unzip".into(),
                "unpack".into(),
                "decompress".into(),
                "7z".into(),
                "tar".into(),
                "zstd".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(ARCHIVE_EXTENSIONS),
            options_schema: vec![],
            default_suffix: "_extracted".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        let mut total_files = 0usize;

        for input in &ctx.inputs {
            ctx.check_cancelled()?;
            let out_dir = ctx.output_dir_for(input, "_extracted");
            let (count, bytes) =
                extract_archive_streaming(input, &out_dir, &ctx.cancel, |frac, msg| {
                    ctx.report_progress(frac, msg)
                })?;
            total_files += count;
            let orig_size = fs::metadata(input).map(|m| m.len()).ok();
            outputs.push(OutputArtifact {
                path: out_dir.display().to_string(),
                label: format!(
                    "{} ({count} files)",
                    out_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("extracted")
                ),
                size_bytes: bytes,
                original_size_bytes: orig_size,
            });
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Extracted {total_files} file(s) safely."),
            outputs,
        ))
    }
}

// ============================================================================
// 3. Archive Preview / List Contents (`archive.inspect`)
// ============================================================================
pub struct ArchiveInspectTool;

impl Tool for ArchiveInspectTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "archive.inspect".into(),
            name: "Inspect Archive Contents".into(),
            name_key: "tool_archive_inspect".into(),
            description:
                "List all files and uncompressed sizes inside an archive without extracting.".into(),
            description_key: "tool_archive_inspect_desc".into(),
            category: ToolCategory::Files,
            icon: "archive".into(),
            keywords: vec![
                "list".into(),
                "preview".into(),
                "contents".into(),
                "zip".into(),
            ],
            input_spec: ToolInputSpec::single_file(ARCHIVE_EXTENSIONS),
            options_schema: vec![],
            default_suffix: "_listing".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let input = &ctx.inputs[0];
        ctx.report_progress(0.3, "Reading archive directory...");
        let entries = list_archive_entries(input, 2000)?;
        let total_uncompressed: u64 = entries.iter().map(|e| e.uncompressed_bytes).sum();
        ctx.report_progress(1.0, "Completed");

        Ok(ToolRunResult {
            summary: format!(
                "Listed {} entries ({} bytes uncompressed).",
                entries.len(),
                total_uncompressed
            ),
            outputs: Vec::new(),
            warnings: Vec::new(),
            report_json: Some(json!({
                "entry_count": entries.len(),
                "total_uncompressed_bytes": total_uncompressed,
                "entries": entries,
            })),
        })
    }
}
