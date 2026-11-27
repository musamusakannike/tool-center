//! AI Image Upscaling (`ai.upscale`): Real-ESRGAN 2x & 4x super-resolution on ONNX Runtime with
//! automatic RAM-based tile sizing, divisible-by-4 border padding, raised-cosine feathered seam
//! blending, alpha preservation, and a fast non-AI SIMD Lanczos3 fallback when no model is installed.

use crate::downloader::AiRuntimeHub;
use crate::manifest::{format_human_bytes, AiTask};
use crate::runtime::OrtSessionHandle;
use core_image::{encode_image_to_path, load_image_any, simd_resize, IMAGE_INPUT_EXTENSIONS};
use core_jobs::{
    validate_inputs_against_spec, CancelHandle, OptionField, OptionKind, OutputArtifact,
    SelectChoice, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError, ToolInputSpec,
    ToolOptions, ToolRunResult,
};
use image::{DynamicImage, Rgba, RgbaImage};
use serde_json::json;
use std::f32::consts::PI;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

pub const LANCZOS_FALLBACK_ID: &str = "lanczos_fallback";

pub struct AiUpscaleTool {
    hub: Arc<AiRuntimeHub>,
}

impl AiUpscaleTool {
    pub fn new(hub: Arc<AiRuntimeHub>) -> Self {
        Self { hub }
    }

    fn model_choices(&self) -> (String, Vec<SelectChoice>) {
        let mut default_id = "upscale.realesrgan_x2plus".to_string();
        let mut choices = Vec::new();
        if let Ok(mgr) = self.hub.manager() {
            for m in mgr.list_models() {
                if m.spec.task == AiTask::Upscale {
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
                value: "upscale.realesrgan_x2plus".into(),
                label: "Real-ESRGAN x2plus (64.0 MB)".into(),
                description: Some("2x general photo super-resolution".into()),
            });
        }
        // Always offer the instant zero-download SIMD Lanczos3 fallback
        choices.push(SelectChoice {
            value: LANCZOS_FALLBACK_ID.into(),
            label: "Lanczos3 Sharp Resize (Instant • No AI Model Required)".into(),
            description: Some(
                "Fast offline non-AI SIMD resampling when no AI model is installed.".into(),
            ),
        });
        (default_id, choices)
    }
}

