//! Image preview, decoding (including HEIC via LGPL sidecar), and manipulation tools.

pub fn crate_name() -> &'static str {
    "core-image"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_crate_name() {
        assert_eq!(crate_name(), "core-image");
    }
}
