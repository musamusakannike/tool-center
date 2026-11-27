//! AI Background Removal (`ai.remove_bg`): salient-object & portrait matting on ONNX Runtime
//! (`U²-Netp`, `U²-Net`, `BiRefNet-Tiny`) with full-resolution preservation, edge-aware joint
//! bilateral mask refinement, and transparent / solid-color / Gaussian-blur / custom-image
//! background compositing.

use crate::downloader::AiRuntimeHub;
use crate::manifest::{format_human_bytes, AiTask};
use core_image::{encode_image_to_path, load_image_any, simd_resize, IMAGE_INPUT_EXTENSIONS};
use core_jobs::{
    validate_inputs_against_spec, OptionField, OptionKind, OutputArtifact, SelectChoice, Tool,
    ToolCategory, ToolContext, ToolDescriptor, ToolError, ToolInputSpec, ToolOptions,
    ToolRunResult, VisibilityCondition,
};
use image::{DynamicImage, Rgba, RgbaImage};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

pub struct AiRemoveBgTool {
    hub: Arc<AiRuntimeHub>,
}

impl AiRemoveBgTool {
    pub fn new(hub: Arc<AiRuntimeHub>) -> Self {
        Self { hub }
    }

    fn model_choices(&self) -> (String, Vec<SelectChoice>) {
        let mut default_id = "bg.u2netp".to_string();
        let mut choices = Vec::new();
        if let Ok(mgr) = self.hub.manager() {
            for m in mgr.list_models() {
                if m.spec.task == AiTask::BackgroundRemoval {
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
                value: "bg.u2netp".into(),
                label: "U²-Netp (4.4 MB)".into(),
                description: Some("Fast general & portrait background removal".into()),
            });
        }
        (default_id, choices)
    }
}

