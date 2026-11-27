use crate::runner::{
    run_ffmpeg_with_progress, select_mp4_video_encoder, select_ogg_audio_encoder,
    select_webm_video_encoder,
};
use crate::sidecar::{parse_hms_seconds, probe_media_file};
use core_jobs::{
    OptionField, OptionKind, OutputArtifact, SelectChoice, Tool, ToolCategory, ToolContext,
    ToolDescriptor, ToolError, ToolInputSpec, ToolOptions, ToolRegistry, ToolRunResult,
    VisibilityCondition,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "mov", "webm", "avi", "m4v"];
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "aac", "ogg", "m4a", "opus"];
pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "mov", "webm", "avi", "m4v", "mp3", "wav", "flac", "aac", "ogg", "m4a", "opus",
];

/// Registers all Milestone 6 Video & Audio Tools into the shared `ToolRegistry`.
pub fn register_media_tools(registry: &ToolRegistry) {
    registry.register(MediaTrimTool);
    registry.register(MediaMergeTool);
    registry.register(VideoCompressTool);
    registry.register(VideoConvertTool);
    registry.register(ExtractAudioTool);
    registry.register(AudioConvertTool);
    registry.register(ExtractFramesTool);
    registry.register(VideoToGifTool);
}

fn media_artifact(source: Option<&Path>, out_path: &Path) -> OutputArtifact {
    let size = fs::metadata(out_path).map(|m| m.len()).unwrap_or(0);
    let orig = source.and_then(|p| fs::metadata(p).map(|m| m.len()).ok());
    OutputArtifact {
        path: out_path.display().to_string(),
        label: out_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string(),
        size_bytes: size,
        original_size_bytes: orig,
    }
}

fn is_audio_only_ext(ext: &str) -> bool {
    AUDIO_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext))
}

fn build_video_transcode_args(
    target_ext: &str,
    bitrate_kbps: u32,
    prefer_av1: bool,
    scale_height: Option<u32>,
) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(h) = scale_height.filter(|&h| h > 0) {
        // Ensure dimensions stay divisible by 2 for 4:2:0 encoders
        args.push("-vf".into());
        args.push(format!("scale=-2:'min({h},ih)'"));
    }

    match target_ext {
        "webm" => {
            let (venc, extra) = select_webm_video_encoder();
            args.extend(["-c:v".into(), venc.into()]);
            args.extend(extra);
            let aenc = select_ogg_audio_encoder();
            args.extend(["-c:a".into(), aenc.into(), "-b:a".into(), "128k".into()]);
        }
        "avi" => {
            args.extend([
                "-c:v".into(),
                "mpeg4".into(),
                "-b:v".into(),
                format!("{bitrate_kbps}k"),
                "-c:a".into(),
                "libmp3lame".into(),
                "-b:a".into(),
                "160k".into(),
            ]);
        }
        _ => {
            let (venc, extra) = select_mp4_video_encoder(prefer_av1 && target_ext != "mov");
            args.extend(["-c:v".into(), venc.into()]);
            args.extend(extra);
            if venc != "libsvtav1" {
                args.extend(["-b:v".into(), format!("{bitrate_kbps}k")]);
            } else {
                args.extend(["-crf".into(), "32".into()]);
            }
            args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "160k".into()]);
            if matches!(target_ext, "mp4" | "mov" | "m4v") {
                args.extend(["-movflags".into(), "+faststart".into()]);
            }
        }
    }
    args
}

fn build_audio_transcode_args(target_ext: &str, bitrate_kbps: u32) -> (&'static str, Vec<String>) {
    match target_ext {
        "wav" => ("wav", vec!["-vn".into(), "-c:a".into(), "pcm_s16le".into()]),
        "flac" => ("flac", vec!["-vn".into(), "-c:a".into(), "flac".into()]),
        "aac" | "m4a" => (
            "m4a",
            vec![
                "-vn".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                format!("{bitrate_kbps}k"),
            ],
        ),
        "ogg" => {
            let enc = select_ogg_audio_encoder();
            (
                "ogg",
                vec![
                    "-vn".into(),
                    "-c:a".into(),
                    enc.into(),
                    "-b:a".into(),
                    format!("{bitrate_kbps}k"),
                ],
            )
        }
        _ => (
            "mp3",
            vec![
                "-vn".into(),
                "-c:a".into(),
                "libmp3lame".into(),
                "-b:a".into(),
                format!("{bitrate_kbps}k"),
            ],
        ),
    }
}

