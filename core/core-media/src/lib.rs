//! Bundled LGPL FFmpeg sidecar wrapper and video/audio processing tools.

pub fn crate_name() -> &'static str {
    "core-media"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_crate_name() {
        assert_eq!(crate_name(), "core-media");
    }
}