impl Tool for AiRemoveBgTool {
    fn descriptor(&self) -> ToolDescriptor {
        let (default_model, model_choices) = self.model_choices();
        let img_exts: Vec<String> = IMAGE_INPUT_EXTENSIONS
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        ToolDescriptor {
            id: "ai.remove_bg".to_string(),
            name: "Remove Background".to_string(),
            name_key: "tool_ai_remove_bg".to_string(),
            description: "Isolate subjects with AI matting at full original resolution — export transparent PNGs or replace with color, blur, or an image."
                .to_string(),
            description_key: "tool_ai_remove_bg_desc".to_string(),
            category: ToolCategory::Ai,
            icon: "bg_remove".to_string(),
            keywords: vec![
                "ai".into(),
                "background".into(),
                "remove".into(),
                "cutout".into(),
                "transparent".into(),
                "alpha".into(),
                "matting".into(),
                "portrait".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            default_suffix: "nobg".to_string(),
            options_schema: vec![
                OptionField {
                    key: "model_id".into(),
                    label: "AI Model".into(),
                    label_key: "opt_ai_model".into(),
                    hint: "U²-Netp is fast and compact; U²-Net Full and BiRefNet capture finer hair and complex silhouettes."
                        .into(),
                    hint_key: "opt_ai_bg_model_hint".into(),
                    kind: OptionKind::Select {
                        choices: model_choices,
                    },
                    default_value: json!(default_model),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "bg_mode".into(),
                    label: "Background Output".into(),
                    label_key: "opt_ai_bg_mode".into(),
                    hint: "Keep transparency (PNG alpha) or composite onto a replacement background."
                        .into(),
                    hint_key: "opt_ai_bg_mode_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "transparent".into(),
                                label: "Transparent PNG (Alpha)".into(),
                                description: Some("Clean cutout with smooth alpha channel".into()),
                            },
                            SelectChoice {
                                value: "solid_color".into(),
                                label: "Solid Color".into(),
                                description: Some("Studio backdrop color (e.g. #FFFFFF)".into()),
                            },
                            SelectChoice {
                                value: "blur".into(),
                                label: "Blurred Original Background".into(),
                                description: Some("Portrait / product depth-of-field bokeh".into()),
                            },
                            SelectChoice {
                                value: "custom_image".into(),
                                label: "Custom Background Image".into(),
                                description: Some("Composite onto another image file".into()),
                            },
                        ],
                    },
                    default_value: json!("transparent"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "bg_color".into(),
                    label: "Background Color (Hex)".into(),
                    label_key: "opt_ai_bg_color".into(),
                    hint: "Hex RGB color such as #FFFFFF (white) or #0F172A (slate).".into(),
                    hint_key: "opt_ai_bg_color_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("#FFFFFF".into()),
                        secret: false,
                    },
                    default_value: json!("#FFFFFF"),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "bg_mode".into(),
                        equals_value: json!("solid_color"),
                    }),
                },
                OptionField {
                    key: "blur_sigma".into(),
                    label: "Background Blur Strength".into(),
                    label_key: "opt_ai_blur_sigma".into(),
                    hint: "Gaussian blur radius in pixels applied behind the subject (2–60).".into(),
                    hint_key: "opt_ai_blur_sigma_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(2),
                        max: Some(60),
                        step: Some(1),
                        unit: Some("px".into()),
                    },
                    default_value: json!(18),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "bg_mode".into(),
                        equals_value: json!("blur"),
                    }),
                },
                OptionField {
                    key: "bg_image_path".into(),
                    label: "Replacement Background Image".into(),
                    label_key: "opt_ai_bg_image".into(),
                    hint: "Choose an image file to place behind the isolated subject.".into(),
                    hint_key: "opt_ai_bg_image_hint".into(),
                    kind: OptionKind::FilePath {
                        accepted_extensions: img_exts,
                    },
                    default_value: json!(""),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "bg_mode".into(),
                        equals_value: json!("custom_image"),
                    }),
                },
                OptionField {
                    key: "edge_refine".into(),
                    label: "High-Res Edge Refinement".into(),
                    label_key: "opt_ai_edge_refine".into(),
                    hint: "Snaps upsampled alpha contours to full-resolution RGB edges via joint bilateral filtering."
                        .into(),
                    hint_key: "opt_ai_edge_refine_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(true),
                    required: false,
                    visible_when: None,
                },
            ],
        }
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        let desc = self.descriptor();
        validate_inputs_against_spec(inputs, &desc.input_spec)?;
        let mode = options.get_string_or("bg_mode", "transparent");
        match mode.as_str() {
            "transparent" | "blur" => {}
            "solid_color" => {
                let hex = options.get_string_or("bg_color", "#FFFFFF");
                parse_hex_rgb(&hex)?;
            }
            "custom_image" => {
                let p = options.get_string_or("bg_image_path", "");
                if p.trim().is_empty() {
                    return Err(ToolError::invalid(
                        "Please select a replacement background image file.",
                    ));
                }
                if !Path::new(p.trim()).is_file() {
                    return Err(ToolError::invalid(format!(
                        "Replacement background image does not exist: '{}'",
                        p.trim()
                    )));
                }
            }
            other => {
                return Err(ToolError::invalid(format!(
                    "Unsupported background mode '{other}'."
                )));
            }
        }
        Ok(())
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        ctx.check_cancelled()?;
        let default_model = self.model_choices().0;
        let model_id = ctx.options.get_string_or("model_id", &default_model);
        let bg_mode = ctx.options.get_string_or("bg_mode", "transparent");
        let edge_refine = ctx.options.get_bool("edge_refine", true);
        let solid_rgb = if bg_mode == "solid_color" {
            parse_hex_rgb(&ctx.options.get_string_or("bg_color", "#FFFFFF"))?
        } else {
            [255, 255, 255]
        };
        let blur_sigma = (ctx.options.get_i64("blur_sigma", 18).clamp(2, 60)) as f32;
        let custom_bg_img = if bg_mode == "custom_image" {
            let bg_path = ctx.options.get_string_or("bg_image_path", "");
            Some(load_image_any(Path::new(bg_path.trim()))?)
        } else {
            None
        };

        ctx.report_progress(0.04, format!("Loading AI model '{model_id}'..."));
        let (spec, session) = self.hub.load_role_session(&model_id, "primary")?;
        let inf_size: u32 = if model_id.contains("birefnet") {
            1024
        } else {
            320
        };

        let mut warnings = Vec::new();
        if session.provider_info().fell_back_to_cpu {
            if let Some(ref reason) = session.provider_info().fallback_reason {
                warnings.push(reason.clone());
            }
        }

        let total = ctx.inputs.len().max(1);
        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        let mut report_items = Vec::with_capacity(ctx.inputs.len());

        for (idx, input_path) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            let base_p = idx as f32 / total as f32;
            let span_p = 1.0 / total as f32;
            let file_name = input_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("image");

            ctx.report_progress(
                base_p + span_p * 0.10,
                format!("({}/{}) Decoding {file_name}...", idx + 1, total),
            );
            let orig_img = load_image_any(input_path)?;
            let orig_rgba = orig_img.to_rgba8();
            let (orig_w, orig_h) = (orig_rgba.width(), orig_rgba.height());

            // 1. Downscale to inference square & normalize (ImageNet mean/std, NCHW)
            let resized = simd_resize(&orig_img, inf_size, inf_size, "bilinear")?;
            let resized_rgb = resized.to_rgb8();
            let plane = (inf_size * inf_size) as usize;
            let mut input_tensor = vec![0.0f32; 3 * plane];
            for (i, px) in resized_rgb.pixels().enumerate() {
                input_tensor[i] = ((px[0] as f32 / 255.0) - IMAGENET_MEAN[0]) / IMAGENET_STD[0];
                input_tensor[plane + i] =
                    ((px[1] as f32 / 255.0) - IMAGENET_MEAN[1]) / IMAGENET_STD[1];
                input_tensor[2 * plane + i] =
                    ((px[2] as f32 / 255.0) - IMAGENET_MEAN[2]) / IMAGENET_STD[2];
            }

            ctx.report_progress(
                base_p + span_p * 0.35,
                format!(
                    "({}/{}) Running AI matting ({}) on {file_name}...",
                    idx + 1,
                    total,
                    session.provider_info().active_provider.to_uppercase()
                ),
            );
            let (out_shape, raw_pred) = session.run_f32_tensor(
                &[1, 3, inf_size as usize, inf_size as usize],
                &input_tensor,
                &ctx.cancel,
            )?;
            ctx.check_cancelled()?;

            // Extract spatial dimensions of first output map (handles [1, 1, H, W], [1, H, W], etc.)
            let (mask_h, mask_w) = infer_spatial_hw(&out_shape, inf_size as usize);
            let expected_len = mask_h * mask_w;
            if raw_pred.len() < expected_len || expected_len == 0 {
                return Err(ToolError::failed(
                    "Unexpected AI model mask dimensions",
                    Some(format!("Output shape: {out_shape:?}")),
                ));
            }
            let norm_small = normalize_saliency_map(&raw_pred[..expected_len]);

            // 2. Upsample mask back to FULL original resolution + edge-aware joint bilateral refinement
            ctx.report_progress(
                base_p + span_p * 0.72,
                format!(
                    "({}/{}) Upsampling & refining {orig_w}×{orig_h} mask...",
                    idx + 1,
                    total
                ),
            );
            let coarse_alpha = bilinear_upsample_f32(
                &norm_small,
                mask_w,
                mask_h,
                orig_w as usize,
                orig_h as usize,
            );
            let final_alpha = if edge_refine {
                refine_alpha_joint_bilateral(&orig_rgba, &coarse_alpha)
            } else {
                coarse_alpha
            };

            // 3. Composite foreground at full original resolution
            ctx.report_progress(
                base_p + span_p * 0.86,
                format!("({}/{}) Compositing output PNG...", idx + 1, total),
            );
            let composited = composite_foreground(
                &orig_rgba,
                &final_alpha,
                &bg_mode,
                solid_rgb,
                blur_sigma,
                custom_bg_img.as_ref(),
            )?;

            let suffix = if bg_mode == "transparent" {
                "nobg"
            } else {
                "bg"
            };
            let out_path = ctx.output_path_for(input_path, suffix, Some("png"));
            let size_bytes = encode_image_to_path(
                &DynamicImage::ImageRgba8(composited),
                &out_path,
                "png",
                90,
                false,
            )?;

            let orig_size = fs::metadata(input_path).map(|m| m.len()).ok();
            let label = out_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("output.png")
                .to_string();
            outputs.push(OutputArtifact {
                path: out_path.display().to_string(),
                label,
                size_bytes,
                original_size_bytes: orig_size,
            });

            report_items.push(json!({
                "input_path": input_path.display().to_string(),
                "output_path": out_path.display().to_string(),
                "width": orig_w,
                "height": orig_h,
                "bg_mode": bg_mode,
            }));
        }

        let summary = format!(
            "Processed {} image{} with {} ({})",
            outputs.len(),
            if outputs.len() == 1 { "" } else { "s" },
            spec.display_name,
            session.provider_info().active_provider.to_uppercase()
        );
        let mut result = ToolRunResult::with_outputs(summary, outputs);
        result.warnings = warnings;
        result.report_json = Some(json!({
            "tool_kind": "ai.remove_bg",
            "model_id": spec.id,
            "model_name": spec.display_name,
            "active_provider": session.provider_info().active_provider,
            "fell_back_to_cpu": session.provider_info().fell_back_to_cpu,
            "bg_mode": bg_mode,
            "items": report_items,
        }));

        ctx.report_progress(1.0, "Background removal complete.");
        Ok(result)
    }
}

