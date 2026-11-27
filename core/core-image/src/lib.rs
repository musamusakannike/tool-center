//! Image preview, EXIF inspection, HEIC/AVIF/WebP decoding, and image manipulation tools.

pub mod preview;

pub use preview::{
    extract_exif_summary, inspect_image_header, load_image_any, render_image_preview_png,
    ImageInspectInfo,
};

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn inspects_and_renders_tiff_preview_and_rejects_corrupt_image() {
        let dir = tempdir().unwrap();
        let tiff_path = dir.path().join("sample.tiff");
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(64, 48, Rgba([20, 120, 220, 255]));
        img.save(&tiff_path).unwrap();

        let header = inspect_image_header(&tiff_path).unwrap();
        assert_eq!(header.width, Some(64));
        assert_eq!(header.height, Some(48));

        let preview_png = render_image_preview_png(&tiff_path, 512).unwrap();
        assert!(preview_png.starts_with(b"\x89PNG"));

        let corrupt_path = dir.path().join("broken.png");
        fs::write(&corrupt_path, b"not-a-valid-png").unwrap();
        let err = render_image_preview_png(&corrupt_path, 512).unwrap_err();
        assert!(err.user_message().contains("Corrupt or unsupported"));
    }
}