impl Tool for AiUpscaleTool {
    fn descriptor(&self) -> ToolDescriptor {
        let (default_model, model_choices) = self.model_choices();

        ToolDescriptor {
            id: "ai.upscale".to_string(),
            name: "Upscale Image (AI)".to_string(),
            name_key: "tool_ai_upscale".to_string(),
            description: "Enlarge images 2× or 4× with Real-ESRGAN tiled super-resolution and seamless feathered blending (or instant Lanczos3 fallback)."
                .to_string(),
            description_key: "tool_ai_upscale_desc".to_string(),
            category: ToolCategory::Ai,
            icon: "upscale".to_string(),
            keywords: vec![
                "ai".into(),
                "upscale".into(),
                "super resolution".into(),
                "enlarge".into(),
                "enhance".into(),
                "esrgan".into(),
                "2x".into(),
                "4x".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            default_suffix: "upscaled".to_string(),
            options_schema: vec![
                OptionField {
                    key: "scale".into(),
                    label: "Upscale Factor".into(),
                    label_key: "opt_ai_upscale_scale".into(),
                    hint: "2× doubles width & height (4× pixels); 4× quadruples width & height (16× pixels)."
                        .into(),
                    hint_key: "opt_ai_upscale_scale_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "2".into(),
                                label: "2× Enlarge".into(),
                                description: Some("Doubles width & height".into()),
                            },
                            SelectChoice {
                                value: "4".into(),
                                label: "4× Enlarge".into(),
                                description: Some("Quadruples width & height".into()),
                            },
                        ],
                    },
                    default_value: json!("2"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "model_id".into(),
                    label: "Upscaling Engine / Model".into(),
                    label_key: "opt_ai_model".into(),
                    hint: "Choose a Real-ESRGAN neural model (downloaded once) or Lanczos3 instant non-AI resize."
                        .into(),
                    hint_key: "opt_ai_upscale_model_hint".into(),
                    kind: OptionKind::Select {
                        choices: model_choices,
                    },
                    default_value: json!(default_model),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "tile_size".into(),
                    label: "Tile Size (RAM Budget)".into(),
                    label_key: "opt_ai_tile_size".into(),
                    hint: "Smaller tiles use less RAM/VRAM; overlapping edges are feathered automatically to eliminate seams."
                        .into(),
                    hint_key: "opt_ai_tile_size_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "auto".into(),
                                label: format!("Auto ({} px based on system RAM)", select_auto_tile_size()),
                                description: Some("Recommended automatic tile sizing".into()),
                            },
                            SelectChoice {
                                value: "128".into(),
                                label: "128 px (Low Memory)".into(),
                                description: Some("Minimal RAM/VRAM footprint".into()),
                            },
                            SelectChoice {
                                value: "256".into(),
                                label: "256 px (Balanced)".into(),
                                description: Some("Good balance of speed and memory".into()),
                            },
                            SelectChoice {
                                value: "384".into(),
                                label: "384 px (High Memory)".into(),
                                description: Some("Fewer tiles on high-RAM desktops".into()),
                            },
                            SelectChoice {
                                value: "512".into(),
                                label: "512 px (Maximum)".into(),
                                description: Some("Fastest on powerful GPUs".into()),
                            },
                        ],
                    },
                    default_value: json!("auto"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "output_format".into(),
                    label: "Output Format".into(),
                    label_key: "opt_output_format".into(),
                    hint: "PNG is lossless and preserves transparency; WebP and JPG produce smaller files."
                        .into(),
                    hint_key: "opt_ai_upscale_fmt_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "png".into(),
                                label: "PNG (Lossless)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "webp".into(),
                                label: "WebP".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "jpg".into(),
                                label: "JPG (92% Quality)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("png"),
                    required: true,
                    visible_when: None,
                },
            ],
        }
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        let desc = self.descriptor();
        validate_inputs_against_spec(inputs, &desc.input_spec)?;

        let scale_str = options.get_string_or("scale", "2");
        if scale_str != "2" && scale_str != "4" {
            return Err(ToolError::invalid(format!(
                "Unsupported upscale factor '{scale_str}'. Expected '2' or '4'."
            )));
        }

        let fmt = options.get_string_or("output_format", "png");
        if !matches!(fmt.as_str(), "png" | "webp" | "jpg") {
            return Err(ToolError::invalid(format!(
                "Unsupported output format '{fmt}'."
            )));
        }
        Ok(())
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        ctx.check_cancelled()?;
        let target_scale: u32 = match ctx.options.get_string_or("scale", "2").as_str() {
            "4" => 4,
            _ => 2,
        };
        let default_model = self.model_choices().0;
        let model_id = ctx.options.get_string_or("model_id", &default_model);
        let tile_opt = ctx.options.get_string_or("tile_size", "auto");
        let tile_size: u32 = match tile_opt.as_str() {
            "auto" => select_auto_tile_size(),
            raw => raw
                .parse::<u32>()
                .map(|v| v.clamp(32, 1024))
                .unwrap_or_else(|_| select_auto_tile_size()),
        };
        let out_ext = ctx.options.get_string_or("output_format", "png");

        let is_lanczos = model_id == LANCZOS_FALLBACK_ID;
        let mut warnings = Vec::new();

        let loaded = if is_lanczos {
            None
        } else {
            ctx.report_progress(0.03, format!("Loading upscaler '{model_id}'..."));
            let (spec, session) = self.hub.load_role_session(&model_id, "primary")?;
            if session.provider_info().fell_back_to_cpu {
                if let Some(ref reason) = session.provider_info().fallback_reason {
                    warnings.push(reason.clone());
                }
            }
            Some((spec, session))
        };

        let active_provider = loaded
            .as_ref()
            .map(|(_, s)| s.provider_info().active_provider.clone())
            .unwrap_or_else(|| "simd-cpu".to_string());
        let engine_label = loaded
            .as_ref()
            .map(|(spec, _)| spec.display_name.clone())
            .unwrap_or_else(|| "Lanczos3 (Non-AI Fallback)".to_string());

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
                base_p + span_p * 0.05,
                format!("({}/{}) Decoding {file_name}...", idx + 1, total),
            );
            let orig_img = load_image_any(input_path)?;
            let orig_w = orig_img.width();
            let orig_h = orig_img.height();
            let dst_w = orig_w.saturating_mul(target_scale);
            let dst_h = orig_h.saturating_mul(target_scale);

            if (dst_w as u64) * (dst_h as u64) > 140_000_000 {
                return Err(ToolError::invalid(format!(
                    "Requested {target_scale}× output ({dst_w}×{dst_h}, {:.1} MP) exceeds safe desktop image limits (140 MP). Choose 2× or resize first.",
                    ((dst_w as f64) * (dst_h as f64)) / 1_000_000.0
                )));
            }
            if (dst_w as u64) * (dst_h as u64) >= 25_000_000 {
                warnings.push(format!(
                    "Large output produced for '{file_name}': {dst_w}×{dst_h} px ({:.1} MP).",
                    ((dst_w as f64) * (dst_h as f64)) / 1_000_000.0
                ));
            }

            let final_img = if let Some((ref spec, ref session)) = loaded {
                let model_native_scale = spec.scale.unwrap_or(2).clamp(2, 4);
                let overlap = (tile_size / 8).clamp(8, 24);

                let upscaled_rgba = if model_native_scale == 2 && target_scale == 4 {
                    // Two-pass 2× -> 4× tiled super-resolution when user selected 4× with an x2 model
                    let mid = run_tiled_super_resolution(
                        &orig_img.to_rgba8(),
                        session,
                        2,
                        tile_size,
                        overlap,
                        &ctx.cancel,
                        |tp, msg| {
                            ctx.report_progress(
                                base_p + span_p * (0.08 + tp * 0.42),
                                format!("({}/{} • Pass 1/2) {msg}", idx + 1, total),
                            );
                        },
                    )?;
                    run_tiled_super_resolution(
                        &mid,
                        session,
                        2,
                        tile_size,
                        overlap,
                        &ctx.cancel,
                        |tp, msg| {
                            ctx.report_progress(
                                base_p + span_p * (0.50 + tp * 0.42),
                                format!("({}/{} • Pass 2/2) {msg}", idx + 1, total),
                            );
                        },
                    )?
                } else {
                    let native_out = run_tiled_super_resolution(
                        &orig_img.to_rgba8(),
                        session,
                        model_native_scale,
                        tile_size,
                        overlap,
                        &ctx.cancel,
                        |tp, msg| {
                            ctx.report_progress(
                                base_p + span_p * (0.08 + tp * 0.82),
                                format!("({}/{}) {msg}", idx + 1, total),
                            );
                        },
                    )?;
                    if model_native_scale != target_scale {
                        simd_resize(
                            &DynamicImage::ImageRgba8(native_out),
                            dst_w,
                            dst_h,
                            "lanczos3",
                        )?
                        .to_rgba8()
                    } else {
                        native_out
                    }
                };
                DynamicImage::ImageRgba8(upscaled_rgba)
            } else {
                ctx.report_progress(
                    base_p + span_p * 0.45,
                    format!(
                        "({}/{}) Resampling {file_name} to {dst_w}×{dst_h} (Lanczos3)...",
                        idx + 1,
                        total
                    ),
                );
                simd_resize(&orig_img, dst_w, dst_h, "lanczos3")?
            };

            ctx.check_cancelled()?;
            ctx.report_progress(
                base_p + span_p * 0.92,
                format!("({}/{}) Encoding {out_ext} output...", idx + 1, total),
            );
            let suffix = format!("{target_scale}x");
            let out_path = ctx.output_path_for(input_path, &suffix, Some(&out_ext));
            let size_bytes = encode_image_to_path(&final_img, &out_path, &out_ext, 92, false)?;

            let orig_size = fs::metadata(input_path).map(|m| m.len()).ok();
            let label = out_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("upscaled")
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
                "input_width": orig_w,
                "input_height": orig_h,
                "output_width": dst_w,
                "output_height": dst_h,
                "scale": target_scale,
                "tile_size": tile_size,
            }));
        }

        let summary = format!(
            "Upscaled {} image{} ({target_scale}×) with {} ({})",
            outputs.len(),
            if outputs.len() == 1 { "" } else { "s" },
            engine_label,
            active_provider.to_uppercase()
        );
        let mut result = ToolRunResult::with_outputs(summary, outputs);
        result.warnings = warnings;
        result.report_json = Some(json!({
            "tool_kind": "ai.upscale",
            "model_id": model_id,
            "model_name": engine_label,
            "active_provider": active_provider,
            "scale": target_scale,
            "tile_size": tile_size,
            "items": report_items,
        }));

        ctx.report_progress(1.0, "Upscaling complete.");
        Ok(result)
    }
}