fn infer_spatial_hw(shape: &[usize], fallback: usize) -> (usize, usize) {
    if shape.len() >= 2 {
        let h = shape[shape.len() - 2].max(1);
        let w = shape[shape.len() - 1].max(1);
        (h, w)
    } else {
        (fallback, fallback)
    }
}

/// Converts raw model output (probabilities in `[0, 1]` or raw logits) into normalized `[0.0, 1.0]` alpha.
pub fn normalize_saliency_map(raw: &[f32]) -> Vec<f32> {
    if raw.is_empty() {
        return Vec::new();
    }
    let mut raw_min = f32::INFINITY;
    let mut raw_max = f32::NEG_INFINITY;
    for &v in raw {
        if v.is_finite() {
            raw_min = raw_min.min(v);
            raw_max = raw_max.max(v);
        }
    }
    let needs_sigmoid = raw_min < -0.05 || raw_max > 1.05;
    let mut probs = Vec::with_capacity(raw.len());
    let mut p_min = f32::INFINITY;
    let mut p_max = f32::NEG_INFINITY;
    for &v in raw {
        let clean = if v.is_finite() { v } else { 0.0 };
        let p = if needs_sigmoid {
            1.0 / (1.0 + (-clean.clamp(-40.0, 40.0)).exp())
        } else {
            clean.clamp(0.0, 1.0)
        };
        p_min = p_min.min(p);
        p_max = p_max.max(p);
        probs.push(p);
    }

    let span = p_max - p_min;
    if span > 0.05 {
        for p in &mut probs {
            *p = ((*p - p_min) / span).clamp(0.0, 1.0);
        }
    }
    probs
}

