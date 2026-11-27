use crate::codecs::{
    compute_target_dimensions, encode_image_to_path, normalize_output_extension, simd_resize,
    strip_metadata_preserving_pixels, IMAGE_INPUT_EXTENSIONS,
};
use crate::preview::load_image_any;
use crate::watermark::{apply_image_watermark, apply_text_watermark};
use core_jobs::{
    OptionField, OptionKind, OutputArtifact, SelectChoice, Tool, ToolCategory, ToolContext,
    ToolDescriptor, ToolError, ToolInputSpec, ToolOptions, ToolRegistry, ToolRunResult,
    VisibilityCondition,
};
use image::DynamicImage;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

/// Registers all Milestone 4 Image Tools into the shared `ToolRegistry`.
pub fn register_image_tools(registry: &ToolRegistry) {
    registry.register(ImageResizeTool);
    registry.register(ImageCropTool);
    registry.register(ImageRotateFlipTool);
    registry.register(ImageConvertTool);
    registry.register(ImageCompressTool);
    registry.register(ImageWatermarkTool);
    registry.register(ImageStripExifTool);
    registry.register(ImageBatchTool);
}

fn source_output_ext(path: &Path) -> &'static str {
    let raw = path.extension().and_then(|s| s.to_str()).unwrap_or("png");
    normalize_output_extension(raw)
}

fn build_artifact(input: &Path, out_path: &Path, size_bytes: u64) -> OutputArtifact {
    let orig_size = fs::metadata(input).map(|m| m.len()).ok();
    OutputArtifact {
        path: out_path.display().to_string(),
        label: out_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string(),
        size_bytes,
        original_size_bytes: orig_size,
    }
}

// ============================================================================
// 1. Resize Image (`image.resize`)
// ============================================================================
pub struct ImageResizeTool;