// ============================================================================
// 1. Cut / Trim Video & Audio (`media.trim`)
// ============================================================================
pub struct MediaTrimTool;

impl Tool for MediaTrimTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "media.trim".into(),
            name: "Cut / Trim Media".into(),
            name_key: "tool_media_trim".into(),
            description:
                "Trim a video or audio clip between start and end timestamps (instant lossless copy when possible)."
                    .into(),
            description_key: "tool_media_trim_desc".into(),
            category: ToolCategory::Video,
            icon: "cut".into(),
            keywords: vec![
                "slice".into(),
                "clip".into(),
                "trim".into(),
                "cut".into(),
                "audio".into(),
                "video".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(MEDIA_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "start_time".into(),
                    label: "Start Time".into(),
                    label_key: "opt_start_time".into(),
                    hint: "Timestamp like 00:00:02 or seconds like 2.5.".into(),
                    hint_key: "opt_start_time_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("00:00:00".into()),
                        secret: false,
                    },
                    default_value: json!("00:00:00"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "end_time".into(),
                    label: "End Time".into(),
                    label_key: "opt_end_time".into(),
                    hint: "Timestamp like 00:00:15 or seconds like 15.".into(),
                    hint_key: "opt_end_time_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("00:00:10".into()),
                        secret: false,
                    },
                    default_value: json!("00:00:10"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "lossless".into(),
                    label: "Lossless Stream Copy (Instant)".into(),
                    label_key: "opt_trim_lossless".into(),
                    hint: "Copies original streams without re-encoding. Disable only if you need frame-exact cuts between keyframes.".into(),
                    hint_key: "opt_trim_lossless_hint".into(),
                    kind: OptionKind::Boolean,
                    default_value: json!(true),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_trimmed".into(),
        }
    }

    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        let desc = self.descriptor();
        core_jobs::validate_inputs_against_spec(inputs, &desc.input_spec)?;
        let start =
            parse_hms_seconds(&options.get_string_or("start_time", "0")).ok_or_else(|| {
                ToolError::invalid("Invalid Start Time format. Use HH:MM:SS or seconds.")
            })?;
        let end = parse_hms_seconds(&options.get_string_or("end_time", "10")).ok_or_else(|| {
            ToolError::invalid("Invalid End Time format. Use HH:MM:SS or seconds.")
        })?;
        if end <= start {
            return Err(ToolError::invalid(
                "End Time must be greater than Start Time.",
            ));
        }
        Ok(())
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let start_str = ctx.options.get_string_or("start_time", "00:00:00");
        let end_str = ctx.options.get_string_or("end_time", "00:00:10");
        let start_s = parse_hms_seconds(&start_str).unwrap_or(0.0);
        let end_s = parse_hms_seconds(&end_str).unwrap_or(10.0);
        let clip_dur = (end_s - start_s).max(0.1);
        let lossless = ctx.options.get_bool("lossless", true);

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for input in &ctx.inputs {
            ctx.check_cancelled()?;
            let ext = input
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_else(|| "mp4".into());
            let out_path = ctx.output_path_for(input, "_trimmed", Some(&ext));

            if lossless {
                let copy_args = vec![
                    "-ss".into(),
                    format!("{start_s:.3}"),
                    "-to".into(),
                    format!("{end_s:.3}"),
                    "-i".into(),
                    input.display().to_string(),
                    "-c".into(),
                    "copy".into(),
                    "-avoid_negative_ts".into(),
                    "make_zero".into(),
                    out_path.display().to_string(),
                ];
                if run_ffmpeg_with_progress(ctx, &copy_args, Some(clip_dur), "Trimming (lossless)")
                    .is_ok()
                {
                    outputs.push(media_artifact(Some(input), &out_path));
                    continue;
                }
            }

            // Frame-accurate re-encode path (or fallback if stream copy failed)
            let mut args = vec![
                "-ss".into(),
                format!("{start_s:.3}"),
                "-to".into(),
                format!("{end_s:.3}"),
                "-i".into(),
                input.display().to_string(),
            ];
            if is_audio_only_ext(&ext) {
                let (_, audio_args) = build_audio_transcode_args(&ext, 192);
                args.extend(audio_args);
            } else {
                args.extend(build_video_transcode_args(&ext, 2500, false, None));
            }
            args.push(out_path.display().to_string());

            run_ffmpeg_with_progress(ctx, &args, Some(clip_dur), "Trimming")?;
            outputs.push(media_artifact(Some(input), &out_path));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Trimmed {} file(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 2. Merge Video / Audio (`media.merge`)
// ============================================================================
pub struct MediaMergeTool;

impl Tool for MediaMergeTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "media.merge".into(),
            name: "Merge Videos / Audio".into(),
            name_key: "tool_media_merge".into(),
            description: "Join two or more video or audio clips sequentially into a single file."
                .into(),
            description_key: "tool_media_merge_desc".into(),
            category: ToolCategory::Video,
            icon: "video".into(),
            keywords: vec![
                "concat".into(),
                "join".into(),
                "combine".into(),
                "stitch".into(),
            ],
            input_spec: ToolInputSpec::multi_minimum(MEDIA_EXTENSIONS, 2),
            options_schema: vec![],
            default_suffix: "_merged".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let first_ext = ctx.inputs[0]
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| "mp4".into());

        let mut total_dur = 0.0;
        let mut list_content = String::new();
        for p in &ctx.inputs {
            if let Ok(info) = probe_media_file(p) {
                total_dur += info.duration_secs.unwrap_or(0.0);
            }
            let escaped = p.display().to_string().replace('\'', "'\\''");
            list_content.push_str(&format!("file '{escaped}'\n"));
        }

        let list_path = std::env::temp_dir().join(format!("tc_concat_{}.txt", Uuid::new_v4()));
        fs::write(&list_path, list_content).map_err(|e| {
            ToolError::failed("Failed to create concat manifest", Some(e.to_string()))
        })?;

        let out_path = ctx.output_path_for(&ctx.inputs[0], "_merged", Some(&first_ext));

        // Attempt lossless stream-copy concat first when all extensions match
        let same_ext = ctx.inputs.iter().all(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case(&first_ext))
                .unwrap_or(false)
        });

        let mut succeeded = false;
        if same_ext {
            let copy_args = vec![
                "-f".into(),
                "concat".into(),
                "-safe".into(),
                "0".into(),
                "-i".into(),
                list_path.display().to_string(),
                "-c".into(),
                "copy".into(),
                out_path.display().to_string(),
            ];
            if run_ffmpeg_with_progress(
                ctx,
                &copy_args,
                (total_dur > 0.0).then_some(total_dur),
                "Merging (lossless)",
            )
            .is_ok()
            {
                succeeded = true;
            }
        }

        if !succeeded {
            let mut re_args = vec![
                "-f".into(),
                "concat".into(),
                "-safe".into(),
                "0".into(),
                "-i".into(),
                list_path.display().to_string(),
            ];
            if is_audio_only_ext(&first_ext) {
                let (_, a_args) = build_audio_transcode_args(&first_ext, 192);
                re_args.extend(a_args);
            } else {
                re_args.extend(build_video_transcode_args(&first_ext, 2500, false, None));
            }
            re_args.push(out_path.display().to_string());
            let res = run_ffmpeg_with_progress(
                ctx,
                &re_args,
                (total_dur > 0.0).then_some(total_dur),
                "Merging clips",
            );
            let _ = fs::remove_file(&list_path);
            res?;
        } else {
            let _ = fs::remove_file(&list_path);
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Merged {} media clips.", ctx.inputs.len()),
            vec![media_artifact(None, &out_path)],
        ))
    }
}