/// Bilinearly upsamples a single-channel `[src_h, src_w]` mask to `[dst_h, dst_w]`.
pub fn bilinear_upsample_f32(
    src: &[f32],
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
) -> Vec<f32> {
    if dst_w == 0 || dst_h == 0 || src.is_empty() {
        return vec![0.0; dst_w * dst_h];
    }
    if src_w == dst_w && src_h == dst_h {
        return src.to_vec();
    }

    let mut dst = vec![0.0f32; dst_w * dst_h];
    let scale_x = src_w as f32 / dst_w as f32;
    let scale_y = src_h as f32 / dst_h as f32;

    for dy in 0..dst_h {
        let sy = ((dy as f32 + 0.5) * scale_y - 0.5).clamp(0.0, (src_h - 1) as f32);
        let y0 = sy.floor() as usize;
        let y1 = (y0 + 1).min(src_h - 1);
        let wy = sy - y0 as f32;
        let row0 = y0 * src_w;
        let row1 = y1 * src_w;
        let dst_row = dy * dst_w;

        for dx in 0..dst_w {
            let sx = ((dx as f32 + 0.5) * scale_x - 0.5).clamp(0.0, (src_w - 1) as f32);
            let x0 = sx.floor() as usize;
            let x1 = (x0 + 1).min(src_w - 1);
            let wx = sx - x0 as f32;

            let v00 = src[row0 + x0];
            let v10 = src[row0 + x1];
            let v01 = src[row1 + x0];
            let v11 = src[row1 + x1];

            let top = v00 + (v10 - v00) * wx;
            let bot = v01 + (v11 - v01) * wx;
            dst[dst_row + dx] = (top + (bot - top) * wy).clamp(0.0, 1.0);
        }
    }
    dst
}

