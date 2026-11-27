//! Image preview, EXIF inspection, HEIC/AVIF/WebP decoding, and image manipulation tools.

pub mod codecs;
pub mod preview;
pub mod tools;
pub mod watermark;

pub use codecs::{
    encode_image_to_path, normalize_output_extension, simd_resize,
    strip_metadata_preserving_pixels, IMAGE_INPUT_EXTENSIONS,
};
pub use preview::{
    extract_exif_summary, inspect_image_header, load_image_any, render_image_preview_png,
    ImageInspectInfo,
};
pub use tools::{
    register_image_tools, ImageBatchTool, ImageCompressTool, ImageConvertTool, ImageCropTool,
    ImageResizeTool, ImageRotateFlipTool, ImageStripExifTool, ImageWatermarkTool,
};

#[cfg(test)]
mod tests {
    use super::*;
    use core_jobs::{CancelHandle, Tool, ToolContext, ToolOptions};
    use image::{ImageBuffer, Rgba};
    use img_parts::jpeg::Jpeg;
    use img_parts::{Bytes, ImageEXIF};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_ctx(inputs: Vec<PathBuf>, options: ToolOptions) -> ToolContext {
        ToolContext::new(
            "test-job",
            inputs,
            options,
            None,
            CancelHandle::new(),
            Arc::new(|_, _| {}),
        )
    }

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

    #[test]
    fn resize_crop_rotate_convert_compress_watermark_and_batch_work_end_to_end() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("photo.png");
        let mut base = ImageBuffer::new(200, 120);
        for (x, y, px) in base.enumerate_pixels_mut() {
            *px = Rgba([(x % 255) as u8, (y % 255) as u8, 180, 255]);
        }
        base.save(&src).unwrap();

        // 1. Resize (`fast_image_resize`)
        let mut resize_opts = ToolOptions::new();
        resize_opts.insert("mode", json!("dimensions"));
        resize_opts.insert("width", json!(100));
        resize_opts.insert("height", json!(100));
        resize_opts.insert("keep_aspect", json!(true));
        let res = ImageResizeTool
            .run(&make_ctx(vec![src.clone()], resize_opts))
            .unwrap();
        let resized_img = load_image_any(std::path::Path::new(&res.outputs[0].path)).unwrap();
        assert_eq!((resized_img.width(), resized_img.height()), (100, 60));

        // 2. Crop (1:1 square -> 120x120)
        let mut crop_opts = ToolOptions::new();
        crop_opts.insert("mode", json!("aspect"));
        crop_opts.insert("aspect_ratio", json!("1:1"));
        let res = ImageCropTool
            .run(&make_ctx(vec![src.clone()], crop_opts))
            .unwrap();
        let cropped_img = load_image_any(std::path::Path::new(&res.outputs[0].path)).unwrap();
        assert_eq!((cropped_img.width(), cropped_img.height()), (120, 120));

        // 3. Rotate 90° + Flip H (200x120 -> 120x200)
        let mut rot_opts = ToolOptions::new();
        rot_opts.insert("rotation", json!("90"));
        rot_opts.insert("flip_horizontal", json!(true));
        let res = ImageRotateFlipTool
            .run(&make_ctx(vec![src.clone()], rot_opts))
            .unwrap();
        let rot_img = load_image_any(std::path::Path::new(&res.outputs[0].path)).unwrap();
        assert_eq!((rot_img.width(), rot_img.height()), (120, 200));

        // 4. Convert to AVIF & WebP
        for fmt in ["avif", "webp", "jpg"] {
            let mut conv_opts = ToolOptions::new();
            conv_opts.insert("format", json!(fmt));
            conv_opts.insert("quality", json!(80));
            let res = ImageConvertTool
                .run(&make_ctx(vec![src.clone()], conv_opts))
                .unwrap();
            assert!(PathBuf::from(&res.outputs[0].path).exists());
        }

        // 5. Compress PNG via OxiPNG
        let res = ImageCompressTool
            .run(&make_ctx(vec![src.clone()], ToolOptions::new()))
            .unwrap();
        assert!(res.outputs[0].size_bytes > 0);

        // 6. Watermark (text + image overlay)
        let mut wm_text_opts = ToolOptions::new();
        wm_text_opts.insert("watermark_type", json!("text"));
        wm_text_opts.insert("text", json!("© 2026 TOOL CENTER"));
        wm_text_opts.insert("position", json!("bottom_right"));
        let res = ImageWatermarkTool
            .run(&make_ctx(vec![src.clone()], wm_text_opts))
            .unwrap();
        assert!(PathBuf::from(&res.outputs[0].path).exists());

        let logo = dir.path().join("logo.png");
        ImageBuffer::from_pixel(32, 32, Rgba([255u8, 40, 40, 200]))
            .save(&logo)
            .unwrap();
        let mut wm_img_opts = ToolOptions::new();
        wm_img_opts.insert("watermark_type", json!("image"));
        wm_img_opts.insert("overlay_path", json!(logo.display().to_string()));
        wm_img_opts.insert("position", json!("top_left"));
        let res = ImageWatermarkTool
            .run(&make_ctx(vec![src.clone()], wm_img_opts))
            .unwrap();
        assert!(PathBuf::from(&res.outputs[0].path).exists());

        // 7. Batch pipeline
        let mut batch_opts = ToolOptions::new();
        batch_opts.insert("max_dimension", json!(80));
        batch_opts.insert("format", json!("jpg"));
        batch_opts.insert("quality", json!(75));
        let res = ImageBatchTool
            .run(&make_ctx(vec![src], batch_opts))
            .unwrap();
        let batch_img = load_image_any(std::path::Path::new(&res.outputs[0].path)).unwrap();
        assert_eq!(batch_img.width(), 80);
    }

    #[test]
    fn strip_exif_removes_embedded_exif_losslessly_from_jpeg() {
        let dir = tempdir().unwrap();
        let raw_jpg = dir.path().join("raw.jpg");
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(40, 40, Rgba([90, 140, 210, 255]));
        encode_image_to_path(
            &image::DynamicImage::ImageRgba8(img),
            &raw_jpg,
            "jpg",
            85,
            false,
        )
        .unwrap();

        // Inject a synthetic EXIF TIFF payload (`MM\x00\x2A...`)
        let jpg_bytes = fs::read(&raw_jpg).unwrap();
        let mut jpeg = Jpeg::from_bytes(Bytes::from(jpg_bytes)).unwrap();
        let dummy_exif = Bytes::from_static(b"MM\x00\x2a\x00\x00\x00\x08\x00\x00\x00\x00\x00\x00");
        jpeg.set_exif(Some(dummy_exif));
        let with_exif_path = dir.path().join("with_exif.jpg");
        jpeg.encoder()
            .write_to(fs::File::create(&with_exif_path).unwrap())
            .unwrap();

        let verify_before =
            Jpeg::from_bytes(Bytes::from(fs::read(&with_exif_path).unwrap())).unwrap();
        assert!(verify_before.exif().is_some());

        let res = ImageStripExifTool
            .run(&make_ctx(vec![with_exif_path], ToolOptions::new()))
            .unwrap();
        let cleaned_bytes = fs::read(&res.outputs[0].path).unwrap();
        let verify_after = Jpeg::from_bytes(Bytes::from(cleaned_bytes)).unwrap();
        assert!(verify_after.exif().is_none());
    }
}