// ============================================================================
// 3. Compress Video (`media.compress`)
// ============================================================================
pub struct VideoCompressTool;

impl Tool for VideoCompressTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "media.compress".into(),
            name: "Compress Video".into(),
            name_key: "tool_video_compress".into(),
            description: "Shrink large video files for sharing while preserving smooth playback."
                .into(),
            description_key: "tool_video_compress_desc".into(),
            category: ToolCategory::Video,
            icon: "compress".into(),
            keywords: vec![
                "shrink".into(),
                "size".into(),
                "bitrate".into(),
                "mp4".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(VIDEO_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "preset".into(),
                    label: "Compression Level".into(),
                    label_key: "opt_vid_comp_preset".into(),
                    hint: "Balanced cuts file size substantially with clean visual detail.".into(),
                    hint_key: "opt_vid_comp_preset_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "balanced".into(),
                                label: "Balanced (Recommended)".into(),
                                description: Some("~1.5 Mbps target".into()),
                            },
                            SelectChoice {
                                value: "smaller".into(),
                                label: "Maximum Space Savings".into(),
                                description: Some("~800 kbps target".into()),
                            },
                            SelectChoice {
                                value: "high".into(),
                                label: "High Visual Quality".into(),
                                description: Some("~3.0 Mbps target".into()),
                            },
                        ],
                    },
                    default_value: json!("balanced"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "max_height".into(),
                    label: "Max Resolution".into(),
                    label_key: "opt_vid_max_res".into(),
                    hint: "Downscaling 4K to 1080p or 720p dramatically reduces file size.".into(),
                    hint_key: "opt_vid_max_res_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "1080".into(),
                                label: "1080p Full HD".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "720".into(),
                                label: "720p HD".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "480".into(),
                                label: "480p Compact".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "0".into(),
                                label: "Keep Original Resolution".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("1080"),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_compressed".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let preset = ctx.options.get_string_or("preset", "balanced");
        let bitrate = match preset.as_str() {
            "smaller" => 800,
            "high" => 3000,
            _ => 1500,
        };
        let max_h: u32 = ctx
            .options
            .get_string_or("max_height", "1080")
            .parse()
            .unwrap_or(1080);

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for input in &ctx.inputs {
            ctx.check_cancelled()?;
            let dur = probe_media_file(input).ok().and_then(|m| m.duration_secs);
            let out_path = ctx.output_path_for(input, "_compressed", Some("mp4"));

            let mut args = vec!["-i".into(), input.display().to_string()];
            args.extend(build_video_transcode_args(
                "mp4",
                bitrate,
                false,
                (max_h > 0).then_some(max_h),
            ));
            args.push(out_path.display().to_string());

            run_ffmpeg_with_progress(ctx, &args, dur, "Compressing video")?;
            outputs.push(media_artifact(Some(input), &out_path));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Compressed {} video(s).", outputs.len()),
            outputs,
        ))
    }
}