/// Edge-aware 5×5 joint bilateral filter guided by full-resolution RGB pixels.
/// Evaluates only boundary/transition pixels (`0.015 < alpha < 0.985`), keeping CPU runtime `O(perimeter)`!
pub fn refine_alpha_joint_bilateral(orig_rgba: &RgbaImage, coarse_alpha: &[f32]) -> Vec<f32> {
    let w = orig_rgba.width() as usize;
    let h = orig_rgba.height() as usize;
    if w == 0 || h == 0 || coarse_alpha.len() != w * h {
        return coarse_alpha.to_vec();
    }

    let raw = orig_rgba.as_raw();
    let mut refined = coarse_alpha.to_vec();
    const SPATIAL_W: [[f32; 5]; 5] = [
        [0.210, 0.376, 0.458, 0.376, 0.210],
        [0.376, 0.676, 0.822, 0.676, 0.376],
        [0.458, 0.822, 1.000, 0.822, 0.458],
        [0.376, 0.676, 0.822, 0.676, 0.376],
        [0.210, 0.376, 0.458, 0.376, 0.210],
    ];
    let inv_two_sigma_color_sq = 0.000_74_f32;

    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let idx = row + x;
            let a_center = coarse_alpha[idx];
            if a_center <= 0.015 || a_center >= 0.985 {
                continue;
            }
            let c_off = idx * 4;
            let cr = raw[c_off] as f32;
            let cg = raw[c_off + 1] as f32;
            let cb = raw[c_off + 2] as f32;

            let mut sum_w = 0.0f32;
            let mut sum_a = 0.0f32;

            let y_min = y.saturating_sub(2);
            let y_max = (y + 2).min(h - 1);
            let x_min = x.saturating_sub(2);
            let x_max = (x + 2).min(w - 1);

            for (ny, sw_row) in (y_min..=y_max).map(|ny| (ny, &SPATIAL_W[ny + 2 - y])) {
                let nrow = ny * w;
                for nx in x_min..=x_max {
                    let nidx = nrow + nx;
                    let n_off = nidx * 4;
                    let dr = raw[n_off] as f32 - cr;
                    let dg = raw[n_off + 1] as f32 - cg;
                    let db = raw[n_off + 2] as f32 - cb;
                    let color_dist_sq = dr * dr + dg * dg + db * db;
                    let w_color = (-color_dist_sq * inv_two_sigma_color_sq).exp();
                    let w_total = sw_row[nx + 2 - x] * w_color;
                    sum_w += w_total;
                    sum_a += w_total * coarse_alpha[nidx];
                }
            }

            if sum_w > 1e-6 {
                refined[idx] = (sum_a / sum_w).clamp(0.0, 1.0);
            }
        }
    }
    refined
}

