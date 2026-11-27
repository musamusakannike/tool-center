use crate::ops::{
    build_subset_or_reordered_pdf, compress_pdf_file, encrypt_pdf_aes256, images_to_pdf_file,
    load_lopdf_doc, merge_lopdf_documents, parse_page_selection, rotate_pdf_pages, save_pdf_clean,
    unlock_pdf_file, watermark_pdf_pages,
};
use crate::preview::bind_pdfium;
use core_image::{encode_image_to_path, normalize_output_extension, IMAGE_INPUT_EXTENSIONS};
use core_jobs::{
    OptionField, OptionKind, OutputArtifact, SelectChoice, Tool, ToolCategory, ToolContext,
    ToolDescriptor, ToolError, ToolInputSpec, ToolOptions, ToolRegistry, ToolRunResult,
    VisibilityCondition,
};
use pdfium_render::prelude::*;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

/// Registers all Milestone 5 PDF Tools into the shared `ToolRegistry`.
pub fn register_pdf_tools(registry: &ToolRegistry) {
    registry.register(PdfMergeTool);
    registry.register(PdfSplitTool);
    registry.register(PdfExtractTool);
    registry.register(PdfReorderTool);
    registry.register(PdfRotateTool);
    registry.register(PdfCompressTool);
    registry.register(PdfToImagesTool);
    registry.register(ImagesToPdfTool);
    registry.register(PdfWatermarkTool);
    registry.register(PdfProtectTool);
    registry.register(PdfUnlockTool);
}

fn pdf_artifact(source: Option<&Path>, out_path: &Path, size_bytes: u64) -> OutputArtifact {
    let orig = source.and_then(|p| fs::metadata(p).map(|m| m.len()).ok());
    OutputArtifact {
        path: out_path.display().to_string(),
        label: out_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output.pdf")
            .to_string(),
        size_bytes,
        original_size_bytes: orig,
    }
}

// ============================================================================
// 1. Merge PDF (`pdf.merge`)
// ============================================================================
pub struct PdfMergeTool;

impl Tool for PdfMergeTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.merge".into(),
            name: "Merge PDFs".into(),
            name_key: "tool_pdf_merge".into(),
            description: "Combine two or more PDF files into one document in your chosen order."
                .into(),
            description_key: "tool_pdf_merge_desc".into(),
            category: ToolCategory::Pdf,
            icon: "pdf_merge".into(),
            keywords: vec![
                "combine".into(),
                "join".into(),
                "append".into(),
                "pdf".into(),
            ],
            input_spec: ToolInputSpec::multi_minimum(&["pdf"], 2),
            options_schema: vec![],
            default_suffix: "_merged".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let mut docs = Vec::with_capacity(ctx.inputs.len());
        let mut total_orig_bytes = 0u64;

        for (i, path) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                (i as f32 / (ctx.inputs.len() + 1) as f32) * 0.85,
                format!("Loading PDF {} of {}", i + 1, ctx.inputs.len()),
            );
            total_orig_bytes += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            docs.push(load_lopdf_doc(path, None)?);
        }

        ctx.check_cancelled()?;
        ctx.report_progress(0.9, "Merging page trees...");
        let mut merged = merge_lopdf_documents(docs)?;
        let out_path = ctx.output_path_for(&ctx.inputs[0], "_merged", Some("pdf"));
        let written = save_pdf_clean(&mut merged, &out_path)?;

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Merged {} PDF files.", ctx.inputs.len()),
            vec![OutputArtifact {
                path: out_path.display().to_string(),
                label: out_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("merged.pdf")
                    .to_string(),
                size_bytes: written,
                original_size_bytes: Some(total_orig_bytes),
            }],
        ))
    }
}

// ============================================================================
// 2. Split PDF (`pdf.split`)
// ============================================================================
pub struct PdfSplitTool;