// ============================================================================
// 4. Convert Video (`media.convert_video`)
// ============================================================================
pub struct VideoConvertTool;

impl Tool for VideoConvertTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "media.convert_video".into(),
            name: "Convert Video".into(),
            name_key: "tool_video_convert".into(),
            description: "Convert videos between MP4, MKV, WebM, MOV, and AVI.".into(),
            description_key: "tool_video_convert_desc".into(),
            category: ToolCategory::Video,
            icon: "video".into(),
            keywords: vec![
                "mp4".into(),
                "mkv".into(),
                "webm".into(),
                "mov".into(),
                "avi".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(VIDEO_EXTENSIONS),
            options_schema: vec![OptionField {
                key: "format".into(),
                label: "Output Format".into(),
                label_key: "opt_video_format".into(),
                hint: "MP4 works almost everywhere; MKV and WebM are open container standards."
                    .into(),
                hint_key: "opt_video_format_hint".into(),
                kind: OptionKind::Select {
                    choices: vec![
                        SelectChoice {
                            value: "mp4".into(),
                            label: "MP4 (Universal)".into(),
                            description: None,
                        },
                        SelectChoice {
                            value: "mkv".into(),
                            label: "MKV (Matroska)".into(),
                            description: None,
                        },
                        SelectChoice {
                            value: "webm".into(),
                            label: "WebM (VP9 / AV1 Web)".into(),
                            description: None,
                        },
                        SelectChoice {
                            value: "mov".into(),
                            label: "MOV (QuickTime)".into(),
                            description: None,
                        },
                        SelectChoice {
                            value: "avi".into(),
                            label: "AVI (Legacy)".into(),
                            description: None,
                        },
                    ],
                },
                default_value: json!("mp4"),
                required: true,
                visible_when: None,
            }],
            default_suffix: "_converted".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let fmt = ctx.options.get_string_or("format", "mp4");
        let mut outputs = Vec::with_capacity(ctx.inputs.len());

        for input in &ctx.inputs {
            ctx.check_cancelled()?;
            let dur = probe_media_file(input).ok().and_then(|m| m.duration_secs);
            let out_path = ctx.output_path_for(input, "_converted", Some(&fmt));

            let mut args = vec!["-i".into(), input.display().to_string()];
            args.extend(build_video_transcode_args(&fmt, 2500, false, None));
            args.push(out_path.display().to_string());

            run_ffmpeg_with_progress(ctx, &args, dur, "Converting video")?;
            outputs.push(media_artifact(Some(input), &out_path));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!(
                "Converted {} video(s) to {}.",
                outputs.len(),
                fmt.to_uppercase()
            ),
            outputs,
        ))
    }
}