fn composite_foreground(
    orig: &RgbaImage,
    alpha_mask: &[f32],
    bg_mode: &str,
    solid_rgb: [u8; 3],
    blur_sigma: f32,
    custom_bg: Option<&DynamicImage>,
) -> Result<RgbaImage, ToolError> {
    let w = orig.width();
    let h = orig.height();
    let mut out = RgbaImage::new(w, h);

    match bg_mode {
        "transparent" => {
            for (i, (src_px, dst_px)) in orig.pixels().zip(out.pixels_mut()).enumerate() {
                let src_a = src_px[3] as f32 / 255.0;
                let final_a = (alpha_mask[i] * src_a * 255.0).round().clamp(0.0, 255.0) as u8;
                *dst_px = Rgba([src_px[0], src_px[1], src_px[2], final_a]);
            }
        }
        "solid_color" => {
            for (i, (src_px, dst_px)) in orig.pixels().zip(out.pixels_mut()).enumerate() {
                let a = (alpha_mask[i] * (src_px[3] as f32 / 255.0)).clamp(0.0, 1.0);
                let inv = 1.0 - a;
                let r = (src_px[0] as f32 * a + solid_rgb[0] as f32 * inv).round() as u8;
                let g = (src_px[1] as f32 * a + solid_rgb[1] as f32 * inv).round() as u8;
                let b = (src_px[2] as f32 * a + solid_rgb[2] as f32 * inv).round() as u8;
                *dst_px = Rgba([r, g, b, 255]);
            }
        }
        "blur" => {
            let blurred = separable_gaussian_blur_rgba(orig, blur_sigma);
            for (i, ((src_px, bg_px), dst_px)) in orig
                .pixels()
                .zip(blurred.pixels())
                .zip(out.pixels_mut())
                .enumerate()
            {
                let a = (alpha_mask[i] * (src_px[3] as f32 / 255.0)).clamp(0.0, 1.0);
                let inv = 1.0 - a;
                let r = (src_px[0] as f32 * a + bg_px[0] as f32 * inv).round() as u8;
                let g = (src_px[1] as f32 * a + bg_px[1] as f32 * inv).round() as u8;
                let b = (src_px[2] as f32 * a + bg_px[2] as f32 * inv).round() as u8;
                *dst_px = Rgba([r, g, b, 255]);
            }
        }
        "custom_image" => {
            let bg_dyn = custom_bg
                .ok_or_else(|| ToolError::invalid("Missing replacement background image."))?;
            let fitted_bg = cover_resize_rgba(bg_dyn, w, h)?;
            for (i, ((src_px, bg_px), dst_px)) in orig
                .pixels()
                .zip(fitted_bg.pixels())
                .zip(out.pixels_mut())
                .enumerate()
            {
                let a = (alpha_mask[i] * (src_px[3] as f32 / 255.0)).clamp(0.0, 1.0);
                let inv = 1.0 - a;
                let r = (src_px[0] as f32 * a + bg_px[0] as f32 * inv).round() as u8;
                let g = (src_px[1] as f32 * a + bg_px[1] as f32 * inv).round() as u8;
                let b = (src_px[2] as f32 * a + bg_px[2] as f32 * inv).round() as u8;
                *dst_px = Rgba([r, g, b, 255]);
            }
        }
        other => {
            return Err(ToolError::invalid(format!(
                "Unsupported background mode '{other}'."
            )));
        }
    }

    Ok(out)
}

/// Resizes `bg` using aspect-fill ("cover") and center-crops to `(target_w, target_h)`.
fn cover_resize_rgba(
    bg: &DynamicImage,
    target_w: u32,
    target_h: u32,
) -> Result<RgbaImage, ToolError> {
    let bw = bg.width().max(1);
    let bh = bg.height().max(1);
    let scale = (target_w as f64 / bw as f64).max(target_h as f64 / bh as f64);
    let scaled_w = ((bw as f64 * scale).ceil() as u32).max(target_w);
    let scaled_h = ((bh as f64 * scale).ceil() as u32).max(target_h);

    let resized = simd_resize(bg, scaled_w, scaled_h, "lanczos3")?;
    let x_off = (scaled_w - target_w) / 2;
    let y_off = (scaled_h - target_h) / 2;
    Ok(resized
        .crop_imm(x_off, y_off, target_w, target_h)
        .to_rgba8())
}

