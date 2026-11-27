use core_jobs::ToolError;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Resolved path and runtime library directory for the LGPL FFmpeg sidecar.
#[derive(Debug, Clone)]
pub struct FfmpegBinary {
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: Option<PathBuf>,
    pub lib_dir: Option<PathBuf>,
}

impl FfmpegBinary {
    /// Creates a `Command` pre-configured with shared-library search paths (`LD_LIBRARY_PATH` /
    /// `DYLD_LIBRARY_PATH` / `PATH`) so LGPL shared builds run cleanly from any working directory.
    pub fn command_ffmpeg(&self) -> Command {
        let mut cmd = Command::new(&self.ffmpeg_path);
        self.apply_env(&mut cmd);
        cmd
    }

    pub fn command_ffprobe(&self) -> Option<Command> {
        let probe = self.ffprobe_path.as_ref()?;
        let mut cmd = Command::new(probe);
        self.apply_env(&mut cmd);
        Some(cmd)
    }

    fn apply_env(&self, cmd: &mut Command) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        if let Some(lib_dir) = &self.lib_dir {
            if cfg!(target_os = "linux") {
                let existing = env::var_os("LD_LIBRARY_PATH").unwrap_or_default();
                let mut paths = vec![lib_dir.clone()];
                paths.extend(env::split_paths(&existing));
                if let Ok(joined) = env::join_paths(paths) {
                    cmd.env("LD_LIBRARY_PATH", joined);
                }
            } else if cfg!(target_os = "macos") {
                let existing = env::var_os("DYLD_LIBRARY_PATH").unwrap_or_default();
                let mut paths = vec![lib_dir.clone()];
                paths.extend(env::split_paths(&existing));
                if let Ok(joined) = env::join_paths(paths) {
                    cmd.env("DYLD_LIBRARY_PATH", joined);
                }
            } else if cfg!(target_os = "windows") {
                let existing = env::var_os("PATH").unwrap_or_default();
                let mut paths = vec![lib_dir.clone()];
                paths.extend(env::split_paths(&existing));
                if let Ok(joined) = env::join_paths(paths) {
                    cmd.env("PATH", joined);
                }
            }
        }
    }
}

/// Searches bundled sidecar directories first, then `vendor/`, then system `PATH`.
pub fn locate_ffmpeg_binary() -> Result<FfmpegBinary, ToolError> {
    let exe_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let probe_name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };

    let mut candidate_roots: Vec<PathBuf> = Vec::new();

    if let Ok(custom) = env::var("TOOL_CENTER_FFMPEG_DIR") {
        candidate_roots.push(PathBuf::from(custom));
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // Linux & Windows bundle: <bundle>/data/ffmpeg
            candidate_roots.push(exe_dir.join("data").join("ffmpeg"));
            candidate_roots.push(exe_dir.join("ffmpeg"));
            // macOS .app bundle: Contents/MacOS/../Resources/ffmpeg
            candidate_roots.push(exe_dir.join("..").join("Resources").join("ffmpeg"));
        }
    }

    // Development / workspace vendor directory
    let host_target = host_vendor_target();
    if let Ok(cwd) = env::current_dir() {
        for ancestor in cwd.ancestors().take(5) {
            candidate_roots.push(ancestor.join("vendor").join(host_target).join("ffmpeg"));
            candidate_roots.push(
                ancestor
                    .join("vendor")
                    .join("macos-universal")
                    .join("ffmpeg"),
            );
        }
    }

    for root in candidate_roots {
        let bin_candidate = root.join("bin").join(exe_name);
        if bin_candidate.is_file() {
            let probe = root.join("bin").join(probe_name);
            let lib = root.join("lib");
            return Ok(FfmpegBinary {
                ffmpeg_path: bin_candidate,
                ffprobe_path: probe.is_file().then_some(probe),
                lib_dir: lib.is_dir().then_some(lib),
            });
        }
        let direct_candidate = root.join(exe_name);
        if direct_candidate.is_file() {
            let probe = root.join(probe_name);
            return Ok(FfmpegBinary {
                ffmpeg_path: direct_candidate,
                ffprobe_path: probe.is_file().then_some(probe),
                lib_dir: None,
            });
        }
    }

    // Fallback to system PATH
    if let Some(sys_ffmpeg) = find_in_path(exe_name) {
        let sys_probe = find_in_path(probe_name);
        return Ok(FfmpegBinary {
            ffmpeg_path: sys_ffmpeg,
            ffprobe_path: sys_probe,
            lib_dir: None,
        });
    }

    Err(ToolError::failed(
        "FFmpeg sidecar binary was not found. Run `python3 scripts/fetch_vendor.py` or bundle FFmpeg.",
        None,
    ))
}

fn find_in_path(bin_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let full = dir.join(bin_name);
        if full.is_file() {
            return Some(full);
        }
    }
    None
}

fn host_vendor_target() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-arm64"
    }
    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    {
        "linux-x64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-arm64"
    }
    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        "macos-x64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "windows-arm64"
    }
    #[cfg(all(target_os = "windows", not(target_arch = "aarch64")))]
    {
        "windows-x64"
    }
}

/// Extracted audio/video stream metadata for the file inspector and duration calculation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaProbeInfo {
    pub format_name: Option<String>,
    pub duration_secs: Option<f64>,
    pub bit_rate_kbps: Option<u64>,
    pub video_codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<String>,
    pub audio_codec: Option<String>,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u32>,
}

