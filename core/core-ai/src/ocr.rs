//! AI Optical Character Recognition (`ai.ocr`): PaddleOCR (`PP-OCRv4`) 3-stage ONNX pipeline
//! (DBNet text detection + 0°/180° angle classification + CTC greedy recognition) with optional
//! auto-contrast & projection-profile deskew preprocessing, streaming page-by-page PDF input via
//! `core-pdf`, and `.txt` / `.md` / **Searchable PDF** (`3 Tr` invisible text layer) outputs.

use crate::downloader::AiRuntimeHub;
use crate::manifest::{format_human_bytes, AiTask};
use crate::runtime::OrtSessionHandle;
use core_image::{encode_image_to_path, load_image_any, simd_resize, IMAGE_INPUT_EXTENSIONS};
use core_jobs::{
    validate_inputs_against_spec, CancelHandle, OptionField, OptionKind, OutputArtifact,
    SelectChoice, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError, ToolInputSpec,
    ToolOptions, ToolRunResult,
};
use core_pdf::render_pdf_page_to_png;
use image::{DynamicImage, Rgba, RgbaImage};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Single detected and recognized text line / region on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrRegionItem {
    /// Normalized `[x0, y0, x1, y1]` in `[0.0, 1.0]` relative to `(page_width, page_height)`.
    pub bbox_norm: [f32; 4],
    /// Pixel `[x0, y0, x1, y1]` on the rasterized page.
    pub bbox_px: [u32; 4],
    pub text: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrPageReport {
    pub source_path: String,
    pub is_pdf: bool,
    pub page_index: u32,
    pub page_width: u32,
    pub page_height: u32,
    pub full_text: String,
    pub regions: Vec<OcrRegionItem>,
}

pub struct AiOcrTool {
    hub: Arc<AiRuntimeHub>,
}

impl AiOcrTool {
    pub fn new(hub: Arc<AiRuntimeHub>) -> Self {
        Self { hub }
    }

    fn model_choices(&self) -> (String, Vec<SelectChoice>) {
        let mut default_id = "ocr.ppocr_v4_en".to_string();
        let mut choices = Vec::new();
        if let Ok(mgr) = self.hub.manager() {
            for m in mgr.list_models() {
                if m.spec.task == AiTask::Ocr {
                    if m.spec.default_for_task {
                        default_id = m.spec.id.clone();
                    }
                    choices.push(SelectChoice {
                        value: m.spec.id.clone(),
                        label: format!(
                            "{} ({})",
                            m.spec.display_name,
                            format_human_bytes(m.spec.size_bytes)
                        ),
                        description: Some(m.spec.description.clone()),
                    });
                }
            }
        }
        if choices.is_empty() {
            choices.push(SelectChoice {
                value: "ocr.ppocr_v4_en".into(),
                label: "PP-OCRv4 English & Latin Pack (10.2 MB)".into(),
                description: Some("DBNet detector + angle classifier + PP-OCRv4 recognizer".into()),
            });
        }
        (default_id, choices)
    }
}

impl Tool for AiOcrTool {
    fn descriptor(&self) -> ToolDescriptor {
        let (default_model, model_choices) = self.model_choices();
        let mut exts: Vec<&str> = IMAGE_INPUT_EXTENSIONS.to_vec();
        exts.push("pdf");

        ToolDescriptor {
            id: "ai.ocr".to_string(),
            name: "OCR Text & Searchable PDF".to_string(),
            name_key: "tool_ai_ocr".to_string(),
            description: "Extract selectable text from images and scanned PDFs offline with PaddleOCR — copy regions, export .txt/.md, or build a Searchable PDF."
                .to_string(),
            description_key: "tool_ai_ocr_desc".to_string(),
            category: ToolCategory::Ai,
            icon: "ocr".to_string(),
            keywords: vec![
                "ai".into(),
                "ocr".into(),
                "text".into(),
                "scan".into(),
                "searchable pdf".into(),
                "extract text".into(),
                "paddleocr".into(),
                "receipt".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(&exts),
            default_suffix: "ocr".to_string(),
            options_schema: vec![
                OptionField {
                    key: "model_id".into(),
                    label: "Language / OCR Model Pack".into(),
                    label_key: "opt_ai_ocr_model".into(),
                    hint: "English/Latin is compact and fast; additional language packs can be installed on demand."
                        .into(),
                    hint_key: "opt_ai_ocr_model_hint".into(),
                    kind: OptionKind::Select {
                        choices: model_choices,
                    },
                    default_value: json!(default_model),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "output_format".into(),
                    label: "Output Format".into(),
                    label_key: "opt_output_format".into(),
                    hint: "Searchable PDF embeds an invisible selectable text layer over original page images."
                        .into(),
                    hint_key: "opt_ai_ocr_output_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "searchable_pdf".into(),
                                label: "Searchable PDF (Original Page + Invisible Text Layer)"
                                    .into(),
                                description: Some(
                                    "Preserves exact scan appearance while making text selectable & searchable"
                                        .into(),
                                ),
                            },
                            SelectChoice {
                                value: "txt".into(),
                                label: "Plain Text (.txt)".into(),
                                description: Some("Clean UTF-8 text file".into()),
                            },
                            SelectChoice {
                                value: "md".into(),
                                label: "Markdown (.md)".into(),
                                description: Some("Structured per-page Markdown document".into()),
                            },
                            SelectChoice {
                                value: "copy_only".into(),
                                label: "Interactive Preview & Copy Only".into(),
                                description: Some(
                                    "Inspect highlighted regions and copy text without saving a file"
                                        .into(),
                                ),
                            },
                        ],
                    },
                    default_value: json!("searchable_pdf"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "enhance_contrast".into(),
                    label: "Enhance Scan Contrast".into(),
                    label_key: "opt_ai_ocr_contrast".into(),
                    hint: "Stretches faint gray scans and low-light document photos before recognition."
                        .into(),
                    hint_key: "opt_ai_ocr_contrast_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(false),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "deskew".into(),
                    label: "Auto-Deskew Tilted Scans".into(),
                    label_key: "opt_ai_ocr_deskew".into(),
                    hint: "Detects and straightens small page tilt angles (±5°) prior to text detection."
                        .into(),
                    hint_key: "opt_ai_ocr_deskew_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(false),
                    required: false,
                    visible_when: None,
                },
            ],
        }
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        let desc = self.descriptor();
        validate_inputs_against_spec(inputs, &desc.input_spec)?;
        let out_fmt = options.get_string_or("output_format", "searchable_pdf");
        if !matches!(
            out_fmt.as_str(),
            "searchable_pdf" | "txt" | "md" | "copy_only"
        ) {
            return Err(ToolError::invalid(format!(
                "Unsupported OCR output format '{out_fmt}'."
            )));
        }
        Ok(())
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        ctx.check_cancelled()?;
        let default_model = self.model_choices().0;
        let model_id = ctx.options.get_string_or("model_id", &default_model);
        let out_fmt = ctx.options.get_string_or("output_format", "searchable_pdf");
        let enhance_contrast = ctx.options.get_bool("enhance_contrast", false);
        let deskew = ctx.options.get_bool("deskew", false);

        ctx.report_progress(0.03, format!("Loading OCR pipeline '{model_id}'..."));
        let (spec, det_session) = self.hub.load_role_session(&model_id, "det")?;
        let cls_session = self
            .hub
            .load_role_session(&model_id, "cls")
            .ok()
            .map(|(_, s)| s);
        let (_, rec_session) = self.hub.load_role_session(&model_id, "rec")?;
        let (_, dict_path) = self.hub.resolve_installed_component(&model_id, "dict")?;
        let dictionary = load_ocr_dictionary(&dict_path)?;

        let mut warnings = Vec::new();
        if det_session.provider_info().fell_back_to_cpu {
            if let Some(ref reason) = det_session.provider_info().fallback_reason {
                warnings.push(reason.clone());
            }
        }

        let pipeline = OcrPipelineRefs {
            det: &det_session,
            cls: cls_session.as_deref(),
            rec: &rec_session,
            dictionary: &dictionary,
            enhance_contrast,
            deskew,
        };

        let total_files = ctx.inputs.len().max(1);
        let mut outputs = Vec::new();
        let mut all_pages_report: Vec<OcrPageReport> = Vec::new();
        let mut combined_all_text = Vec::new();

        for (f_idx, input_path) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            let file_base_p = f_idx as f32 / total_files as f32;
            let file_span_p = 1.0 / total_files as f32;
            let file_name = input_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("document");
            let is_pdf = input_path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false);

            let mut pdf_builder = if out_fmt == "searchable_pdf" {
                Some(SearchablePdfBuilder::new())
            } else {
                None
            };
            let mut file_page_texts: Vec<String> = Vec::new();

            if is_pdf {
                ctx.report_progress(
                    file_base_p + file_span_p * 0.04,
                    format!("Rasterizing {file_name} page 1..."),
                );
                let first_render = render_pdf_page_to_png(input_path, 0, 1800, None)?;
                let page_count = first_render.page_count.max(1);

                for page_idx in 0..page_count {
                    ctx.check_cancelled()?;
                    let p_frac = page_idx as f32 / page_count as f32;
                    ctx.report_progress(
                        file_base_p + file_span_p * (0.05 + p_frac * 0.88),
                        format!("OCR on {file_name} — page {}/{page_count}...", page_idx + 1),
                    );

                    let rendered = if page_idx == 0 {
                        first_render.clone()
                    } else {
                        render_pdf_page_to_png(input_path, page_idx, 1800, None)?
                    };
                    let page_img = image::load_from_memory(&rendered.png_bytes).map_err(|e| {
                        ToolError::failed(
                            "Failed decoding rasterized PDF page",
                            Some(e.to_string()),
                        )
                    })?;

                    let page_report = run_ocr_on_page(
                        &page_img,
                        input_path,
                        true,
                        page_idx,
                        &pipeline,
                        &ctx.cancel,
                    )?;
                    if let Some(ref mut builder) = pdf_builder {
                        builder.append_page(&page_img, &page_report.regions)?;
                    }
                    file_page_texts.push(page_report.full_text.clone());
                    all_pages_report.push(page_report);
                }
            } else {
                ctx.report_progress(
                    file_base_p + file_span_p * 0.12,
                    format!("Running OCR on {file_name}..."),
                );
                let page_img = load_image_any(input_path)?;
                let page_report =
                    run_ocr_on_page(&page_img, input_path, false, 0, &pipeline, &ctx.cancel)?;
                if let Some(ref mut builder) = pdf_builder {
                    builder.append_page(&page_img, &page_report.regions)?;
                }
                file_page_texts.push(page_report.full_text.clone());
                all_pages_report.push(page_report);
            }

            ctx.check_cancelled()?;
            for pt in &file_page_texts {
                if !pt.trim().is_empty() {
                    combined_all_text.push(pt.clone());
                }
            }

            let orig_size = fs::metadata(input_path).map(|m| m.len()).ok();
            match out_fmt.as_str() {
                "searchable_pdf" => {
                    let out_path = ctx.output_path_for(input_path, "searchable", Some("pdf"));
                    let size_bytes = pdf_builder
                        .expect("pdf_builder initialized")
                        .save(&out_path)?;
                    let label = out_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("searchable.pdf")
                        .to_string();
                    outputs.push(OutputArtifact {
                        path: out_path.display().to_string(),
                        label,
                        size_bytes,
                        original_size_bytes: orig_size,
                    });
                }
                "txt" => {
                    let out_path = ctx.output_path_for(input_path, "ocr", Some("txt"));
                    let content = if file_page_texts.len() == 1 {
                        format!("{}\n", file_page_texts[0])
                    } else {
                        file_page_texts
                            .iter()
                            .enumerate()
                            .map(|(i, t)| format!("--- Page {} ---\n{}", i + 1, t))
                            .collect::<Vec<_>>()
                            .join("\n\n")
                            + "\n"
                    };
                    fs::write(&out_path, content.as_bytes()).map_err(|e| {
                        ToolError::failed("Failed writing OCR .txt output", Some(e.to_string()))
                    })?;
                    let label = out_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("ocr.txt")
                        .to_string();
                    outputs.push(OutputArtifact {
                        path: out_path.display().to_string(),
                        label,
                        size_bytes: content.len() as u64,
                        original_size_bytes: orig_size,
                    });
                }
                "md" => {
                    let out_path = ctx.output_path_for(input_path, "ocr", Some("md"));
                    let body = if file_page_texts.len() == 1 {
                        format!("# OCR — {file_name}\n\n{}\n", file_page_texts[0])
                    } else {
                        let sections = file_page_texts
                            .iter()
                            .enumerate()
                            .map(|(i, t)| format!("## Page {}\n\n{}", i + 1, t))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        format!("# OCR — {file_name}\n\n{sections}\n")
                    };
                    fs::write(&out_path, body.as_bytes()).map_err(|e| {
                        ToolError::failed("Failed writing OCR .md output", Some(e.to_string()))
                    })?;
                    let label = out_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("ocr.md")
                        .to_string();
                    outputs.push(OutputArtifact {
                        path: out_path.display().to_string(),
                        label,
                        size_bytes: body.len() as u64,
                        original_size_bytes: orig_size,
                    });
                }
                _ => {}
            }
        }

        let total_regions: usize = all_pages_report.iter().map(|p| p.regions.len()).sum();
        let active_provider = det_session.provider_info().active_provider.clone();
        let summary = format!(
            "Recognized {total_regions} text region{} across {} page{} with {} ({})",
            if total_regions == 1 { "" } else { "s" },
            all_pages_report.len(),
            if all_pages_report.len() == 1 { "" } else { "s" },
            spec.display_name,
            active_provider.to_uppercase()
        );

        let mut result = ToolRunResult::with_outputs(summary, outputs);
        result.warnings = warnings;
        result.report_json = Some(json!({
            "tool_kind": "ai.ocr",
            "model_id": spec.id,
            "model_name": spec.display_name,
            "active_provider": active_provider,
            "output_format": out_fmt,
            "total_regions": total_regions,
            "full_text": combined_all_text.join("\n\n"),
            "pages": all_pages_report,
        }));

        ctx.report_progress(1.0, "OCR complete.");
        Ok(result)
    }
}