// ============================================================================
// 5. Extract Audio from Video (`media.extract_audio`)
// ============================================================================
pub struct ExtractAudioTool;

impl Tool for ExtractAudioTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "media.extract_audio".into(),
            name: "Extract Audio".into(),
            name_key: "tool_extract_audio".into(),
            description: "Save the audio track from any video file as MP3, WAV, FLAC, M4A, or OGG."
                .into(),
            description_key: "tool_extract_audio_desc".into(),
            category: ToolCategory::Audio,
            icon: "audio".into(),
            keywords: vec![
                "mp3".into(),
                "soundtrack".into(),
                "rip".into(),
                "wav".into(),
                "flac".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(VIDEO_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "format".into(),
                    label: "Audio Format".into(),
                    label_key: "opt_extract_audio_fmt".into(),
                    hint: "MP3 and M4A are compact; FLAC and WAV are lossless.".into(),
                    hint_key: "opt_extract_audio_fmt_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "mp3".into(),
                                label: "MP3 (Universal)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "m4a".into(),
                                label: "M4A / AAC".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "flac".into(),
                                label: "FLAC (Lossless Compressed)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "wav".into(),
                                label: "WAV (Uncompressed PCM)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "ogg".into(),
                                label: "OGG (Vorbis / Opus)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("mp3"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "bitrate".into(),
                    label: "Audio Bitrate".into(),
                    label_key: "opt_audio_bitrate".into(),
                    hint: "Used for MP3, M4A, and OGG outputs.".into(),
                    hint_key: "opt_audio_bitrate_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "192".into(),
                                label: "192 kbps (High Quality)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "320".into(),
                                label: "320 kbps (Maximum)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "128".into(),
                                label: "128 kbps (Compact)".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("192"),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_audio".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let req_fmt = ctx.options.get_string_or("format", "mp3");
        let bitrate: u32 = ctx
            .options
            .get_string_or("bitrate", "192")
            .parse()
            .unwrap_or(192);
        let (out_ext, audio_args) = build_audio_transcode_args(&req_fmt, bitrate);

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for input in &ctx.inputs {
            ctx.check_cancelled()?;
            let dur = probe_media_file(input).ok().and_then(|m| m.duration_secs);
            let out_path = ctx.output_path_for(input, "_audio", Some(out_ext));

            let mut args = vec!["-i".into(), input.display().to_string()];
            args.extend(audio_args.clone());
            args.push(out_path.display().to_string());

            run_ffmpeg_with_progress(ctx, &args, dur, "Extracting audio")?;
            outputs.push(media_artifact(None, &out_path));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!(
                "Extracted audio to {} from {} video(s).",
                out_ext.to_uppercase(),
                outputs.len()
            ),
            outputs,
        ))
    }
}