/// Fast separable 1D horizontal + 1D vertical Gaussian blur over RGBA8 pixels.
pub fn separable_gaussian_blur_rgba(src: &RgbaImage, sigma: f32) -> RgbaImage {
    let w = src.width() as usize;
    let h = src.height() as usize;
    if w == 0 || h == 0 || sigma <= 0.2 {
        return src.clone();
    }

    let radius = ((sigma * 2.5).ceil() as usize).clamp(1, 96);
    let mut kernel = Vec::with_capacity(2 * radius + 1);
    let two_sigma_sq = 2.0 * sigma * sigma;
    let mut k_sum = 0.0f32;
    for i in 0..=(2 * radius) {
        let d = i as f32 - radius as f32;
        let kv = (-d * d / two_sigma_sq).exp();
        kernel.push(kv);
        k_sum += kv;
    }
    for kv in &mut kernel {
        *kv /= k_sum;
    }

    let raw = src.as_raw();
    let mut horiz = vec![0.0f32; w * h * 3];

    // Horizontal pass
    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let mut r_acc = 0.0f32;
            let mut g_acc = 0.0f32;
            let mut b_acc = 0.0f32;
            for (ki, &kw) in kernel.iter().enumerate() {
                let sx = (x as isize + ki as isize - radius as isize).clamp(0, (w - 1) as isize)
                    as usize;
                let off = (row + sx) * 4;
                r_acc += raw[off] as f32 * kw;
                g_acc += raw[off + 1] as f32 * kw;
                b_acc += raw[off + 2] as f32 * kw;
            }
            let dst_off = (row + x) * 3;
            horiz[dst_off] = r_acc;
            horiz[dst_off + 1] = g_acc;
            horiz[dst_off + 2] = b_acc;
        }
    }

    // Vertical pass
    let mut out = RgbaImage::new(w as u32, h as u32);
    let out_raw = out.as_mut();
    for y in 0..h {
        for x in 0..w {
            let mut r_acc = 0.0f32;
            let mut g_acc = 0.0f32;
            let mut b_acc = 0.0f32;
            for (ki, &kw) in kernel.iter().enumerate() {
                let sy = (y as isize + ki as isize - radius as isize).clamp(0, (h - 1) as isize)
                    as usize;
                let off = (sy * w + x) * 3;
                r_acc += horiz[off] * kw;
                g_acc += horiz[off + 1] * kw;
                b_acc += horiz[off + 2] * kw;
            }
            let dst_off = (y * w + x) * 4;
            out_raw[dst_off] = r_acc.round().clamp(0.0, 255.0) as u8;
            out_raw[dst_off + 1] = g_acc.round().clamp(0.0, 255.0) as u8;
            out_raw[dst_off + 2] = b_acc.round().clamp(0.0, 255.0) as u8;
            out_raw[dst_off + 3] = 255;
        }
    }

    out
}

pub fn parse_hex_rgb(input: &str) -> Result<[u8; 3], ToolError> {
    let hex = input.trim().trim_start_matches('#');
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).map_err(|_| invalid_hex(input))?;
            let g = u8::from_str_radix(&hex[1..2], 16).map_err(|_| invalid_hex(input))?;
            let b = u8::from_str_radix(&hex[2..3], 16).map_err(|_| invalid_hex(input))?;
            Ok([r * 17, g * 17, b * 17])
        }
        6 | 8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| invalid_hex(input))?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| invalid_hex(input))?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| invalid_hex(input))?;
            Ok([r, g, b])
        }
        _ => Err(invalid_hex(input)),
    }
}

fn invalid_hex(raw: &str) -> ToolError {
    ToolError::invalid(format!(
        "Invalid hex color '{raw}'. Expected #RRGGBB (for example #FFFFFF)."
    ))
}