/// Probes container & codec metadata via `ffprobe` JSON output, falling back to `ffmpeg -i` stderr.
pub fn probe_media_file(path: &Path) -> Result<MediaProbeInfo, ToolError> {
    let sidecar = locate_ffmpeg_binary()?;
    if let Some(mut cmd) = sidecar.command_ffprobe() {
        let output = cmd
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| ToolError::failed("Failed to run ffprobe", Some(e.to_string())))?;

        if output.status.success() {
            if let Ok(parsed) = parse_ffprobe_json(&output.stdout) {
                return Ok(parsed);
            }
        }
    }

    // Fallback when only `ffmpeg` is bundled (e.g. single-binary macOS LGPL sidecar)
    let output = sidecar
        .command_ffmpeg()
        .args(["-hide_banner", "-i"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| ToolError::failed("Failed to run ffmpeg probe", Some(e.to_string())))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_ffmpeg_stderr_probe(&stderr, path)
}

fn parse_ffprobe_json(raw: &[u8]) -> Result<MediaProbeInfo, serde_json::Error> {
    let root: serde_json::Value = serde_json::from_slice(raw)?;
    let mut info = MediaProbeInfo::default();

    if let Some(fmt) = root.get("format") {
        info.format_name = fmt
            .get("format_long_name")
            .or_else(|| fmt.get("format_name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        info.duration_secs = fmt
            .get("duration")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok());
        info.bit_rate_kbps = fmt
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .map(|bps| bps / 1000);
    }

    if let Some(streams) = root.get("streams").and_then(|v| v.as_array()) {
        for stream in streams {
            let codec_type = stream
                .get("codec_type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if codec_type == "video" && info.video_codec.is_none() {
                info.video_codec = stream
                    .get("codec_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_uppercase());
                info.width = stream
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                info.height = stream
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                if let Some(rate) = stream.get("avg_frame_rate").and_then(|v| v.as_str()) {
                    if let Some((num, den)) = rate.split_once('/') {
                        if let (Ok(n), Ok(d)) = (num.parse::<f64>(), den.parse::<f64>()) {
                            if d > 0.0 && n > 0.0 {
                                info.fps = Some(format!("{:.2} fps", n / d));
                            }
                        }
                    }
                }
            } else if codec_type == "audio" && info.audio_codec.is_none() {
                info.audio_codec = stream
                    .get("codec_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_uppercase());
                info.sample_rate_hz = stream
                    .get("sample_rate")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u32>().ok());
                info.channels = stream
                    .get("channels")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
            }
        }
    }

    Ok(info)
}

fn parse_ffmpeg_stderr_probe(stderr: &str, path: &Path) -> Result<MediaProbeInfo, ToolError> {
    if stderr.contains("Invalid data found when processing input") {
        return Err(ToolError::corrupt(
            path,
            "Media file appears corrupted or unreadable.",
            Some(stderr.lines().last().unwrap_or_default().to_string()),
        ));
    }

    let mut info = MediaProbeInfo::default();
    for line in stderr.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Duration:") {
            if let Some(dur_str) = rest.split(',').next() {
                info.duration_secs = parse_hms_seconds(dur_str.trim());
            }
        } else if trimmed.contains("Video:") && info.video_codec.is_none() {
            if let Some(after) = trimmed.split("Video:").nth(1) {
                let codec = after
                    .split([',', ' '])
                    .find(|s| !s.is_empty())
                    .unwrap_or_default();
                if !codec.is_empty() {
                    info.video_codec = Some(codec.to_uppercase());
                }
            }
        } else if trimmed.contains("Audio:") && info.audio_codec.is_none() {
            if let Some(after) = trimmed.split("Audio:").nth(1) {
                let codec = after
                    .split([',', ' '])
                    .find(|s| !s.is_empty())
                    .unwrap_or_default();
                if !codec.is_empty() {
                    info.audio_codec = Some(codec.to_uppercase());
                }
            }
        }
    }
    Ok(info)
}

pub fn parse_hms_seconds(hms: &str) -> Option<f64> {
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() == 3 {
        let h = parts[0].parse::<f64>().ok()?;
        let m = parts[1].parse::<f64>().ok()?;
        let s = parts[2].parse::<f64>().ok()?;
        Some(h * 3600.0 + m * 60.0 + s)
    } else {
        hms.parse::<f64>().ok()
    }
}

/// Decodes a still image (such as `.heic`, `.heif`, or `.avif`) via the LGPL FFmpeg sidecar
/// into PNG bytes, optionally bounding the longest edge to `max_edge` pixels.
pub fn decode_still_image_to_png(path: &Path, max_edge: Option<u32>) -> Result<Vec<u8>, ToolError> {
    let sidecar = locate_ffmpeg_binary()?;
    let mut cmd = sidecar.command_ffmpeg();
    cmd.args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-frames:v", "1"]);

    if let Some(limit) = max_edge {
        let filter = format!(
            "scale='min({limit},iw)':'min({limit},ih)':force_original_aspect_ratio=decrease"
        );
        cmd.args(["-vf", &filter]);
    }

    let out = cmd
        .args(["-f", "image2pipe", "-vcodec", "png", "pipe:1"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            ToolError::failed("Failed to invoke FFmpeg image decoder", Some(e.to_string()))
        })?;

    if !out.status.success() || out.stdout.is_empty() {
        let details = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ToolError::corrupt(
            path,
            format!(
                "Could not decode image '{}'.",
                path.file_name().and_then(|s| s.to_str()).unwrap_or("file")
            ),
            (!details.is_empty()).then_some(details),
        ));
    }

    Ok(out.stdout)
}
