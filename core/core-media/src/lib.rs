//! Bundled LGPL FFmpeg sidecar wrapper, media metadata probing, and video/audio processing tools.

pub mod sidecar;

pub use sidecar::{
    decode_still_image_to_png, locate_ffmpeg_binary, probe_media_file, FfmpegBinary, MediaProbeInfo,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_locator_finds_vendor_or_system_ffmpeg() {
        let found = locate_ffmpeg_binary();
        assert!(
            found.is_ok(),
            "Expected bundled vendor or system ffmpeg binary to be located: {found:?}"
        );
    }
}