// ============================================================================
// 6. Convert Audio (`media.convert_audio`)
// ============================================================================
pub struct AudioConvertTool;

impl Tool for AudioConvertTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "media.convert_audio".into(),
            name: "Convert Audio".into(),
            name_key: "tool_convert_audio".into(),
            description: "Convert audio files between MP3, WAV, FLAC, M4A (AAC), and OGG.".into(),
            description_key: "tool_convert_audio_desc".into(),
            category: ToolCategory::Audio,
            icon: "audio".into(),
            keywords: vec![
                "mp3".into(),
                "wav".into(),
                "flac".into(),
                "aac".into(),
                "ogg".into(),
            ],
            input_spec: ToolInputSpec::single_or_batch(AUDIO_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "format".into(),
                    label: "Output Format".into(),
                    label_key: "opt_conv_audio_fmt".into(),
                    hint: "Target audio format.".into(),
                    hint_key: "opt_conv_audio_fmt_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "mp3".into(),
                                label: "MP3".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "wav".into(),
                                label: "WAV (Lossless PCM)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "flac".into(),
                                label: "FLAC (Lossless)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "m4a".into(),
                                label: "M4A / AAC".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "ogg".into(),
                                label: "OGG".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("mp3"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "bitrate".into(),
                    label: "Bitrate".into(),
                    label_key: "opt_conv_audio_br".into(),
                    hint: "Used for lossy formats (MP3, M4A, OGG).".into(),
                    hint_key: "opt_conv_audio_br_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "192".into(),
                                label: "192 kbps".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "320".into(),
                                label: "320 kbps".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "128".into(),
                                label: "128 kbps".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("192"),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_converted".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let req_fmt = ctx.options.get_string_or("format", "mp3");
        let bitrate: u32 = ctx
            .options
            .get_string_or("bitrate", "192")
            .parse()
            .unwrap_or(192);
        let (out_ext, audio_args) = build_audio_transcode_args(&req_fmt, bitrate);

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for input in &ctx.inputs {
            ctx.check_cancelled()?;
            let dur = probe_media_file(input).ok().and_then(|m| m.duration_secs);
            let out_path = ctx.output_path_for(input, "_converted", Some(out_ext));

            let mut args = vec!["-i".into(), input.display().to_string()];
            args.extend(audio_args.clone());
            args.push(out_path.display().to_string());

            run_ffmpeg_with_progress(ctx, &args, dur, "Converting audio")?;
            outputs.push(media_artifact(Some(input), &out_path));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!(
                "Converted {} audio file(s) to {}.",
                outputs.len(),
                out_ext.to_uppercase()
            ),
            outputs,
        ))
    }
}

// ============================================================================
// 7. Extract Video Frames (`media.extract_frames`)
// ============================================================================
pub struct ExtractFramesTool;