impl Tool for PdfSplitTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.split".into(),
            name: "Split PDF".into(),
            name_key: "tool_pdf_split".into(),
            description: "Split a PDF into single pages, fixed-size chunks, or custom page ranges."
                .into(),
            description_key: "tool_pdf_split_desc".into(),
            category: ToolCategory::Pdf,
            icon: "pdf_split".into(),
            keywords: vec![
                "burst".into(),
                "separate".into(),
                "chunks".into(),
                "divide".into(),
            ],
            input_spec: ToolInputSpec::single_file(&["pdf"]),
            options_schema: vec![
                OptionField {
                    key: "mode".into(),
                    label: "Split Method".into(),
                    label_key: "opt_pdf_split_mode".into(),
                    hint: "Choose how the document should be divided into separate files.".into(),
                    hint_key: "opt_pdf_split_mode_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "every_page".into(),
                                label: "One PDF per page".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "every_n".into(),
                                label: "Every N pages".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "ranges".into(),
                                label: "Custom page ranges".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("every_page"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "pages_per_file".into(),
                    label: "Pages per Output File".into(),
                    label_key: "opt_pages_per_file".into(),
                    hint: "Number of pages placed into each split PDF.".into(),
                    hint_key: "opt_pages_per_file_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(1),
                        max: Some(1000),
                        step: Some(1),
                        unit: Some("pages".into()),
                    },
                    default_value: json!(2),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("every_n"),
                    }),
                },
                OptionField {
                    key: "ranges".into(),
                    label: "Page Ranges (Semicolon-separated)".into(),
                    label_key: "opt_split_ranges".into(),
                    hint: "Example: 1-3; 4-6; 7-10 creates three separate PDF files.".into(),
                    hint_key: "opt_split_ranges_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("1-3; 4-6".into()),
                        secret: false,
                    },
                    default_value: json!("1-2; 3-4"),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("ranges"),
                    }),
                },
            ],
            default_suffix: "_split".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let input = &ctx.inputs[0];
        let doc = load_lopdf_doc(input, None)?;
        let total = doc.get_pages().len() as u32;
        if total == 0 {
            return Err(ToolError::invalid("PDF document has 0 pages."));
        }

        let mode = ctx.options.get_string_or("mode", "every_page");
        let groups: Vec<Vec<u32>> = match mode.as_str() {
            "every_n" => {
                let step = ctx.options.get_i64("pages_per_file", 2).max(1) as usize;
                let all: Vec<u32> = (1..=total).collect();
                all.chunks(step).map(|c| c.to_vec()).collect()
            }
            "ranges" => {
                let raw = ctx.options.get_string_or("ranges", "1");
                let mut parsed = Vec::new();
                for seg in raw.split(';') {
                    if seg.trim().is_empty() {
                        continue;
                    }
                    parsed.push(parse_page_selection(seg, total)?);
                }
                if parsed.is_empty() {
                    return Err(ToolError::invalid("Please enter at least one page range."));
                }
                parsed
            }
            _ => (1..=total).map(|p| vec![p]).collect(),
        };

        let mut outputs = Vec::with_capacity(groups.len());
        for (idx, pages) in groups.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                idx as f32 / groups.len() as f32,
                format!("Writing split file {} of {}", idx + 1, groups.len()),
            );
            let suffix = format!("_part_{:03}", idx + 1);
            let out_path = ctx.output_path_for(input, &suffix, Some("pdf"));
            let written = build_subset_or_reordered_pdf(input, pages, &out_path)?;
            outputs.push(pdf_artifact(None, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Split PDF into {} file(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 3. Extract PDF Pages (`pdf.extract`)
// ============================================================================
pub struct PdfExtractTool;

impl Tool for PdfExtractTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.extract".into(),
            name: "Extract PDF Pages".into(),
            name_key: "tool_pdf_extract".into(),
            description: "Save chosen pages or page ranges into a new standalone PDF.".into(),
            description_key: "tool_pdf_extract_desc".into(),
            category: ToolCategory::Pdf,
            icon: "pdf_extract".into(),
            keywords: vec!["select".into(), "subset".into(), "pages".into()],
            input_spec: ToolInputSpec::single_file(&["pdf"]),
            options_schema: vec![OptionField {
                key: "pages".into(),
                label: "Pages to Extract".into(),
                label_key: "opt_extract_pages".into(),
                hint: "Examples: 1, 3, 5-8 or 2-4.".into(),
                hint_key: "opt_extract_pages_hint".into(),
                kind: OptionKind::Text {
                    placeholder: Some("1, 3-5".into()),
                    secret: false,
                },
                default_value: json!("1"),
                required: true,
                visible_when: None,
            }],
            default_suffix: "_extracted".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let input = &ctx.inputs[0];
        let doc = load_lopdf_doc(input, None)?;
        let total = doc.get_pages().len() as u32;
        let spec = ctx.options.get_string_or("pages", "1");
        let pages = parse_page_selection(&spec, total)?;

        ctx.report_progress(0.4, "Extracting selected pages...");
        let out_path = ctx.output_path_for(input, "_extracted", Some("pdf"));
        let written = build_subset_or_reordered_pdf(input, &pages, &out_path)?;

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Extracted {} page(s).", pages.len()),
            vec![pdf_artifact(Some(input), &out_path, written)],
        ))
    }
}