struct OcrPipelineRefs<'a> {
    det: &'a OrtSessionHandle,
    cls: Option<&'a OrtSessionHandle>,
    rec: &'a OrtSessionHandle,
    dictionary: &'a [String],
    enhance_contrast: bool,
    deskew: bool,
}

fn load_ocr_dictionary(path: &Path) -> Result<Vec<String>, ToolError> {
    let raw = fs::read_to_string(path).map_err(|e| {
        ToolError::failed(
            format!("Cannot read OCR character dictionary '{}'", path.display()),
            Some(e.to_string()),
        )
    })?;
    let chars: Vec<String> = raw
        .lines()
        .map(|l| l.trim_end_matches(['\r', '\n']).to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if chars.is_empty() {
        return Err(ToolError::failed(
            "OCR character dictionary is empty.",
            None,
        ));
    }
    Ok(chars)
}

fn run_ocr_on_page(
    orig_page: &DynamicImage,
    source_path: &Path,
    is_pdf: bool,
    page_index: u32,
    pipeline: &OcrPipelineRefs<'_>,
    cancel: &CancelHandle,
) -> Result<OcrPageReport, ToolError> {
    cancel.check()?;
    let mut working = orig_page.to_rgba8();
    if pipeline.enhance_contrast {
        working = auto_contrast_rgba(&working);
    }
    if pipeline.deskew {
        let angle = estimate_skew_angle_deg(&working);
        if angle.abs() >= 0.4 {
            working = rotate_rgba_white_bg(&working, -angle);
        }
    }

    let (page_w, page_h) = (working.width().max(1), working.height().max(1));
    let work_dyn = DynamicImage::ImageRgba8(working);

    // 1. DBNet Detection
    let max_side = 960u32;
    let longest = page_w.max(page_h);
    let scale = if longest > max_side {
        max_side as f32 / longest as f32
    } else {
        1.0
    };
    let det_w = (((page_w as f32 * scale).round() as u32).max(32)).next_multiple_of(32);
    let det_h = (((page_h as f32 * scale).round() as u32).max(32)).next_multiple_of(32);

    let det_resized = simd_resize(&work_dyn, det_w, det_h, "bilinear")?.to_rgb8();
    let plane = (det_w * det_h) as usize;
    let mut det_tensor = vec![0.0f32; 3 * plane];
    for (i, px) in det_resized.pixels().enumerate() {
        det_tensor[i] = ((px[0] as f32 / 255.0) - IMAGENET_MEAN[0]) / IMAGENET_STD[0];
        det_tensor[plane + i] = ((px[1] as f32 / 255.0) - IMAGENET_MEAN[1]) / IMAGENET_STD[1];
        det_tensor[2 * plane + i] = ((px[2] as f32 / 255.0) - IMAGENET_MEAN[2]) / IMAGENET_STD[2];
    }

    let (det_shape, det_probs) = pipeline.det.run_f32_tensor(
        &[1, 3, det_h as usize, det_w as usize],
        &det_tensor,
        cancel,
    )?;
    if det_probs.len() < plane {
        return Err(ToolError::failed(
            "OCR text detector returned unexpected tensor shape",
            Some(format!("Shape: {det_shape:?}")),
        ));
    }

    let boxes = extract_dbnet_boxes(
        &det_probs[..plane],
        det_w as usize,
        det_h as usize,
        page_w,
        page_h,
    );

    // 2. Angle Classification + CTC Recognition per box
    let mut regions = Vec::with_capacity(boxes.len());
    for [x0, y0, x1, y1] in boxes {
        cancel.check()?;
        let bw = (x1 - x0).max(2);
        let bh = (y1 - y0).max(2);
        let mut crop = work_dyn.crop_imm(x0, y0, bw, bh);

        // Rotate vertical narrow strips 90° CCW if height >= 2.2 * width
        if bh >= bw.saturating_mul(2) && bh > 24 {
            crop = DynamicImage::ImageRgba8(image::imageops::rotate270(&crop.to_rgba8()));
        }

        if let Some(cls_sess) = pipeline.cls {
            if should_rotate_180(&crop, cls_sess, cancel)? {
                crop = DynamicImage::ImageRgba8(image::imageops::rotate180(&crop.to_rgba8()));
            }
        }

        let (text, conf) = recognize_text_line(&crop, pipeline.rec, pipeline.dictionary, cancel)?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }

        regions.push(OcrRegionItem {
            bbox_norm: [
                (x0 as f32 / page_w as f32).clamp(0.0, 1.0),
                (y0 as f32 / page_h as f32).clamp(0.0, 1.0),
                (x1 as f32 / page_w as f32).clamp(0.0, 1.0),
                (y1 as f32 / page_h as f32).clamp(0.0, 1.0),
            ],
            bbox_px: [x0, y0, x1, y1],
            text: trimmed.to_string(),
            confidence: conf,
        });
    }

    let full_text = assemble_reading_order_text(&regions);
    Ok(OcrPageReport {
        source_path: source_path.display().to_string(),
        is_pdf,
        page_index,
        page_width: page_w,
        page_height: page_h,
        full_text,
        regions,
    })
}