impl Tool for ImageResizeTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "image.resize".to_string(),
            name: "Resize Image".to_string(),
            name_key: "tool_image_resize".to_string(),
            description:
                "Scale images by pixel dimensions or percentage with sharp SIMD filtering."
                    .to_string(),
            description_key: "tool_image_resize_desc".to_string(),
            category: ToolCategory::Images,
            icon: "resize".to_string(),
            keywords: vec![
                "scale".into(),
                "dimensions".into(),
                "width".into(),
                "height".into(),
                "shrink".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "mode".into(),
                    label: "Resize Mode".into(),
                    label_key: "opt_resize_mode".into(),
                    hint: "Choose exact maximum pixel dimensions or a percentage scale.".into(),
                    hint_key: "opt_resize_mode_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "dimensions".into(),
                                label: "By Dimensions (px)".into(),
                                description: Some("Fit within width & height".into()),
                            },
                            SelectChoice {
                                value: "percentage".into(),
                                label: "By Percentage (%)".into(),
                                description: Some("Scale relative to current size".into()),
                            },
                        ],
                    },
                    default_value: json!("dimensions"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "width".into(),
                    label: "Width".into(),
                    label_key: "opt_width".into(),
                    hint: "Target width in pixels.".into(),
                    hint_key: "opt_width_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(1),
                        max: Some(32000),
                        step: Some(10),
                        unit: Some("px".into()),
                    },
                    default_value: json!(1920),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("dimensions"),
                    }),
                },
                OptionField {
                    key: "height".into(),
                    label: "Height".into(),
                    label_key: "opt_height".into(),
                    hint: "Target height in pixels.".into(),
                    hint_key: "opt_height_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(1),
                        max: Some(32000),
                        step: Some(10),
                        unit: Some("px".into()),
                    },
                    default_value: json!(1080),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("dimensions"),
                    }),
                },
                OptionField {
                    key: "keep_aspect".into(),
                    label: "Keep original proportions".into(),
                    label_key: "opt_keep_aspect".into(),
                    hint: "Prevents stretching by fitting cleanly inside the target box.".into(),
                    hint_key: "opt_keep_aspect_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(true),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("dimensions"),
                    }),
                },
                OptionField {
                    key: "scale_percent".into(),
                    label: "Scale Percentage".into(),
                    label_key: "opt_scale_percent".into(),
                    hint: "50% halves width and height; 200% doubles them.".into(),
                    hint_key: "opt_scale_percent_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(5),
                        max: Some(400),
                        step: Some(5),
                        unit: Some("%".into()),
                    },
                    default_value: json!(50),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("percentage"),
                    }),
                },
                OptionField {
                    key: "filter".into(),
                    label: "Resampling Quality".into(),
                    label_key: "opt_resize_filter".into(),
                    hint: "Lanczos3 preserves crispness for photos; Nearest keeps sharp pixel art."
                        .into(),
                    hint_key: "opt_resize_filter_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "lanczos3".into(),
                                label: "Lanczos3 (Best for Photos)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "bilinear".into(),
                                label: "Bilinear (Smooth & Fast)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "nearest".into(),
                                label: "Nearest Neighbor (Pixel Art)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("lanczos3"),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_resized".to_string(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let mode = ctx.options.get_string_or("mode", "dimensions");
        let filter = ctx.options.get_string_or("filter", "lanczos3");
        let mut outputs = Vec::with_capacity(ctx.inputs.len());

        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Resizing image {} of {}", i + 1, ctx.inputs.len()),
            );

            let img = load_image_any(input)?;
            let (dst_w, dst_h) = if mode == "percentage" {
                let pct = ctx.options.get_i64("scale_percent", 50).clamp(1, 1000) as f64 / 100.0;
                (
                    ((img.width() as f64 * pct).round() as u32).max(1),
                    ((img.height() as f64 * pct).round() as u32).max(1),
                )
            } else {
                let req_w = ctx.options.get_i64("width", 1920).max(1) as u32;
                let req_h = ctx.options.get_i64("height", 1080).max(1) as u32;
                let keep = ctx.options.get_bool("keep_aspect", true);
                compute_target_dimensions(img.width(), img.height(), req_w, req_h, keep)
            };

            let resized = simd_resize(&img, dst_w, dst_h, &filter)?;
            let out_ext = source_output_ext(input);
            let out_path = ctx.output_path_for(input, "_resized", Some(out_ext));
            let written = encode_image_to_path(&resized, &out_path, out_ext, 88, false)?;
            outputs.push(build_artifact(input, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Resized {} image(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 2. Crop Image (`image.crop`)
// ============================================================================
pub struct ImageCropTool;

impl Tool for ImageCropTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "image.crop".to_string(),
            name: "Crop Image".to_string(),
            name_key: "tool_image_crop".to_string(),
            description: "Crop to a standard aspect ratio or exact pixel rectangle.".to_string(),
            description_key: "tool_image_crop_desc".to_string(),
            category: ToolCategory::Images,
            icon: "crop".to_string(),
            keywords: vec![
                "trim".into(),
                "aspect".into(),
                "square".into(),
                "cut".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "mode".into(),
                    label: "Crop Method".into(),
                    label_key: "opt_crop_mode".into(),
                    hint: "Center-crop to a popular aspect ratio or enter exact pixel coordinates."
                        .into(),
                    hint_key: "opt_crop_mode_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "aspect".into(),
                                label: "Centered Aspect Ratio".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "pixels".into(),
                                label: "Exact Pixel Box (X, Y, Width, Height)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("aspect"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "aspect_ratio".into(),
                    label: "Aspect Ratio".into(),
                    label_key: "opt_aspect_ratio".into(),
                    hint: "Automatically centers the largest matching crop rectangle.".into(),
                    hint_key: "opt_aspect_ratio_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "1:1".into(),
                                label: "1:1 Square".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "16:9".into(),
                                label: "16:9 Widescreen".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "4:3".into(),
                                label: "4:3 Classic".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "3:2".into(),
                                label: "3:2 Camera Photo".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "9:16".into(),
                                label: "9:16 Portrait / Vertical".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("1:1"),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("aspect"),
                    }),
                },
                OptionField {
                    key: "x".into(),
                    label: "Left Offset (X)".into(),
                    label_key: "opt_crop_x".into(),
                    hint: "Pixels from the left edge.".into(),
                    hint_key: "opt_crop_x_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(0),
                        max: Some(32000),
                        step: Some(1),
                        unit: Some("px".into()),
                    },
                    default_value: json!(0),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("pixels"),
                    }),
                },
                OptionField {
                    key: "y".into(),
                    label: "Top Offset (Y)".into(),
                    label_key: "opt_crop_y".into(),
                    hint: "Pixels from the top edge.".into(),
                    hint_key: "opt_crop_y_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(0),
                        max: Some(32000),
                        step: Some(1),
                        unit: Some("px".into()),
                    },
                    default_value: json!(0),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("pixels"),
                    }),
                },
                OptionField {
                    key: "width".into(),
                    label: "Crop Width".into(),
                    label_key: "opt_crop_w".into(),
                    hint: "Width of the cropped area in pixels.".into(),
                    hint_key: "opt_crop_w_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(1),
                        max: Some(32000),
                        step: Some(1),
                        unit: Some("px".into()),
                    },
                    default_value: json!(512),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("pixels"),
                    }),
                },
                OptionField {
                    key: "height".into(),
                    label: "Crop Height".into(),
                    label_key: "opt_crop_h".into(),
                    hint: "Height of the cropped area in pixels.".into(),
                    hint_key: "opt_crop_h_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(1),
                        max: Some(32000),
                        step: Some(1),
                        unit: Some("px".into()),
                    },
                    default_value: json!(512),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("pixels"),
                    }),
                },
            ],
            default_suffix: "_cropped".to_string(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let mode = ctx.options.get_string_or("mode", "aspect");
        let mut outputs = Vec::with_capacity(ctx.inputs.len());

        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Cropping image {} of {}", i + 1, ctx.inputs.len()),
            );

            let img = load_image_any(input)?;
            let (iw, ih) = (img.width(), img.height());
            let (cx, cy, cw, ch) = if mode == "pixels" {
                let x = (ctx.options.get_i64("x", 0).max(0) as u32).min(iw.saturating_sub(1));
                let y = (ctx.options.get_i64("y", 0).max(0) as u32).min(ih.saturating_sub(1));
                let w = (ctx.options.get_i64("width", iw as i64).max(1) as u32).min(iw - x);
                let h = (ctx.options.get_i64("height", ih as i64).max(1) as u32).min(ih - y);
                (x, y, w, h)
            } else {
                let ratio_str = ctx.options.get_string_or("aspect_ratio", "1:1");
                let (rw, rh) = match ratio_str.as_str() {
                    "16:9" => (16.0, 9.0),
                    "4:3" => (4.0, 3.0),
                    "3:2" => (3.0, 2.0),
                    "9:16" => (9.0, 16.0),
                    _ => (1.0, 1.0),
                };
                let target_ratio = rw / rh;
                let current_ratio = iw as f64 / ih.max(1) as f64;
                let (w, h) = if current_ratio > target_ratio {
                    (((ih as f64 * target_ratio).round() as u32).max(1), ih)
                } else {
                    (iw, ((iw as f64 / target_ratio).round() as u32).max(1))
                };
                ((iw - w) / 2, (ih - h) / 2, w, h)
            };

            let cropped = img.crop_imm(cx, cy, cw.max(1), ch.max(1));
            let out_ext = source_output_ext(input);
            let out_path = ctx.output_path_for(input, "_cropped", Some(out_ext));
            let written = encode_image_to_path(&cropped, &out_path, out_ext, 90, false)?;
            outputs.push(build_artifact(input, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Cropped {} image(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 3. Rotate & Flip Image (`image.rotate_flip`)
// ============================================================================
pub struct ImageRotateFlipTool;

impl Tool for ImageRotateFlipTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "image.rotate_flip".to_string(),
            name: "Rotate & Flip Image".to_string(),
            name_key: "tool_image_rotate_flip".to_string(),
            description: "Rotate 90°, 180°, or 270° and mirror horizontally or vertically."
                .to_string(),
            description_key: "tool_image_rotate_flip_desc".to_string(),
            category: ToolCategory::Images,
            icon: "rotate".to_string(),
            keywords: vec!["mirror".into(), "turn".into(), "orientation".into()],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "rotation".into(),
                    label: "Rotation Angle".into(),
                    label_key: "opt_rotation".into(),
                    hint: "Clockwise rotation to apply.".into(),
                    hint_key: "opt_rotation_hint".into(),
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
                            SelectChoice {
                                value: "0".into(),
                                label: "No Rotation (Flip Only)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("90"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "flip_horizontal".into(),
                    label: "Flip Horizontally (Left ⇄ Right)".into(),
                    label_key: "opt_flip_h".into(),
                    hint: "Mirror the image from left to right.".into(),
                    hint_key: "opt_flip_h_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(false),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "flip_vertical".into(),
                    label: "Flip Vertically (Top ⇅ Bottom)".into(),
                    label_key: "opt_flip_v".into(),
                    hint: "Mirror the image from top to bottom.".into(),
                    hint_key: "opt_flip_v_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(false),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_rotated".to_string(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let rot = ctx.options.get_string_or("rotation", "90");
        let flip_h = ctx.options.get_bool("flip_horizontal", false);
        let flip_v = ctx.options.get_bool("flip_vertical", false);
        let mut outputs = Vec::with_capacity(ctx.inputs.len());

        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Transforming image {} of {}", i + 1, ctx.inputs.len()),
            );

            let mut img = load_image_any(input)?;
            img = match rot.as_str() {
                "90" => img.rotate90(),
                "180" => img.rotate180(),
                "270" => img.rotate270(),
                _ => img,
            };
            if flip_h {
                img = img.fliph();
            }
            if flip_v {
                img = img.flipv();
            }

            let out_ext = source_output_ext(input);
            let out_path = ctx.output_path_for(input, "_rotated", Some(out_ext));
            let written = encode_image_to_path(&img, &out_path, out_ext, 90, false)?;
            outputs.push(build_artifact(input, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Transformed {} image(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 4. Convert Image (`image.convert`)
// ============================================================================
pub struct ImageConvertTool;

impl Tool for ImageConvertTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "image.convert".to_string(),
            name: "Convert Image".to_string(),
            name_key: "tool_image_convert".to_string(),
            description:
                "Convert images (including HEIC, AVIF, WebP, TIFF) to PNG, JPG, WebP, AVIF, TIFF, BMP, or GIF."
                    .to_string(),
            description_key: "tool_image_convert_desc".to_string(),
            category: ToolCategory::Images,
            icon: "convert_image".to_string(),
            keywords: vec![
                "heic".into(),
                "avif".into(),
                "webp".into(),
                "png".into(),
                "jpg".into(),
                "format".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "format".into(),
                    label: "Output Format".into(),
                    label_key: "opt_image_format".into(),
                    hint: "Target file format for converted images.".into(),
                    hint_key: "opt_image_format_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "png".into(),
                                label: "PNG (Lossless + Transparency)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "jpg".into(),
                                label: "JPG (Small Photo File)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "webp".into(),
                                label: "WebP (Modern Web)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "avif".into(),
                                label: "AVIF (High Efficiency)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "tiff".into(),
                                label: "TIFF (Print / Archival)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "bmp".into(),
                                label: "BMP (Uncompressed Bitmap)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "gif".into(),
                                label: "GIF (256-Color Static)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("png"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "quality".into(),
                    label: "Quality".into(),
                    label_key: "opt_quality".into(),
                    hint: "Higher values preserve more detail; 80–90 is visually near-lossless."
                        .into(),
                    hint_key: "opt_quality_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(10),
                        max: Some(100),
                        step: Some(5),
                        unit: Some("%".into()),
                    },
                    default_value: json!(85),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_converted".to_string(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let fmt = normalize_output_extension(&ctx.options.get_string_or("format", "png"));
        let quality = ctx.options.get_i64("quality", 85).clamp(1, 100) as u8;
        let mut outputs = Vec::with_capacity(ctx.inputs.len());

        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!(
                    "Converting image {} of {} to {}",
                    i + 1,
                    ctx.inputs.len(),
                    fmt.to_uppercase()
                ),
            );

            let img = load_image_any(input)?;
            let out_path = ctx.output_path_for(input, "_converted", Some(fmt));
            let written = encode_image_to_path(&img, &out_path, fmt, quality, true)?;
            outputs.push(build_artifact(input, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!(
                "Converted {} image(s) to {}.",
                outputs.len(),
                fmt.to_uppercase()
            ),
            outputs,
        ))
    }
}

// ============================================================================
// 5. Compress Image (`image.compress`)
// ============================================================================
pub struct ImageCompressTool;

impl Tool for ImageCompressTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "image.compress".to_string(),
            name: "Compress Image".to_string(),
            name_key: "tool_image_compress".to_string(),
            description:
                "Shrink PNG (OxiPNG), JPG (MozJPEG trellis), WebP, and AVIF files with minimal visual loss."
                    .to_string(),
            description_key: "tool_image_compress_desc".to_string(),
            category: ToolCategory::Images,
            icon: "compress".to_string(),
            keywords: vec![
                "optimize".into(),
                "shrink".into(),
                "oxipng".into(),
                "mozjpeg".into(),
                "size".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![OptionField {
                key: "quality".into(),
                label: "Compression Quality".into(),
                label_key: "opt_compress_quality".into(),
                hint:
                    "75–85 achieves strong size savings for JPG/WebP/AVIF; PNG optimization is always lossless."
                        .into(),
                hint_key: "opt_compress_quality_hint".into(),
                kind: OptionKind::Integer {
                    min: Some(20),
                    max: Some(98),
                    step: Some(2),
                    unit: Some("%".into()),
                },
                default_value: json!(78),
                required: false,
                visible_when: None,
            }],
            default_suffix: "_compressed".to_string(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let quality = ctx.options.get_i64("quality", 78).clamp(10, 100) as u8;
        let mut outputs = Vec::with_capacity(ctx.inputs.len());

        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Compressing image {} of {}", i + 1, ctx.inputs.len()),
            );

            let in_ext = input
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            let out_ext = match in_ext.as_str() {
                "png" => "png",
                "webp" => "webp",
                "avif" => "avif",
                _ => "jpg",
            };
            let out_path = ctx.output_path_for(input, "_compressed", Some(out_ext));

            let written = if in_ext == "png" {
                let raw = fs::read(input).map_err(|e| {
                    ToolError::invalid(format!("Cannot read '{}': {e}", input.display()))
                })?;
                let mut opts = oxipng::Options::from_preset(3);
                opts.strip = oxipng::StripChunks::Safe;
                let optimized = oxipng::optimize_from_memory(&raw, &opts).map_err(|e| {
                    ToolError::corrupt(input, "Invalid PNG file.", Some(e.to_string()))
                })?;
                fs::write(&out_path, &optimized).map_err(|e| {
                    ToolError::failed("Failed to write compressed PNG", Some(e.to_string()))
                })?;
                optimized.len() as u64
            } else {
                let img = load_image_any(input)?;
                encode_image_to_path(&img, &out_path, out_ext, quality, true)?
            };

            outputs.push(build_artifact(input, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Compressed {} image(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 6. Watermark Image (`image.watermark`)
// ============================================================================
pub struct ImageWatermarkTool;

impl Tool for ImageWatermarkTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "image.watermark".to_string(),
            name: "Watermark Image".to_string(),
            name_key: "tool_image_watermark".to_string(),
            description:
                "Add a text badge or logo image overlay with adjustable position and opacity."
                    .to_string(),
            description_key: "tool_image_watermark_desc".to_string(),
            category: ToolCategory::Images,
            icon: "watermark".to_string(),
            keywords: vec![
                "logo".into(),
                "copyright".into(),
                "stamp".into(),
                "overlay".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "watermark_type".into(),
                    label: "Watermark Type".into(),
                    label_key: "opt_wm_type".into(),
                    hint: "Stamp custom text or overlay a transparent PNG/logo image.".into(),
                    hint_key: "opt_wm_type_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "text".into(),
                                label: "Text Label".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "image".into(),
                                label: "Logo / Image Overlay".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("text"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "text".into(),
                    label: "Watermark Text".into(),
                    label_key: "opt_wm_text".into(),
                    hint: "Short copyright or brand text to stamp onto each image.".into(),
                    hint_key: "opt_wm_text_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("© 2026 Studio Name".into()),
                        secret: false,
                    },
                    default_value: json!("CONFIDENTIAL"),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "watermark_type".into(),
                        equals_value: json!("text"),
                    }),
                },
                OptionField {
                    key: "overlay_path".into(),
                    label: "Watermark Image File".into(),
                    label_key: "opt_wm_image_path".into(),
                    hint: "Choose a PNG, WebP, or JPG logo to composite.".into(),
                    hint_key: "opt_wm_image_path_hint".into(),
                    kind: OptionKind::FilePath {
                        accepted_extensions: vec![
                            "png".into(),
                            "webp".into(),
                            "jpg".into(),
                            "jpeg".into(),
                        ],
                    },
                    default_value: json!(""),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "watermark_type".into(),
                        equals_value: json!("image"),
                    }),
                },
                OptionField {
                    key: "position".into(),
                    label: "Position".into(),
                    label_key: "opt_wm_position".into(),
                    hint: "Anchor corner or center where the watermark sits.".into(),
                    hint_key: "opt_wm_position_hint".into(),
                    kind: OptionKind::PositionGrid,
                    default_value: json!("bottom_right"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "opacity".into(),
                    label: "Opacity".into(),
                    label_key: "opt_wm_opacity".into(),
                    hint: "Lower values make the watermark more subtle and translucent.".into(),
                    hint_key: "opt_wm_opacity_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(10),
                        max: Some(100),
                        step: Some(5),
                        unit: Some("%".into()),
                    },
                    default_value: json!(75),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "scale".into(),
                    label: "Size".into(),
                    label_key: "opt_wm_scale".into(),
                    hint: "Relative watermark size compared to the image.".into(),
                    hint_key: "opt_wm_scale_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(10),
                        max: Some(80),
                        step: Some(5),
                        unit: Some("%".into()),
                    },
                    default_value: json!(25),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_watermarked".to_string(),
        }
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        let desc = self.descriptor();
        core_jobs::validate_inputs_against_spec(inputs, &desc.input_spec)?;
        let wm_type = options.get_string_or("watermark_type", "text");
        if wm_type == "image" {
            let overlay = options.get_string_or("overlay_path", "");
            if overlay.trim().is_empty() || !Path::new(&overlay).is_file() {
                return Err(ToolError::invalid(
                    "Please choose a valid watermark logo image file.",
                ));
            }
        } else if options.get_string_or("text", "").trim().is_empty() {
            return Err(ToolError::invalid("Please enter watermark text."));
        }
        Ok(())
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let wm_type = ctx.options.get_string_or("watermark_type", "text");
        let position = ctx.options.get_string_or("position", "bottom_right");
        let opacity = ctx.options.get_i64("opacity", 75).clamp(5, 100) as u32;
        let scale = ctx.options.get_i64("scale", 25).clamp(5, 90) as u32;

        let overlay_img = if wm_type == "image" {
            let p = PathBuf::from(ctx.options.get_string_or("overlay_path", ""));
            Some(load_image_any(&p)?)
        } else {
            None
        };
        let text = ctx.options.get_string_or("text", "CONFIDENTIAL");

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Watermarking image {} of {}", i + 1, ctx.inputs.len()),
            );

            let mut rgba = load_image_any(input)?.to_rgba8();
            if let Some(ref ov) = overlay_img {
                apply_image_watermark(&mut rgba, ov, &position, scale, opacity)?;
            } else {
                apply_text_watermark(&mut rgba, &text, &position, scale, opacity)?;
            }

            let out_ext = source_output_ext(input);
            let out_path = ctx.output_path_for(input, "_watermarked", Some(out_ext));
            let dyn_out = DynamicImage::ImageRgba8(rgba);
            let written = encode_image_to_path(&dyn_out, &out_path, out_ext, 90, false)?;
            outputs.push(build_artifact(input, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Watermarked {} image(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 7. Strip EXIF & Metadata (`image.strip_exif`)
// ============================================================================
pub struct ImageStripExifTool;

impl Tool for ImageStripExifTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "image.strip_exif".to_string(),
            name: "Strip EXIF Metadata".to_string(),
            name_key: "tool_image_strip_exif".to_string(),
            description:
                "Remove GPS location, camera serials, timestamps, and comments without re-encoding JPG/PNG/WebP pixels."
                    .to_string(),
            description_key: "tool_image_strip_exif_desc".to_string(),
            category: ToolCategory::Images,
            icon: "exif".to_string(),
            keywords: vec![
                "privacy".into(),
                "gps".into(),
                "metadata".into(),
                "exif".into(),
                "clean".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![],
            default_suffix: "_clean".to_string(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Stripping metadata {} of {}", i + 1, ctx.inputs.len()),
            );

            let out_ext = source_output_ext(input);
            let out_path = ctx.output_path_for(input, "_clean", Some(out_ext));
            let written = strip_metadata_preserving_pixels(input, &out_path)?;
            outputs.push(build_artifact(input, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Removed metadata from {} image(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 8. Batch Image Processor (`image.batch`)
// ============================================================================
pub struct ImageBatchTool;

impl Tool for ImageBatchTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "image.batch".to_string(),
            name: "Batch Process Images".to_string(),
            name_key: "tool_image_batch".to_string(),
            description:
                "Resize, strip metadata, convert format, and compress many images in one fast pass."
                    .to_string(),
            description_key: "tool_image_batch_desc".to_string(),
            category: ToolCategory::Images,
            icon: "batch".to_string(),
            keywords: vec!["bulk".into(), "pipeline".into(), "web".into(), "export".into()],
            input_spec: ToolInputSpec::single_or_batch(IMAGE_INPUT_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "max_dimension".into(),
                    label: "Max Width / Height (0 = Keep Original Size)".into(),
                    label_key: "opt_batch_max_dim".into(),
                    hint: "Downscales oversized images proportionally; smaller images stay untouched."
                        .into(),
                    hint_key: "opt_batch_max_dim_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(0),
                        max: Some(16000),
                        step: Some(100),
                        unit: Some("px".into()),
                    },
                    default_value: json!(1920),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "format".into(),
                    label: "Output Format".into(),
                    label_key: "opt_batch_format".into(),
                    hint: "Keep each file's current format or unify all outputs.".into(),
                    hint_key: "opt_batch_format_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "keep".into(),
                                label: "Keep Original Format".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "jpg".into(),
                                label: "JPG (MozJPEG)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "png".into(),
                                label: "PNG (OxiPNG)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "webp".into(),
                                label: "WebP".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "avif".into(),
                                label: "AVIF".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("webp"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "quality".into(),
                    label: "Compression Quality".into(),
                    label_key: "opt_batch_quality".into(),
                    hint: "Target visual quality for compressed outputs (EXIF is automatically stripped)."
                        .into(),
                    hint_key: "opt_batch_quality_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(20),
                        max: Some(100),
                        step: Some(5),
                        unit: Some("%".into()),
                    },
                    default_value: json!(82),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_batch".to_string(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let max_dim = ctx.options.get_i64("max_dimension", 1920).max(0) as u32;
        let fmt_choice = ctx.options.get_string_or("format", "webp");
        let quality = ctx.options.get_i64("quality", 82).clamp(10, 100) as u8;

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for (i, input) in ctx.inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Batch processing {} of {}", i + 1, ctx.inputs.len()),
            );

            let mut img = load_image_any(input)?;
            if max_dim > 0 && (img.width() > max_dim || img.height() > max_dim) {
                let (dw, dh) =
                    compute_target_dimensions(img.width(), img.height(), max_dim, max_dim, true);
                img = simd_resize(&img, dw, dh, "lanczos3")?;
            }

            let out_ext = if fmt_choice == "keep" {
                source_output_ext(input)
            } else {
                normalize_output_extension(&fmt_choice)
            };
            let out_path = ctx.output_path_for(input, "_batch", Some(out_ext));
            // Re-encoding fresh pixel buffers also omits all source EXIF metadata cleanly.
            let written = encode_image_to_path(&img, &out_path, out_ext, quality, true)?;
            outputs.push(build_artifact(input, &out_path, written));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Batch processed {} image(s).", outputs.len()),
            outputs,
        ))
    }
}