// ============================================================================
// 4. Reorder PDF Pages (`pdf.reorder`)
// ============================================================================
pub struct PdfReorderTool;

impl Tool for PdfReorderTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.reorder".into(),
            name: "Reorder PDF Pages".into(),
            name_key: "tool_pdf_reorder".into(),
            description: "Rearrange or reverse pages inside a PDF document.".into(),
            description_key: "tool_pdf_reorder_desc".into(),
            category: ToolCategory::Pdf,
            icon: "pdf_reorder".into(),
            keywords: vec!["sort".into(), "reverse".into(), "arrange".into(), "order".into()],
            input_spec: ToolInputSpec::single_file(&["pdf"]),
            options_schema: vec![OptionField {
                key: "order".into(),
                label: "New Page Order".into(),
                label_key: "opt_page_order".into(),
                hint: "Enter page numbers in your desired order (e.g. '3, 1, 2, 4-6') or type 'reverse' to flip the whole document.".into(),
                hint_key: "opt_page_order_hint".into(),
                kind: OptionKind::Text {
                    placeholder: Some("reverse  or  2, 1, 3-5".into()),
                    secret: false,
                },
                default_value: json!("reverse"),
                required: true,
                visible_when: None,
            }],
            default_suffix: "_reordered".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let input = &ctx.inputs[0];
        let doc = load_lopdf_doc(input, None)?;
        let total = doc.get_pages().len() as u32;
        let order_spec = ctx.options.get_string_or("order", "reverse");
        let pages = parse_page_selection(&order_spec, total)?;

        ctx.report_progress(0.5, "Reordering pages...");
        let out_path = ctx.output_path_for(input, "_reordered", Some("pdf"));
        let written = build_subset_or_reordered_pdf(input, &pages, &out_path)?;

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Saved PDF with {} reordered page(s).", pages.len()),
            vec![pdf_artifact(Some(input), &out_path, written)],
        ))
    }
}

// ============================================================================
// 5. Rotate PDF (`pdf.rotate`)
// ============================================================================
pub struct PdfRotateTool;

