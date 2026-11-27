//! Bundled LGPL FFmpeg sidecar wrapper, media metadata probing, and video/audio processing tools.

pub mod runner;
pub mod sidecar;
pub mod tools;

pub use runner::{available_ffmpeg_encoders, run_ffmpeg_with_progress};
pub use sidecar::{
    decode_still_image_to_png, locate_ffmpeg_binary, parse_hms_seconds, probe_media_file,
    FfmpegBinary, MediaProbeInfo,
};
pub use tools::{
    register_media_tools, AudioConvertTool, ExtractAudioTool, ExtractFramesTool, MediaMergeTool,
    MediaTrimTool, VideoCompressTool, VideoConvertTool, VideoToGifTool, AUDIO_EXTENSIONS,
    MEDIA_EXTENSIONS, VIDEO_EXTENSIONS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use core_jobs::{CancelHandle, Tool, ToolContext, ToolOptions};
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_ctx(inputs: Vec<PathBuf>, options: ToolOptions) -> ToolContext {
        ToolContext::new(
            "media-test-job",
            inputs,
            options,
            None,
            CancelHandle::new(),
            Arc::new(|_, _| {}),
        )
    }

    /// Generates a tiny 2-second synthetic MP4 video + sine-wave audio fixture via the LGPL FFmpeg sidecar.
    fn generate_test_video(dest: &Path) {
        let sidecar = locate_ffmpeg_binary().unwrap();
        let (venc, extra) = runner::select_mp4_video_encoder(false);
        let mut cmd = sidecar.command_ffmpeg();
        cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=160x120:rate=15",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            venc,
        ]);
        cmd.args(&extra);
        cmd.args(["-c:a", "aac", "-shortest"])
            .arg(dest)
            .stdin(Stdio::null());
        let status = cmd.status().unwrap();
        assert!(status.success(), "Failed to synthesize test MP4 fixture");
    }

    #[test]
    fn sidecar_locator_finds_vendor_or_system_ffmpeg() {
        let found = locate_ffmpeg_binary();
        assert!(
            found.is_ok(),
            "Expected bundled vendor or system ffmpeg binary to be located: {found:?}"
        );
        assert!(!available_ffmpeg_encoders().is_empty());
    }

    #[test]
    fn parses_ffmpeg_stderr_probe_dimensions_and_streams_without_ffprobe() {
        let sample_stderr = "\
Input #0, mov,mp4,m4a,3gp,3g2,mj2, from '/tmp/clip1.mp4':
  Metadata:
    major_brand     : isom
  Duration: 00:00:02.00, start: 0.000000, bitrate: 158 kb/s
  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, progressive), 160x120 [SAR 1:1 DAR 4:3], 72 kb/s, 15 fps, 15 tbr, 15360 tbn (default)
  Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, mono, fltp, 74 kb/s (default)