/// Automatically chooses a safe tile size (`160`, `256`, or `384`) from available system RAM.
pub fn select_auto_tile_size() -> u32 {
    let avail_mib = detect_available_ram_mib().unwrap_or(8192);
    if avail_mib >= 12_288 {
        384
    } else if avail_mib >= 5_120 {
        256
    } else {
        160
    }
}

fn detect_available_ram_mib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemAvailable:") {
                    let kb_str = rest.split_whitespace().next()?;
                    let kb = kb_str.parse::<u64>().ok()?;
                    return Some(kb / 1024);
                }
            }
        }
    }
    None
}

/// Runs tiled ONNX super-resolution with overlap context, divisible-by-4 edge padding, and
/// raised-cosine feather blending so tile boundaries are completely invisible.
pub fn run_tiled_super_resolution<F>(
    src_rgba: &RgbaImage,
    session: &OrtSessionHandle,
    scale: u32,
    tile_size: u32,
    overlap: u32,
    cancel: &CancelHandle,
    mut on_tile_progress: F,
) -> Result<RgbaImage, ToolError>
where
    F: FnMut(f32, String),
{
    let orig_w = src_rgba.width() as usize;
    let orig_h = src_rgba.height() as usize;
    let s = scale.max(1) as usize;
    let out_w = orig_w * s;
    let out_h = orig_h * s;

    let stride = tile_size.max(32) as usize;
    let ov = (overlap as usize).min(stride / 4).max(4);

    let tiles_x = orig_w.div_ceil(stride);
    let tiles_y = orig_h.div_ceil(stride);
    let total_tiles = (tiles_x * tiles_y).max(1);

    let mut acc_rgb = vec![0.0f32; out_w * out_h * 3];
    let mut acc_weight = vec![0.0f32; out_w * out_h];

    let src_raw = src_rgba.as_raw();
    let has_transparency = src_rgba.pixels().any(|p| p[3] < 255);

    let mut tile_idx = 0usize;
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            cancel.check()?;
            tile_idx += 1;
            on_tile_progress(
                (tile_idx - 1) as f32 / total_tiles as f32,
                format!("Upscaling tile {tile_idx}/{total_tiles}..."),
            );

            let wx0 = (tx * stride).saturating_sub(ov);
            let wx1 = ((tx + 1) * stride + ov).min(orig_w);
            let wy0 = (ty * stride).saturating_sub(ov);
            let wy1 = ((ty + 1) * stride + ov).min(orig_h);

            let win_w = wx1 - wx0;
            let win_h = wy1 - wy0;
            // Pad input tensor spatial dimensions to multiple of 4 (required by RRDB pixel-unshuffle)
            let pad_w = win_w.next_multiple_of(4);
            let pad_h = win_h.next_multiple_of(4);
            let in_plane = pad_w * pad_h;

            let mut tile_tensor = vec![0.0f32; 3 * in_plane];
            for ly in 0..pad_h {
                let sy = (wy0 + ly.min(win_h - 1)).min(orig_h - 1);
                let src_row = sy * orig_w;
                let dst_row = ly * pad_w;
                for lx in 0..pad_w {
                    let sx = (wx0 + lx.min(win_w - 1)).min(orig_w - 1);
                    let src_off = (src_row + sx) * 4;
                    let t_idx = dst_row + lx;
                    tile_tensor[t_idx] = src_raw[src_off] as f32 / 255.0;
                    tile_tensor[in_plane + t_idx] = src_raw[src_off + 1] as f32 / 255.0;
                    tile_tensor[2 * in_plane + t_idx] = src_raw[src_off + 2] as f32 / 255.0;
                }
            }

            let (pred_shape, pred_data) =
                session.run_f32_tensor(&[1, 3, pad_h, pad_w], &tile_tensor, cancel)?;

            let pred_h = pad_h * s;
            let pred_w = pad_w * s;
            let pred_plane = pred_h * pred_w;
            if pred_data.len() < 3 * pred_plane {
                return Err(ToolError::failed(
                    "Upscaler returned unexpected output tensor shape",
                    Some(format!(
                        "Expected [1, 3, {pred_h}, {pred_w}], got {pred_shape:?}"
                    )),
                ));
            }

            let valid_out_w = win_w * s;
            let valid_out_h = win_h * s;
            let ramp = (2 * ov * s).max(1);

            let has_left_neighbor = wx0 > 0;
            let has_right_neighbor = wx1 < orig_w;
            let has_top_neighbor = wy0 > 0;
            let has_bottom_neighbor = wy1 < orig_h;

            let x_weights =
                build_1d_feather_weights(valid_out_w, ramp, has_left_neighbor, has_right_neighbor);
            let y_weights =
                build_1d_feather_weights(valid_out_h, ramp, has_top_neighbor, has_bottom_neighbor);

            let dst_x0 = wx0 * s;
            let dst_y0 = wy0 * s;

            for (oy, &wy) in y_weights.iter().enumerate() {
                let pred_row = oy * pred_w;
                let dst_row = (dst_y0 + oy) * out_w;
                for (ox, &wx) in x_weights.iter().enumerate() {
                    let w = wx * wy;
                    let p_idx = pred_row + ox;
                    let r = pred_data[p_idx].clamp(0.0, 1.0);
                    let g = pred_data[pred_plane + p_idx].clamp(0.0, 1.0);
                    let b = pred_data[2 * pred_plane + p_idx].clamp(0.0, 1.0);

                    let d_idx = dst_row + dst_x0 + ox;
                    acc_weight[d_idx] += w;
                    let rgb_off = d_idx * 3;
                    acc_rgb[rgb_off] += r * w;
                    acc_rgb[rgb_off + 1] += g * w;
                    acc_rgb[rgb_off + 2] += b * w;
                }
            }
        }
    }

    let upscaled_alpha = if has_transparency {
        Some(
            simd_resize(
                &DynamicImage::ImageRgba8(src_rgba.clone()),
                out_w as u32,
                out_h as u32,
                "lanczos3",
            )?
            .to_rgba8(),
        )
    } else {
        None
    };

    let mut out_img = RgbaImage::new(out_w as u32, out_h as u32);
    for (i, px) in out_img.pixels_mut().enumerate() {
        let w = acc_weight[i].max(1e-6);
        let rgb_off = i * 3;
        let r = ((acc_rgb[rgb_off] / w) * 255.0).round().clamp(0.0, 255.0) as u8;
        let g = ((acc_rgb[rgb_off + 1] / w) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        let b = ((acc_rgb[rgb_off + 2] / w) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        let a = upscaled_alpha
            .as_ref()
            .map(|al| al.as_raw()[i * 4 + 3])
            .unwrap_or(255);
        *px = Rgba([r, g, b, a]);
    }

    Ok(out_img)
}

/// Computes raised-cosine 1D feather weights in `(0.0, 1.0]` across `len` pixels.
fn build_1d_feather_weights(
    len: usize,
    ramp: usize,
    feather_start: bool,
    feather_end: bool,
) -> Vec<f32> {
    let mut weights = vec![1.0f32; len];
    let eff_ramp = ramp.min(len / 2).max(1);
    if feather_start {
        for (i, w) in weights.iter_mut().take(eff_ramp).enumerate() {
            let t = (i as f32 + 0.5) / eff_ramp as f32;
            *w *= 0.5 - 0.5 * (PI * t).cos();
        }
    }
    if feather_end {
        for i in 0..eff_ramp {
            let t = (i as f32 + 0.5) / eff_ramp as f32;
            let fade = 0.5 - 0.5 * (PI * t).cos();
            weights[len - 1 - i] *= fade;
        }
    }
    weights
}