impl Tool for PdfRotateTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.rotate".into(),
            name: "Rotate PDF".into(),
            name_key: "tool_pdf_rotate".into(),
            description: "Rotate all pages or chosen page ranges by 90°, 180°, or 270°.".into(),
            description_key: "tool_pdf_rotate_desc".into(),
            category: ToolCategory::Pdf,
            icon: "pdf_rotate".into(),
            keywords: vec!["orientation".into(), "landscape".into(), "turn".into()],
            input_spec: ToolInputSpec::single_or_batch(&["pdf"]),
            options_schema: vec![
                OptionField {
                    key: "angle".into(),
                    label: "Rotation".into(),
                    label_key: "opt_pdf_angle".into(),
                    hint: "Clockwise rotation applied to selected pages.".into(),
                    hint_key: "opt_pdf_angle_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "90".into(),
                                label: "90° Clockwise".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "180".into(),
                                label: "180° Upside Down".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "270".into(),
                                label: "90° Counter-Clockwise (270°)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("90"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "pages".into(),
                    label: "Pages to Rotate".into(),
                    label_key: "opt_rotate_pages".into(),
                    hint: "Use 'all' for every page, or enter specific pages like '1, 3-5'.".into(),
                    hint_key: "opt_rotate_pages_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("all".into()),
                        secret: false,
                    },
                    default_value: json!("all"),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_rotated".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let angle: i64 = ctx
            .options
            .get_string_or("angle", "90")
            .parse()
            .unwrap_or(90);
        let pages = ctx.options.get_string_or("pages", "all");

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Rotating PDF {} of {}", i + 1, ctx.inputs.len()),
            );
            let out_path = ctx.output_path_for(input, "_rotated", Some("pdf"));
            let written = rotate_pdf_pages(input, &pages, angle, &out_path)?;
            outputs.push(pdf_artifact(Some(input), &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Rotated {} PDF file(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 6. Compress PDF (`pdf.compress`)
// ============================================================================
pub struct PdfCompressTool;

impl Tool for PdfCompressTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.compress".into(),
            name: "Compress PDF".into(),
            name_key: "tool_pdf_compress".into(),
            description:
                "Shrink PDF size by pruning unused objects and applying maximum stream & object-stream compression."
                    .into(),
            description_key: "tool_pdf_compress_desc".into(),
            category: ToolCategory::Pdf,
            icon: "compress".into(),
            keywords: vec!["shrink".into(), "optimize".into(), "size".into(), "reduce".into()],
            input_spec: ToolInputSpec::single_or_batch(&["pdf"]),
            options_schema: vec![],
            default_suffix: "_compressed".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Compressing PDF {} of {}", i + 1, ctx.inputs.len()),
            );
            let out_path = ctx.output_path_for(input, "_compressed", Some("pdf"));
            let written = compress_pdf_file(input, &out_path)?;
            outputs.push(pdf_artifact(Some(input), &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Compressed {} PDF file(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 7. PDF Pages to Images (`pdf.to_images`)
// ============================================================================
pub struct PdfToImagesTool;

impl Tool for PdfToImagesTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.to_images".into(),
            name: "PDF to Images".into(),
            name_key: "tool_pdf_to_images".into(),
            description: "Render each PDF page into a crisp PNG, JPG, or WebP image via PDFium."
                .into(),
            description_key: "tool_pdf_to_images_desc".into(),
            category: ToolCategory::Pdf,
            icon: "pdf_images".into(),
            keywords: vec![
                "rasterize".into(),
                "png".into(),
                "jpg".into(),
                "export".into(),
                "pages".into(),
            ],
            input_spec: ToolInputSpec::single_file(&["pdf"]),
            options_schema: vec![
                OptionField {
                    key: "format".into(),
                    label: "Image Format".into(),
                    label_key: "opt_pdf_img_format".into(),
                    hint: "PNG keeps sharp text edges; JPG creates smaller files.".into(),
                    hint_key: "opt_pdf_img_format_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "png".into(),
                                label: "PNG (Crisp Lossless)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "jpg".into(),
                                label: "JPG (Compact)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "webp".into(),
                                label: "WebP".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("png"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "dpi".into(),
                    label: "Resolution (DPI)".into(),
                    label_key: "opt_pdf_dpi".into(),
                    hint: "150 DPI is great for screens; 300 DPI is print-grade.".into(),
                    hint_key: "opt_pdf_dpi_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "150".into(),
                                label: "150 DPI (Standard Screen)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "300".into(),
                                label: "300 DPI (High Print Quality)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "96".into(),
                                label: "96 DPI (Fast Preview)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("150"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "pages".into(),
                    label: "Pages".into(),
                    label_key: "opt_pdf_img_pages".into(),
                    hint: "Use 'all' or enter specific pages like '1-5'.".into(),
                    hint_key: "opt_pdf_img_pages_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("all".into()),
                        secret: false,
                    },
                    default_value: json!("all"),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_page".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let input = &ctx.inputs[0];
        let fmt = normalize_output_extension(&ctx.options.get_string_or("format", "png"));
        let dpi: f32 = ctx
            .options
            .get_string_or("dpi", "150")
            .parse()
            .unwrap_or(150.0);
        let pages_spec = ctx.options.get_string_or("pages", "all");

        let pdfium = bind_pdfium()?;
        let doc = pdfium.load_pdf_from_file(input, None).map_err(|e| {
            ToolError::corrupt(
                input,
                "Could not open PDF via PDFium (unlock first if password-protected).",
                Some(e.to_string()),
            )
        })?;

        let total = doc.pages().len() as u32;
        let selected = parse_page_selection(&pages_spec, total)?;
        let mut outputs = Vec::with_capacity(selected.len());

        for (idx, &page_num) in selected.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                idx as f32 / selected.len() as f32,
                format!("Rendering page {page_num} ({}/{})", idx + 1, selected.len()),
            );

            let page = doc
                .pages()
                .get((page_num - 1) as i32)
                .map_err(|e| ToolError::failed("Failed to read PDF page", Some(e.to_string())))?;

            let width_pts = page.width().value.max(72.0);
            let target_px = ((width_pts / 72.0) * dpi).round().clamp(200.0, 6000.0) as i32;
            let cfg = PdfRenderConfig::new()
                .set_target_width(target_px)
                .render_form_data(true)
                .render_annotations(true);

            let dyn_img = page
                .render_with_config(&cfg)
                .and_then(|b| b.as_image())
                .map_err(|e| ToolError::failed("Failed to render PDF page", Some(e.to_string())))?;

            let suffix = format!("_page_{page_num:03}");
            let out_path = ctx.output_path_for(input, &suffix, Some(fmt));
            let written = encode_image_to_path(&dyn_img, &out_path, fmt, 88, false)?;
            outputs.push(pdf_artifact(None, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!(
                "Rendered {} PDF page(s) to {}.",
                outputs.len(),
                fmt.to_uppercase()
            ),
            outputs,
        ))
    }
}

// ============================================================================
// 8. Images to PDF (`pdf.from_images`)
// ============================================================================
pub struct ImagesToPdfTool;

impl Tool for ImagesToPdfTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.from_images".into(),
            name: "Images to PDF".into(),
            name_key: "tool_images_to_pdf".into(),
            description: "Combine one or more photos or scans into a clean multi-page PDF.".into(),
            description_key: "tool_images_to_pdf_desc".into(),
            category: ToolCategory::Pdf,
            icon: "images_pdf".into(),
            keywords: vec!["scan".into(), "jpg".into(), "png".into(), "combine".into()],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "page_size".into(),
                    label: "Page Size".into(),
                    label_key: "opt_img_pdf_size".into(),
                    hint: "Fit onto standard A4/Letter paper or size each page to match the image."
                        .into(),
                    hint_key: "opt_img_pdf_size_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "auto".into(),
                                label: "Match Image Dimensions".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "a4".into(),
                                label: "A4 Paper".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "letter".into(),
                                label: "US Letter".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("auto"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "margin".into(),
                    label: "Page Margin".into(),
                    label_key: "opt_img_pdf_margin".into(),
                    hint: "White border around each image in points (0 = edge-to-edge).".into(),
                    hint_key: "opt_img_pdf_margin_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(0),
                        max: Some(100),
                        step: Some(6),
                        unit: Some("pt".into()),
                    },
                    default_value: json!(18),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_document".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let page_size = ctx.options.get_string_or("page_size", "auto");
        let margin = ctx.options.get_i64("margin", 18).max(0) as f32;
        let out_path = ctx.output_path_for(&ctx.inputs[0], "_document", Some("pdf"));

        let written = images_to_pdf_file(
            &ctx.inputs,
            &page_size,
            margin,
            &out_path,
            &ctx.cancel,
            &|idx, total| {
                ctx.report_progress(
                    idx as f32 / total as f32,
                    format!("Embedding image {} of {total}", idx + 1),
                );
            },
        )?;

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Created PDF from {} image(s).", ctx.inputs.len()),
            vec![pdf_artifact(None, &out_path, written)],
        ))
    }
}