At least one output file must be specified";

        let info =
            sidecar::parse_ffmpeg_stderr_probe(sample_stderr, Path::new("/tmp/clip1.mp4")).unwrap();
        assert_eq!(info.video_codec.as_deref(), Some("H264"));
        assert_eq!(info.width, Some(160));
        assert_eq!(info.height, Some(120));
        assert_eq!(info.fps.as_deref(), Some("15.00 fps"));
        assert_eq!(info.duration_secs, Some(2.0));
        assert_eq!(info.bit_rate_kbps, Some(158));
        assert_eq!(info.audio_codec.as_deref(), Some("AAC"));
        assert_eq!(info.sample_rate_hz, Some(44100));
        assert_eq!(info.channels, Some(1));
    }

    #[test]
    fn trim_merge_compress_convert_extract_audio_frames_and_gif_end_to_end() {
        let dir = tempdir().unwrap();
        let clip1 = dir.path().join("clip1.mp4");
        generate_test_video(&clip1);

        let probe = probe_media_file(&clip1).unwrap();
        assert_eq!(probe.width, Some(160));
        assert_eq!(probe.height, Some(120));

        // 1. Cut / Trim (lossless stream copy)
        let mut trim_opts = ToolOptions::new();
        trim_opts.insert("start_time", json!("00:00:00"));
        trim_opts.insert("end_time", json!("00:00:01"));
        trim_opts.insert("lossless", json!(true));
        let trim_res = MediaTrimTool
            .run(&make_ctx(vec![clip1.clone()], trim_opts))
            .unwrap();
        let trimmed_path = PathBuf::from(&trim_res.outputs[0].path);
        assert!(trimmed_path.exists());

        // 2. Merge 2 clips
        let merge_res = MediaMergeTool
            .run(&make_ctx(
                vec![clip1.clone(), trimmed_path],
                ToolOptions::new(),
            ))
            .unwrap();
        assert!(Path::new(&merge_res.outputs[0].path).exists());

        // 3. Compress Video
        let comp_res = VideoCompressTool
            .run(&make_ctx(vec![clip1.clone()], ToolOptions::new()))
            .unwrap();
        assert!(comp_res.outputs[0].size_bytes > 0);

        // 4. Convert Video to MKV (all platforms) + WebM (when VP8/VP9/AV1 encoder is compiled in)
        let mut conv_mkv_opts = ToolOptions::new();
        conv_mkv_opts.insert("format", json!("mkv"));
        let conv_mkv_res = VideoConvertTool
            .run(&make_ctx(vec![clip1.clone()], conv_mkv_opts))
            .unwrap();
        assert!(Path::new(&conv_mkv_res.outputs[0].path).exists());

        if runner::select_webm_video_encoder().is_some() {
            let mut conv_v_opts = ToolOptions::new();
            conv_v_opts.insert("format", json!("webm"));
            let conv_v_res = VideoConvertTool
                .run(&make_ctx(vec![clip1.clone()], conv_v_opts))
                .unwrap();
            assert!(Path::new(&conv_v_res.outputs[0].path).exists());
        }

        // 5. Extract Audio (MP3) -> Convert Audio (WAV + OGG)
        let mut ext_a_opts = ToolOptions::new();
        ext_a_opts.insert("format", json!("mp3"));
        let ext_a_res = ExtractAudioTool
            .run(&make_ctx(vec![clip1.clone()], ext_a_opts))
            .unwrap();
        let mp3_path = PathBuf::from(&ext_a_res.outputs[0].path);
        assert!(mp3_path.exists());

        let mut conv_a_opts = ToolOptions::new();
        conv_a_opts.insert("format", json!("wav"));
        let conv_a_res = AudioConvertTool
            .run(&make_ctx(vec![mp3_path.clone()], conv_a_opts))
            .unwrap();
        assert!(Path::new(&conv_a_res.outputs[0].path).exists());

        let mut conv_ogg_opts = ToolOptions::new();
        conv_ogg_opts.insert("format", json!("ogg"));
        let conv_ogg_res = AudioConvertTool
            .run(&make_ctx(vec![mp3_path], conv_ogg_opts))
            .unwrap();
        assert!(Path::new(&conv_ogg_res.outputs[0].path).exists());

        // 6. Extract Frames
        let mut frames_opts = ToolOptions::new();
        frames_opts.insert("mode", json!("interval"));
        frames_opts.insert("every_seconds", json!(1.0));
        let frames_res = ExtractFramesTool
            .run(&make_ctx(vec![clip1.clone()], frames_opts))
            .unwrap();
        assert!(Path::new(&frames_res.outputs[0].path).is_dir());

        // 7. Make Animated GIF
        let mut gif_opts = ToolOptions::new();
        gif_opts.insert("duration", json!(1.0));
        gif_opts.insert("width", json!(120));
        gif_opts.insert("fps", json!(8));
        let gif_res = VideoToGifTool
            .run(&make_ctx(vec![clip1], gif_opts))
            .unwrap();
        let gif_bytes = std::fs::read(&gif_res.outputs[0].path).unwrap();
        assert!(gif_bytes.starts_with(b"GIF89a"));
    }
}