fn should_rotate_180(
    crop: &DynamicImage,
    cls_sess: &OrtSessionHandle,
    cancel: &CancelHandle,
) -> Result<bool, ToolError> {
    let cls_w = 192u32;
    let cls_h = 48u32;
    let resized = simd_resize(crop, cls_w, cls_h, "bilinear")?.to_rgb8();
    let plane = (cls_w * cls_h) as usize;
    let mut tensor = vec![0.0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        tensor[i] = ((px[0] as f32 / 255.0) - 0.5) / 0.5;
        tensor[plane + i] = ((px[1] as f32 / 255.0) - 0.5) / 0.5;
        tensor[2 * plane + i] = ((px[2] as f32 / 255.0) - 0.5) / 0.5;
    }
    let (_, out) =
        cls_sess.run_f32_tensor(&[1, 3, cls_h as usize, cls_w as usize], &tensor, cancel)?;
    Ok(out.len() >= 2 && out[1] > 0.90 && out[1] > out[0])
}

fn recognize_text_line(
    crop: &DynamicImage,
    rec_sess: &OrtSessionHandle,
    dictionary: &[String],
    cancel: &CancelHandle,
) -> Result<(String, f32), ToolError> {
    let rec_h = 48u32;
    let aspect = crop.width().max(1) as f32 / crop.height().max(1) as f32;
    let rec_w = ((rec_h as f32 * aspect).round() as u32).clamp(48, 1280);
    let resized = simd_resize(crop, rec_w, rec_h, "bilinear")?.to_rgb8();

    let plane = (rec_w * rec_h) as usize;
    let mut tensor = vec![0.0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        tensor[i] = ((px[0] as f32 / 255.0) - 0.5) / 0.5;
        tensor[plane + i] = ((px[1] as f32 / 255.0) - 0.5) / 0.5;
        tensor[2 * plane + i] = ((px[2] as f32 / 255.0) - 0.5) / 0.5;
    }

    let (shape, probs) =
        rec_sess.run_f32_tensor(&[1, 3, rec_h as usize, rec_w as usize], &tensor, cancel)?;
    if shape.len() < 2 {
        return Err(ToolError::failed(
            "OCR recognizer returned invalid tensor shape",
            Some(format!("Shape: {shape:?}")),
        ));
    }
    let vocab_size = *shape.last().unwrap_or(&0);
    let time_steps = shape[shape.len() - 2];
    Ok(ctc_greedy_decode(
        &probs, time_steps, vocab_size, dictionary,
    ))
}