impl Tool for ExtractFramesTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "media.extract_frames".into(),
            name: "Extract Video Frames".into(),
            name_key: "tool_extract_frames".into(),
            description: "Export still PNG or JPG images from a video at regular intervals or a single timestamp."
                .into(),
            description_key: "tool_extract_frames_desc".into(),
            category: ToolCategory::Video,
            icon: "frames".into(),
            keywords: vec![
                "screenshot".into(),
                "thumbnails".into(),
                "stills".into(),
                "sequence".into(),
            ],
            input_spec: ToolInputSpec::single_file(VIDEO_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "mode".into(),
                    label: "Capture Mode".into(),
                    label_key: "opt_frames_mode".into(),
                    hint: "Export a sequence across the video or grab one exact moment.".into(),
                    hint_key: "opt_frames_mode_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "interval".into(),
                                label: "Periodic Interval (Sequence)".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "single".into(),
                                label: "Single Frame at Timestamp".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("interval"),
                    required: true,
                    visible_when: None,
                },
                OptionField {
                    key: "every_seconds".into(),
                    label: "Capture Every N Seconds".into(),
                    label_key: "opt_every_seconds".into(),
                    hint: "1 exports one frame per second; 5 exports one frame every 5 seconds."
                        .into(),
                    hint_key: "opt_every_seconds_hint".into(),
                    kind: OptionKind::Number {
                        min: Some(0.1),
                        max: Some(600.0),
                        step: Some(0.5),
                        unit: Some("sec".into()),
                    },
                    default_value: json!(1.0),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("interval"),
                    }),
                },
                OptionField {
                    key: "timestamp".into(),
                    label: "Timestamp".into(),
                    label_key: "opt_frame_ts".into(),
                    hint: "Exact moment to capture (e.g. 00:00:03).".into(),
                    hint_key: "opt_frame_ts_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("00:00:01".into()),
                        secret: false,
                    },
                    default_value: json!("00:00:01"),
                    required: false,
                    visible_when: Some(VisibilityCondition {
                        option_key: "mode".into(),
                        equals_value: json!("single"),
                    }),
                },
                OptionField {
                    key: "format".into(),
                    label: "Image Format".into(),
                    label_key: "opt_frame_fmt".into(),
                    hint: "PNG is lossless; JPG uses less disk space.".into(),
                    hint_key: "opt_frame_fmt_hint".into(),
                    kind: OptionKind::Select {
                        choices: vec![
                            SelectChoice {
                                value: "png".into(),
                                label: "PNG".into(),
                                description: None,
                            },
                            SelectChoice {
                                value: "jpg".into(),
                                label: "JPG".into(),
                                description: None,
                            },
                        ],
                    },
                    default_value: json!("png"),
                    required: true,
                    visible_when: None,
                },
            ],
            default_suffix: "_frames".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let input = &ctx.inputs[0];
        let mode = ctx.options.get_string_or("mode", "interval");
        let fmt = if ctx.options.get_string_or("format", "png") == "jpg" {
            "jpg"
        } else {
            "png"
        };

        if mode == "single" {
            let ts = ctx.options.get_string_or("timestamp", "00:00:01");
            let out_path = ctx.output_path_for(input, "_frame", Some(fmt));
            let args = vec![
                "-ss".into(),
                ts,
                "-i".into(),
                input.display().to_string(),
                "-frames:v".into(),
                "1".into(),
                out_path.display().to_string(),
            ];
            run_ffmpeg_with_progress(ctx, &args, None, "Extracting frame")?;
            ctx.report_progress(1.0, "Completed");
            return Ok(ToolRunResult::with_outputs(
                "Extracted 1 video frame.",
                vec![media_artifact(None, &out_path)],
            ));
        }

        let every_s = ctx.options.get_f64("every_seconds", 1.0).max(0.05);
        let dur = probe_media_file(input).ok().and_then(|m| m.duration_secs);
        let out_dir = ctx.output_dir_for(input, "_frames");
        fs::create_dir_all(&out_dir).map_err(|e| {
            ToolError::failed("Failed to create frames folder", Some(e.to_string()))
        })?;

        let pattern = out_dir.join(format!("frame_%04d.{fmt}"));
        let fps_expr = format!("fps=1/{every_s:.3}");
        let args = vec![
            "-i".into(),
            input.display().to_string(),
            "-vf".into(),
            fps_expr,
            pattern.display().to_string(),
        ];

        run_ffmpeg_with_progress(ctx, &args, dur, "Extracting frames")?;

        let mut frame_files: Vec<PathBuf> = fs::read_dir(&out_dir)
            .map_err(|e| ToolError::failed("Failed reading extracted frames", Some(e.to_string())))?
            .filter_map(|e| e.ok().map(|d| d.path()))
            .filter(|p| p.is_file())
            .collect();
        frame_files.sort();

        let total_bytes: u64 = frame_files
            .iter()
            .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
            .sum();

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Extracted {} frame(s) into folder.", frame_files.len()),
            vec![OutputArtifact {
                path: out_dir.display().to_string(),
                label: format!(
                    "{} ({} frames)",
                    out_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("frames"),
                    frame_files.len()
                ),
                size_bytes: total_bytes,
                original_size_bytes: None,
            }],
        ))
    }
}

