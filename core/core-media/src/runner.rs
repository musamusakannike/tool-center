use crate::sidecar::{locate_ffmpeg_binary, FfmpegBinary};
use core_jobs::{CancelHandle, ToolContext, ToolError};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

/// Cached set of encoder names compiled into the active LGPL FFmpeg sidecar.
static AVAILABLE_ENCODERS: OnceLock<HashSet<String>> = OnceLock::new();

pub fn available_ffmpeg_encoders() -> &'static HashSet<String> {
    AVAILABLE_ENCODERS.get_or_init(|| {
        let mut set = HashSet::new();
        if let Ok(sidecar) = locate_ffmpeg_binary() {
            if let Ok(out) = sidecar
                .command_ffmpeg()
                .args(["-hide_banner", "-encoders"])
                .stdin(Stdio::null())
                .output()
            {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let trimmed = line.trim_start();
                    // Encoder lines look like: ` V....D libopenh264    OpenH264 ...`
                    let mut parts = trimmed.split_whitespace();
                    if let (Some(flags), Some(name)) = (parts.next(), parts.next()) {
                        if flags.len() == 6
                            && flags.starts_with(['V', 'A', 'S'])
                            && !name.starts_with('=')
                        {
                            set.insert(name.to_string());
                        }
                    }
                }
            }
        }
        set
    })
}

/// Selects the best LGPL-compatible H.264 / MP4-compatible video encoder available on this platform.
pub fn select_mp4_video_encoder(prefer_av1: bool) -> (&'static str, Vec<String>) {
    let encs = available_ffmpeg_encoders();
    if prefer_av1 && encs.contains("libsvtav1") {
        return (
            "libsvtav1",
            vec![
                "-preset".into(),
                "8".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
            ],
        );
    }
    if encs.contains("h264_videotoolbox") {
        return (
            "h264_videotoolbox",
            vec!["-pix_fmt".into(), "yuv420p".into()],
        );
    }
    if encs.contains("libopenh264") {
        return ("libopenh264", vec!["-pix_fmt".into(), "yuv420p".into()]);
    }
    if encs.contains("libx264") {
        return (
            "libx264",
            vec![
                "-preset".into(),
                "veryfast".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
            ],
        );
    }
    (
        "mpeg4",
        vec![
            "-q:v".into(),
            "5".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
        ],
    )
}

/// Selects an LGPL-compatible WebM video encoder (`libvpx-vp9`, `libsvtav1`, `librav1e`, `libaom-av1`, or `libvpx`) if compiled in.
pub fn select_webm_video_encoder() -> Option<(&'static str, Vec<String>)> {
    let encs = available_ffmpeg_encoders();
    if encs.contains("libvpx-vp9") {
        Some((
            "libvpx-vp9",
            vec![
                "-b:v".into(),
                "0".into(),
                "-crf".into(),
                "32".into(),
                "-deadline".into(),
                "good".into(),
                "-cpu-used".into(),
                "4".into(),
            ],
        ))
    } else if encs.contains("libsvtav1") {
        Some(("libsvtav1", vec!["-preset".into(), "8".into()]))
    } else if encs.contains("librav1e") {
        Some(("librav1e", vec!["-speed".into(), "8".into()]))
    } else if encs.contains("libaom-av1") {
        Some((
            "libaom-av1",
            vec!["-cpu-used".into(), "6".into(), "-crf".into(), "34".into()],
        ))
    } else if encs.contains("libvpx") {
        Some(("libvpx", vec!["-b:v".into(), "1500k".into()]))
    } else {
        None
    }
}

/// Selects an LGPL-compatible Ogg audio encoder (`libvorbis`, `libopus`, or built-in `vorbis` with `-strict -2 -ac 2`).
pub fn select_ogg_audio_encoder() -> (&'static str, Vec<String>) {
    let encs = available_ffmpeg_encoders();
    if encs.contains("libvorbis") {
        ("libvorbis", Vec::new())
    } else if encs.contains("libopus") {
        ("libopus", Vec::new())
    } else {
        (
            "vorbis",
            vec!["-strict".into(), "-2".into(), "-ac".into(), "2".into()],
        )
    }
}