/// Decodes a `[time_steps, vocab_size]` CTC output matrix with repeat collapsing and blank (`0`) removal.
pub fn ctc_greedy_decode(
    matrix: &[f32],
    time_steps: usize,
    vocab_size: usize,
    dictionary: &[String],
) -> (String, f32) {
    if time_steps == 0 || vocab_size == 0 || matrix.len() < time_steps * vocab_size {
        return (String::new(), 0.0);
    }

    let mut decoded = String::new();
    let mut conf_sum = 0.0f32;
    let mut conf_count = 0usize;
    let mut prev_idx = 0usize;

    for t in 0..time_steps {
        let row = &matrix[t * vocab_size..(t + 1) * vocab_size];
        let mut best_idx = 0usize;
        let mut best_val = f32::NEG_INFINITY;
        let mut row_sum = 0.0f32;
        for (idx, &v) in row.iter().enumerate() {
            row_sum += v;
            if v > best_val {
                best_val = v;
                best_idx = idx;
            }
        }

        // Convert logit to softmax probability if model exported unnormalized logits
        let best_prob = if (row_sum - 1.0).abs() > 0.15 {
            let denom: f32 = row.iter().map(|&v| (v - best_val).exp()).sum();
            if denom > 0.0 {
                1.0 / denom
            } else {
                0.0
            }
        } else {
            best_val.clamp(0.0, 1.0)
        };

        if best_idx > 0 && best_idx != prev_idx {
            let dict_pos = best_idx - 1;
            if dict_pos < dictionary.len() {
                decoded.push_str(&dictionary[dict_pos]);
                conf_sum += best_prob;
                conf_count += 1;
            } else if dict_pos == dictionary.len() {
                // PaddleOCR `use_space_char: true` places ASCII space right after dictionary entries
                decoded.push(' ');
                conf_sum += best_prob;
                conf_count += 1;
            }
        }
        prev_idx = best_idx;
    }

    let avg_conf = if conf_count > 0 {
        conf_sum / conf_count as f32
    } else {
        0.0
    };
    (decoded, avg_conf)
}