// ============================================================================
// 9. Add PDF Watermark (`pdf.watermark`)
// ============================================================================
pub struct PdfWatermarkTool;

impl Tool for PdfWatermarkTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.watermark".into(),
            name: "Watermark PDF".into(),
            name_key: "tool_pdf_watermark".into(),
            description: "Stamp translucent vector text across every page of a PDF.".into(),
            description_key: "tool_pdf_watermark_desc".into(),
            category: ToolCategory::Pdf,
            icon: "watermark".into(),
            keywords: vec!["stamp".into(), "draft".into(), "confidential".into()],
            input_spec: ToolInputSpec::single_or_batch(&["pdf"]),
            options_schema: vec![
                OptionField {
                    key: "text".into(),
                    label: "Watermark Text".into(),
                    label_key: "opt_pdf_wm_text".into(),
                    hint: "Text stamped on every page (e.g. DRAFT or CONFIDENTIAL).".into(),
                    hint_key: "opt_pdf_wm_text_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("CONFIDENTIAL".into()),
                        secret: false,
                    },
                    default_value: json!("CONFIDENTIAL"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "font_size".into(),
                    label: "Text Size".into(),
                    label_key: "opt_pdf_wm_size".into(),
                    hint: "Font size in points.".into(),
                    hint_key: "opt_pdf_wm_size_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(12),
                        max: Some(120),
                        step: Some(4),
                        unit: Some("pt".into()),
                    },
                    default_value: json!(48),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "opacity".into(),
                    label: "Opacity".into(),
                    label_key: "opt_pdf_wm_opacity".into(),
                    hint: "25–40% keeps underlying document text easy to read.".into(),
                    hint_key: "opt_pdf_wm_opacity_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(10),
                        max: Some(100),
                        step: Some(5),
                        unit: Some("%".into()),
                    },
                    default_value: json!(30),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "diagonal".into(),
                    label: "45° Diagonal Stamp".into(),
                    label_key: "opt_pdf_wm_diag".into(),
                    hint: "Angle the watermark diagonally across the page.".into(),
                    hint_key: "opt_pdf_wm_diag_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(true),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "position".into(),
                    label: "Placement".into(),
                    label_key: "opt_pdf_wm_pos".into(),
                    hint: "Page anchor position.".into(),
                    hint_key: "opt_pdf_wm_pos_hint".into(),
                    kind: OptionKind::PositionGrid,
                    default_value: json!("center"),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_watermarked".into(),
        }
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        let desc = self.descriptor();
        core_jobs::validate_inputs_against_spec(inputs, &desc.input_spec)?;
        if options.get_string_or("text", "").trim().is_empty() {
            return Err(ToolError::invalid("Please enter watermark text."));
        }
        Ok(())
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let text = ctx.options.get_string_or("text", "CONFIDENTIAL");
        let font_size = ctx.options.get_i64("font_size", 48).clamp(8, 200) as f32;
        let opacity = ctx.options.get_i64("opacity", 30).clamp(5, 100) as u32;
        let diagonal = ctx.options.get_bool("diagonal", true);
        let position = ctx.options.get_string_or("position", "center");

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Watermarking PDF {} of {}", i + 1, ctx.inputs.len()),
            );
            let out_path = ctx.output_path_for(input, "_watermarked", Some("pdf"));
            let written = watermark_pdf_pages(
                input, &text, font_size, opacity, diagonal, &position, &out_path,
            )?;
            outputs.push(pdf_artifact(Some(input), &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Watermarked {} PDF file(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 10. Protect PDF with Password (`pdf.protect`)
// ============================================================================
pub struct PdfProtectTool;

impl Tool for PdfProtectTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.protect".into(),
            name: "Protect PDF (Add Password)".into(),
            name_key: "tool_pdf_protect".into(),
            description: "Encrypt a PDF with strong AES-256 password protection.".into(),
            description_key: "tool_pdf_protect_desc".into(),
            category: ToolCategory::Pdf,
            icon: "lock".into(),
            keywords: vec![
                "encrypt".into(),
                "password".into(),
                "security".into(),
                "aes256".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(&["pdf"]),
            options_schema: vec![
                OptionField {
                    key: "password".into(),
                    label: "Open Password".into(),
                    label_key: "opt_pdf_open_pw".into(),
                    hint: "Required whenever anyone opens the protected PDF.".into(),
                    hint_key: "opt_pdf_open_pw_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("Enter strong password".into()),
                        secret: true,
                    },
                    default_value: json!(""),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "allow_printing".into(),
                    label: "Allow printing".into(),
                    label_key: "opt_pdf_allow_print".into(),
                    hint: "Permit recipients to print the document after unlocking.".into(),
                    hint_key: "opt_pdf_allow_print_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(true),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "allow_copying".into(),
                    label: "Allow copying text".into(),
                    label_key: "opt_pdf_allow_copy".into(),
                    hint: "Permit recipients to copy text after unlocking.".into(),
                    hint_key: "opt_pdf_allow_copy_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(true),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_protected".into(),
        }
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        let desc = self.descriptor();
        core_jobs::validate_inputs_against_spec(inputs, &desc.input_spec)?;
        if options.get_string_or("password", "").is_empty() {
            return Err(ToolError::invalid(
                "Please enter a password to encrypt the PDF.",
            ));
        }
        Ok(())
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let pw = ctx.options.get_string_or("password", "");
        let allow_print = ctx.options.get_bool("allow_printing", true);
        let allow_copy = ctx.options.get_bool("allow_copying", true);

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Encrypting PDF {} of {}", i + 1, ctx.inputs.len()),
            );
            let out_path = ctx.output_path_for(input, "_protected", Some("pdf"));
            let written = encrypt_pdf_aes256(input, &pw, &pw, allow_print, allow_copy, &out_path)?;
            outputs.push(pdf_artifact(Some(input), &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Encrypted {} PDF file(s) with AES-256.", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 11. Unlock PDF (`pdf.unlock`)
// ============================================================================
pub struct PdfUnlockTool;

impl Tool for PdfUnlockTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "pdf.unlock".into(),
            name: "Unlock PDF (Remove Password)".into(),
            name_key: "tool_pdf_unlock".into(),
            description: "Remove password protection from a PDF you have the password for.".into(),
            description_key: "tool_pdf_unlock_desc".into(),
            category: ToolCategory::Pdf,
            icon: "unlock".into(),
            keywords: vec!["decrypt".into(), "password".into(), "remove".into()],
            input_spec: ToolInputSpec::single_or_batch(&["pdf"]),
            options_schema: vec![OptionField {
                key: "password".into(),
                label: "Current PDF Password".into(),
                label_key: "opt_pdf_current_pw".into(),
                hint: "Enter the existing password to authenticate and save an unlocked copy."
                    .into(),
                hint_key: "opt_pdf_current_pw_hint".into(),
                kind: OptionKind::Text {
                    placeholder: Some("Current password".into()),
                    secret: true,
                },
                default_value: json!(""),
                required: true,
                visible_when: None,
            }],
            default_suffix: "_unlocked".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let pw = ctx.options.get_string_or("password", "");
        let mut outputs = Vec::with_capacity(ctx.inputs.len());

        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Unlocking PDF {} of {}", i + 1, ctx.inputs.len()),
            );
            let out_path = ctx.output_path_for(input, "_unlocked", Some("pdf"));
            let written = unlock_pdf_file(input, &pw, &out_path)?;
            outputs.push(pdf_artifact(Some(input), &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Unlocked {} PDF file(s).", outputs.len()),
            outputs,
        ))
    }
}