/// Selects an LGPL-compatible MP3 audio encoder (`libmp3lame` or `libshine`).
pub fn select_mp3_audio_encoder() -> Option<&'static str> {
    let encs = available_ffmpeg_encoders();
    if encs.contains("libmp3lame") {
        Some("libmp3lame")
    } else if encs.contains("libshine") {
        Some("libshine")
    } else {
        None
    }
}

/// Spawns the bundled LGPL FFmpeg sidecar with `-progress pipe:1`, parses live progress
/// against `expected_duration_secs`, and kills the child process immediately on cancellation.
pub fn run_ffmpeg_with_progress(
    ctx: &ToolContext,
    args: &[String],
    expected_duration_secs: Option<f64>,
    step_prefix: &str,
) -> Result<(), ToolError> {
    ctx.check_cancelled()?;
    let sidecar = locate_ffmpeg_binary()?;
    run_ffmpeg_sidecar(
        &sidecar,
        args,
        expected_duration_secs,
        &ctx.cancel,
        |frac, msg| ctx.report_progress(frac, msg),
        step_prefix,
    )
}

pub fn run_ffmpeg_sidecar<F>(
    sidecar: &FfmpegBinary,
    args: &[String],
    expected_duration_secs: Option<f64>,
    cancel: &CancelHandle,
    report_progress: F,
    step_prefix: &str,
) -> Result<(), ToolError>
where
    F: Fn(f32, String),
{
    let mut cmd = sidecar.command_ffmpeg();
    cmd.args([
        "-nostdin",
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-progress",
        "pipe:1",
        "-nostats",
    ]);
    cmd.args(args);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| ToolError::failed("Failed to launch FFmpeg sidecar", Some(e.to_string())))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Drain stderr in background thread (bounded to 16 KiB) so pipe buffers never deadlock
    let stderr_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(err_pipe) = stderr {
            let _ = err_pipe.take(16 * 1024).read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).trim().to_string()
    });

    let shared_child = Arc::new(Mutex::new(child));
    let kill_ref = Arc::clone(&shared_child);
    cancel.on_cancel(move || {
        if let Ok(mut guard) = kill_ref.lock() {
            let _ = guard.kill();
        }
    });

    if let Some(out_pipe) = stdout {
        let reader = BufReader::new(out_pipe);
        let total_us = expected_duration_secs
            .filter(|&d| d > 0.05)
            .map(|d| d * 1_000_000.0);

        for line in reader.lines().map_while(Result::ok) {
            if cancel.is_cancelled() {
                if let Ok(mut guard) = shared_child.lock() {
                    let _ = guard.kill();
                }
                break;
            }
            if let Some(val_str) = line.strip_prefix("out_time_us=") {
                if let (Some(total), Ok(cur_us)) = (total_us, val_str.trim().parse::<f64>()) {
                    if cur_us >= 0.0 {
                        let pct = (cur_us / total).clamp(0.0, 0.99) as f32;
                        report_progress(pct, format!("{step_prefix} ({:.0}%)", pct * 100.0));
                    }
                }
            }
        }
    }

    let status = {
        let mut guard = shared_child
            .lock()
            .map_err(|_| ToolError::failed("FFmpeg process lock poisoned", None))?;
        guard.wait().map_err(|e| {
            ToolError::failed("Failed waiting on FFmpeg process", Some(e.to_string()))
        })?
    };

    let stderr_text = stderr_handle.join().unwrap_or_default();

    if cancel.is_cancelled() {
        return Err(ToolError::Cancelled);
    }

    if !status.success() {
        return Err(ToolError::failed(
            "Media processing failed — the file may use an unsupported or damaged stream.",
            (!stderr_text.is_empty()).then_some(stderr_text),
        ));
    }

    Ok(())
}