/// Extracts, unclips, merges same-line fragments, and reading-order-sorts bounding boxes from a DBNet map.
pub fn extract_dbnet_boxes(
    prob_map: &[f32],
    map_w: usize,
    map_h: usize,
    orig_w: u32,
    orig_h: u32,
) -> Vec<[u32; 4]> {
    if map_w == 0 || map_h == 0 || prob_map.len() < map_w * map_h {
        return Vec::new();
    }

    let thresh = 0.30f32;
    let box_thresh = 0.45f32;
    let mut visited = vec![false; map_w * map_h];
    let mut stack = Vec::new();
    let mut raw_boxes: Vec<[u32; 4]> = Vec::new();

    let scale_x = orig_w as f32 / map_w as f32;
    let scale_y = orig_h as f32 / map_h as f32;

    for y in 0..map_h {
        for x in 0..map_w {
            let start = y * map_w + x;
            if visited[start] || prob_map[start] <= thresh {
                continue;
            }

            visited[start] = true;
            stack.push((x, y));

            let mut min_x = x;
            let mut max_x = x;
            let mut min_y = y;
            let mut max_y = y;
            let mut count = 0usize;
            let mut prob_sum = 0.0f32;

            while let Some((cx, cy)) = stack.pop() {
                let idx = cy * map_w + cx;
                count += 1;
                prob_sum += prob_map[idx];
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);

                let ny_min = cy.saturating_sub(1);
                let ny_max = (cy + 1).min(map_h - 1);
                let nx_min = cx.saturating_sub(1);
                let nx_max = (cx + 1).min(map_w - 1);
                for ny in ny_min..=ny_max {
                    let row = ny * map_w;
                    for nx in nx_min..=nx_max {
                        let nidx = row + nx;
                        if !visited[nidx] && prob_map[nidx] > thresh {
                            visited[nidx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }

            if count < 10 {
                continue;
            }
            let mean_score = prob_sum / count as f32;
            if mean_score < box_thresh {
                continue;
            }

            let comp_h = (max_y - min_y + 1) as f32;
            let unclip_x = (comp_h * 0.45).max(2.0);
            let unclip_y = (comp_h * 0.35).max(2.0);

            let ox0 = (((min_x as f32 - unclip_x).max(0.0)) * scale_x).floor() as u32;
            let oy0 = (((min_y as f32 - unclip_y).max(0.0)) * scale_y).floor() as u32;
            let ox1 = ((((max_x + 1) as f32 + unclip_x) * scale_x).ceil() as u32).min(orig_w);
            let oy1 = ((((max_y + 1) as f32 + unclip_y) * scale_y).ceil() as u32).min(orig_h);

            if ox1 > ox0 + 3 && oy1 > oy0 + 3 {
                raw_boxes.push([ox0, oy0, ox1, oy1]);
            }
        }
    }

    // Sort in natural reading order (top-to-bottom line bands, then left-to-right)
    raw_boxes.sort_by(|a, b| {
        let ha = (a[3] - a[1]).max(1) as i32;
        let hb = (b[3] - b[1]).max(1) as i32;
        let cya = ((a[1] + a[3]) / 2) as i32;
        let cyb = ((b[1] + b[3]) / 2) as i32;
        let line_tol = ha.min(hb) / 2;
        if (cya - cyb).abs() <= line_tol {
            a[0].cmp(&b[0])
        } else {
            cya.cmp(&cyb)
        }
    });

    raw_boxes
}

fn assemble_reading_order_text(regions: &[OcrRegionItem]) -> String {
    if regions.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = regions[0].text.clone();
    let mut prev_box = regions[0].bbox_px;

    for reg in regions.iter().skip(1) {
        let prev_cy = (prev_box[1] + prev_box[3]) as i32 / 2;
        let cur_cy = (reg.bbox_px[1] + reg.bbox_px[3]) as i32 / 2;
        let prev_h = (prev_box[3] - prev_box[1]).max(1) as i32;
        if (cur_cy - prev_cy).abs() <= prev_h / 2 {
            current_line.push(' ');
            current_line.push_str(&reg.text);
        } else {
            lines.push(current_line);
            current_line = reg.text.clone();
        }
        prev_box = reg.bbox_px;
    }
    lines.push(current_line);
    lines.join("\n")
}

// ============================================================================
// Lightweight Scan Preprocessing (Auto-Contrast + Projection-Profile Deskew)
// ============================================================================

pub fn auto_contrast_rgba(src: &RgbaImage) -> RgbaImage {
    let mut hist = [0u32; 256];
    let total = (src.width() * src.height()) as usize;
    if total == 0 {
        return src.clone();
    }
    for px in src.pixels() {
        let luma = ((px[0] as u32 * 77 + px[1] as u32 * 150 + px[2] as u32 * 29) >> 8) as usize;
        hist[luma.min(255)] += 1;
    }

    let low_target = (total / 100).max(1) as u32;
    let high_target = (total * 99 / 100).max(1) as u32;
    let mut acc = 0u32;
    let mut lo = 0u8;
    let mut hi = 255u8;
    for (i, &c) in hist.iter().enumerate() {
        acc += c;
        if acc <= low_target {
            lo = i as u8;
        }
        if acc <= high_target {
            hi = i as u8;
        }
    }
    if hi <= lo + 8 {
        return src.clone();
    }
    let scale = 255.0 / (hi - lo) as f32;
    let mut out = RgbaImage::new(src.width(), src.height());
    for (s, d) in src.pixels().zip(out.pixels_mut()) {
        let r = ((s[0].saturating_sub(lo) as f32) * scale)
            .round()
            .clamp(0.0, 255.0) as u8;
        let g = ((s[1].saturating_sub(lo) as f32) * scale)
            .round()
            .clamp(0.0, 255.0) as u8;
        let b = ((s[2].saturating_sub(lo) as f32) * scale)
            .round()
            .clamp(0.0, 255.0) as u8;
        *d = Rgba([r, g, b, s[3]]);
    }
    out
}

pub fn estimate_skew_angle_deg(src: &RgbaImage) -> f32 {
    let max_thumb = 360u32;
    let (w, h) = (src.width().max(1), src.height().max(1));
    let scale = (max_thumb as f32 / w.max(h) as f32).min(1.0);
    let tw = ((w as f32 * scale).round() as usize).max(16);
    let th = ((h as f32 * scale).round() as usize).max(16);

    let thumb = simd_resize(
        &DynamicImage::ImageRgba8(src.clone()),
        tw as u32,
        th as u32,
        "bilinear",
    )
    .map(|d| d.to_rgba8())
    .unwrap_or_else(|_| src.clone());

    let mut ink = vec![0.0f32; tw * th];
    for (i, px) in thumb.pixels().enumerate() {
        let luma = (px[0] as f32 * 0.299 + px[1] as f32 * 0.587 + px[2] as f32 * 0.114) / 255.0;
        ink[i] = (1.0 - luma).max(0.0);
    }

    let cx = tw as f32 * 0.5;
    let cy = th as f32 * 0.5;
    let mut best_angle = 0.0f32;
    let mut best_var = -1.0f32;

    let mut step_idx = -10i32;
    while step_idx <= 10 {
        let deg = step_idx as f32 * 0.5;
        let rad = deg.to_radians();
        let sin_a = rad.sin();
        let cos_a = rad.cos();

        let mut row_sums = vec![0.0f32; th];
        for y in 0..th {
            let dy = y as f32 - cy;
            for x in 0..tw {
                let dx = x as f32 - cx;
                let ry = (-dx * sin_a + dy * cos_a + cy).round() as isize;
                if ry >= 0 && (ry as usize) < th {
                    row_sums[ry as usize] += ink[y * tw + x];
                }
            }
        }

        let mean: f32 = row_sums.iter().sum::<f32>() / th as f32;
        let var: f32 = row_sums
            .iter()
            .map(|&v| {
                let d = v - mean;
                d * d
            })
            .sum();

        if var > best_var {
            best_var = var;
            best_angle = deg;
        }
        step_idx += 1;
    }

    best_angle
}

fn rotate_rgba_white_bg(src: &RgbaImage, angle_deg: f32) -> RgbaImage {
    let w = src.width() as usize;
    let h = src.height() as usize;
    let mut out = RgbaImage::from_pixel(w as u32, h as u32, Rgba([255, 255, 255, 255]));
    let rad = (-angle_deg).to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;

    for dy in 0..h {
        let fy = dy as f32 - cy;
        for dx in 0..w {
            let fx = dx as f32 - cx;
            let sx = fx * cos_a - fy * sin_a + cx;
            let sy = fx * sin_a + fy * cos_a + cy;
            if sx >= 0.0 && sx < (w - 1) as f32 && sy >= 0.0 && sy < (h - 1) as f32 {
                let x0 = sx.floor() as usize;
                let y0 = sy.floor() as usize;
                out.put_pixel(dx as u32, dy as u32, *src.get_pixel(x0 as u32, y0 as u32));
            }
        }
    }
    out
}

// ============================================================================
// Streaming Searchable PDF Builder (`lopdf`: Original Page Image + `3 Tr` Text)
// ============================================================================

struct SearchablePdfBuilder {
    doc: Document,
    pages_id: ObjectId,
    font_id: ObjectId,
    kids: Vec<Object>,
}

impl SearchablePdfBuilder {
    fn new() -> Self {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        Self {
            doc,
            pages_id,
            font_id,
            kids: Vec::new(),
        }
    }

    fn append_page(
        &mut self,
        page_img: &DynamicImage,
        regions: &[OcrRegionItem],
    ) -> Result<(), ToolError> {
        let (iw, ih) = (page_img.width().max(1), page_img.height().max(1));
        let tmp_jpg = std::env::temp_dir().join(format!(
            "tc_ocr_page_{}_{}.jpg",
            std::process::id(),
            self.kids.len()
        ));
        encode_image_to_path(page_img, &tmp_jpg, "jpg", 88, false)?;
        let jpg_bytes = fs::read(&tmp_jpg).map_err(|e| {
            ToolError::failed("Failed reading encoded OCR page JPEG", Some(e.to_string()))
        })?;
        let _ = fs::remove_file(&tmp_jpg);

        let img_id = self.doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => iw as i64,
                "Height" => ih as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpg_bytes,
        ));

        let page_w_pt = (iw as f32 * 0.75).max(72.0);
        let page_h_pt = (ih as f32 * 0.75).max(72.0);

        let mut ops = vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    page_w_pt.into(),
                    0.into(),
                    0.into(),
                    page_h_pt.into(),
                    0.into(),
                    0.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
            Operation::new("Q", vec![]),
        ];

        if !regions.is_empty() {
            ops.push(Operation::new("BT", vec![]));
            // PDF Text Rendering Mode 3 = Invisible selectable/searchable text!
            ops.push(Operation::new("Tr", vec![3.into()]));

            for reg in regions {
                let x_pt = reg.bbox_norm[0] * page_w_pt;
                let w_pt = ((reg.bbox_norm[2] - reg.bbox_norm[0]) * page_w_pt).max(4.0);
                let h_pt = ((reg.bbox_norm[3] - reg.bbox_norm[1]) * page_h_pt).max(4.0);
                let baseline_y_pt =
                    ((1.0 - reg.bbox_norm[3]) * page_h_pt + h_pt * 0.16).clamp(0.0, page_h_pt);
                let font_pt = (h_pt * 0.78).clamp(4.0, 96.0);

                let pdf_bytes = encode_winansi_lossy(&reg.text);
                if pdf_bytes.is_empty() {
                    continue;
                }
                let est_w = (pdf_bytes.len().max(1) as f32) * font_pt * 0.52;
                let horiz_scale = ((w_pt / est_w) * 100.0).clamp(40.0, 220.0);

                ops.push(Operation::new(
                    "Tf",
                    vec![Object::Name(b"F1".to_vec()), font_pt.into()],
                ));
                ops.push(Operation::new("Tz", vec![horiz_scale.into()]));
                ops.push(Operation::new(
                    "Tm",
                    vec![
                        1.into(),
                        0.into(),
                        0.into(),
                        1.into(),
                        x_pt.into(),
                        baseline_y_pt.into(),
                    ],
                ));
                ops.push(Operation::new(
                    "Tj",
                    vec![Object::string_literal(pdf_bytes)],
                ));
            }

            ops.push(Operation::new("ET", vec![]));
        }

        let content_bytes = Content { operations: ops }.encode().map_err(|e| {
            ToolError::failed(
                "Failed encoding Searchable PDF content stream",
                Some(e.to_string()),
            )
        })?;
        let content_id = self
            .doc
            .add_object(Stream::new(dictionary! {}, content_bytes));

        let resources = dictionary! {
            "Font" => dictionary! {
                "F1" => self.font_id,
            },
            "XObject" => dictionary! {
                "Im0" => img_id,
            },
        };

        let page_id = self.doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "Contents" => content_id,
            "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), page_w_pt.into(), page_h_pt.into()],
        });
        self.kids.push(page_id.into());
        Ok(())
    }

    fn save(mut self, out_path: &Path) -> Result<u64, ToolError> {
        let page_count = self.kids.len() as i64;
        self.doc.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => self.kids,
                "Count" => page_count,
            }),
        );
        let catalog_id = self.doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => self.pages_id,
        });
        self.doc.trailer.set("Root", catalog_id);
        self.doc.compress();

        self.doc.save(out_path).map_err(|e| {
            ToolError::failed("Failed saving Searchable PDF file", Some(e.to_string()))
        })?;
        fs::metadata(out_path)
            .map(|m| m.len())
            .map_err(|e| ToolError::failed("Failed stating Searchable PDF", Some(e.to_string())))
    }
}

fn encode_winansi_lossy(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if (0x20..=0x7E).contains(&code) || (0xA0..=0xFF).contains(&code) {
            out.push(code as u8);
        } else if ch.is_whitespace() {
            out.push(b' ');
        } else {
            out.push(b'?');
        }
    }
    out
}