// ============================================================================
// 8. Make Animated GIF (`media.make_gif`)
// ============================================================================
pub struct VideoToGifTool;

impl Tool for VideoToGifTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "media.make_gif".into(),
            name: "Make Animated GIF".into(),
            name_key: "tool_make_gif".into(),
            description:
                "Turn a video clip into a crisp animated GIF using adaptive 256-color palette generation."
                    .into(),
            description_key: "tool_make_gif_desc".into(),
            category: ToolCategory::Video,
            icon: "gif".into(),
            keywords: vec!["gif".into(), "animation".into(), "loop".into(), "meme".into()],
            input_spec: ToolInputSpec::single_or_batch(VIDEO_EXTENSIONS),
            options_schema: vec![
                OptionField {
                    key: "start_time".into(),
                    label: "Start Time".into(),
                    label_key: "opt_gif_start".into(),
                    hint: "Where in the video the GIF should start.".into(),
                    hint_key: "opt_gif_start_hint".into(),
                    kind: OptionKind::Text {
                        placeholder: Some("00:00:00".into()),
                        secret: false,
                    },
                    default_value: json!("00:00:00"),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "duration".into(),
                    label: "Clip Length".into(),
                    label_key: "opt_gif_duration".into(),
                    hint: "Length of the animated GIF in seconds.".into(),
                    hint_key: "opt_gif_duration_hint".into(),
                    kind: OptionKind::Number {
                        min: Some(0.5),
                        max: Some(30.0),
                        step: Some(0.5),
                        unit: Some("sec".into()),
                    },
                    default_value: json!(5.0),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "width".into(),
                    label: "Width".into(),
                    label_key: "opt_gif_width".into(),
                    hint: "480 px balances sharp quality and compact file size.".into(),
                    hint_key: "opt_gif_width_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(120),
                        max: Some(1280),
                        step: Some(40),
                        unit: Some("px".into()),
                    },
                    default_value: json!(480),
                    required: false,
                    visible_when: None,
                },
                OptionField {
                    key: "fps".into(),
                    label: "Frame Rate".into(),
                    label_key: "opt_gif_fps".into(),
                    hint: "10–15 fps looks smooth while keeping GIF size reasonable.".into(),
                    hint_key: "opt_gif_fps_hint".into(),
                    kind: OptionKind::Integer {
                        min: Some(5),
                        max: Some(30),
                        step: Some(1),
                        unit: Some("fps".into()),
                    },
                    default_value: json!(12),
                    required: false,
                    visible_when: None,
                },
            ],
            default_suffix: "_anim".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let start = ctx.options.get_string_or("start_time", "00:00:00");
        let dur = ctx.options.get_f64("duration", 5.0).clamp(0.2, 60.0);
        let width = ctx.options.get_i64("width", 480).clamp(80, 1920);
        let fps = ctx.options.get_i64("fps", 12).clamp(2, 30);

        let filter = format!(
            "fps={fps},scale={width}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer"
        );

        let mut outputs = Vec::with_capacity(ctx.inputs.len());
        for input in &ctx.inputs {
            ctx.check_cancelled()?;
            let out_path = ctx.output_path_for(input, "_anim", Some("gif"));
            let args = vec![
                "-ss".into(),
                start.clone(),
                "-t".into(),
                format!("{dur:.2}"),
                "-i".into(),
                input.display().to_string(),
                "-vf".into(),
                filter.clone(),
                "-loop".into(),
                "0".into(),
                out_path.display().to_string(),
            ];
            run_ffmpeg_with_progress(ctx, &args, Some(dur), "Generating GIF")?;
            outputs.push(media_artifact(Some(input), &out_path));
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs(
            format!("Created {} animated GIF(s).", outputs.len()),
            outputs,
        ))
    }
}
