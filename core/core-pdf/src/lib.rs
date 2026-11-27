//! PDF rendering (`pdfium-render`) and structural manipulation (`lopdf`) tools.

pub mod ops;
pub mod preview;
pub mod tools;

pub use preview::{
    bind_pdfium, inspect_pdf_file, render_pdf_page_to_png, PdfInspectInfo, PdfRenderedPage,
};
pub use tools::{
    register_pdf_tools, ImagesToPdfTool, PdfCompressTool, PdfExtractTool, PdfMergeTool,
    PdfProtectTool, PdfReorderTool, PdfRotateTool, PdfSplitTool, PdfToImagesTool, PdfUnlockTool,
    PdfWatermarkTool,
};

/// Helper for tests and internal fixtures to build a valid multi-page PDF document.
pub fn create_minimal_pdf_bytes(page_texts: &[&str]) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });

    let mut kids = Vec::new();
    for text in page_texts {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 24.into()]),
                Operation::new("Td", vec![72.into(), 720.into()]),
                Operation::new("Tj", vec![Object::string_literal(*text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        kids.push(page_id.into());
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_texts.len() as i64,
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_jobs::{CancelHandle, Tool, ToolContext, ToolOptions};
    use image::{ImageBuffer, Rgba};
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_ctx(inputs: Vec<PathBuf>, options: ToolOptions) -> ToolContext {
        ToolContext::new(
            "pdf-test-job",
            inputs,
            options,
            None,
            CancelHandle::new(),
            Arc::new(|_, _| {}),
        )
    }

    #[test]
    fn inspects_and_renders_generated_pdf_via_bundled_pdfium() {
        let dir = tempdir().unwrap();
        let pdf_path = dir.path().join("fixture.pdf");
        let bytes = create_minimal_pdf_bytes(&["Page One", "Page Two"]);
        fs::write(&pdf_path, bytes).unwrap();

        let info = inspect_pdf_file(&pdf_path).unwrap();
        assert_eq!(info.page_count, 2);
        assert!(!info.encrypted);

        if bind_pdfium().is_ok() {
            let rendered = render_pdf_page_to_png(&pdf_path, 0, 600, None).unwrap();
            assert_eq!(rendered.page_count, 2);
            assert!(rendered.width_px >= 500);
            assert!(rendered.png_bytes.starts_with(b"\x89PNG"));
        }
    }

    #[test]
    fn merge_split_extract_reorder_rotate_compress_watermark_and_images_roundtrip() {
        let dir = tempdir().unwrap();
        let p1 = dir.path().join("doc1.pdf");
        let p2 = dir.path().join("doc2.pdf");
        fs::write(&p1, create_minimal_pdf_bytes(&["A1", "A2"])).unwrap();
        fs::write(&p2, create_minimal_pdf_bytes(&["B1", "B2"])).unwrap();

        // 1. Merge (2 + 2 = 4 pages)
        let merged_res = PdfMergeTool
            .run(&make_ctx(vec![p1.clone(), p2], ToolOptions::new()))
            .unwrap();
        let merged_path = PathBuf::from(&merged_res.outputs[0].path);
        assert_eq!(inspect_pdf_file(&merged_path).unwrap().page_count, 4);

        // 2. Split (every 2 pages -> 2 files of 2 pages)
        let mut split_opts = ToolOptions::new();
        split_opts.insert("mode", json!("every_n"));
        split_opts.insert("pages_per_file", json!(2));
        let split_res = PdfSplitTool
            .run(&make_ctx(vec![merged_path.clone()], split_opts))
            .unwrap();
        assert_eq!(split_res.outputs.len(), 2);

        // 3. Extract pages 2-4 (3 pages)
        let mut ext_opts = ToolOptions::new();
        ext_opts.insert("pages", json!("2-4"));
        let ext_res = PdfExtractTool
            .run(&make_ctx(vec![merged_path.clone()], ext_opts))
            .unwrap();
        assert_eq!(
            inspect_pdf_file(Path::new(&ext_res.outputs[0].path))
                .unwrap()
                .page_count,
            3
        );

        // 4. Reorder (`reverse`)
        let mut re_opts = ToolOptions::new();
        re_opts.insert("order", json!("reverse"));
        let re_res = PdfReorderTool
            .run(&make_ctx(vec![merged_path.clone()], re_opts))
            .unwrap();
        assert_eq!(
            inspect_pdf_file(Path::new(&re_res.outputs[0].path))
                .unwrap()
                .page_count,
            4
        );

        // 5. Rotate 90°
        let mut rot_opts = ToolOptions::new();
        rot_opts.insert("angle", json!("90"));
        rot_opts.insert("pages", json!("1, 3"));
        let rot_res = PdfRotateTool
            .run(&make_ctx(vec![merged_path.clone()], rot_opts))
            .unwrap();
        assert!(Path::new(&rot_res.outputs[0].path).exists());

        // 6. Compress
        let comp_res = PdfCompressTool
            .run(&make_ctx(vec![merged_path.clone()], ToolOptions::new()))
            .unwrap();
        assert!(comp_res.outputs[0].size_bytes > 0);

        // 7. Watermark
        let mut wm_opts = ToolOptions::new();
        wm_opts.insert("text", json!("DRAFT"));
        let wm_res = PdfWatermarkTool
            .run(&make_ctx(vec![merged_path], wm_opts))
            .unwrap();
        assert_eq!(
            inspect_pdf_file(Path::new(&wm_res.outputs[0].path))
                .unwrap()
                .page_count,
            4
        );

        // 8. Images to PDF -> PDF to Images
        let img1 = dir.path().join("scan1.png");
        let img2 = dir.path().join("scan2.png");
        ImageBuffer::from_pixel(120, 160, Rgba([240u8, 240, 250, 255]))
            .save(&img1)
            .unwrap();
        ImageBuffer::from_pixel(120, 160, Rgba([200u8, 220, 240, 255]))
            .save(&img2)
            .unwrap();
        let i2p_res = ImagesToPdfTool
            .run(&make_ctx(vec![img1, img2], ToolOptions::new()))
            .unwrap();
        let created_pdf = PathBuf::from(&i2p_res.outputs[0].path);
        assert_eq!(inspect_pdf_file(&created_pdf).unwrap().page_count, 2);

        if bind_pdfium().is_ok() {
            let mut p2i_opts = ToolOptions::new();
            p2i_opts.insert("format", json!("png"));
            p2i_opts.insert("dpi", json!("96"));
            let p2i_res = PdfToImagesTool
                .run(&make_ctx(vec![created_pdf], p2i_opts))
                .unwrap();
            assert_eq!(p2i_res.outputs.len(), 2);
        }
    }

    #[test]
    fn protect_and_unlock_pdf_aes256_password_roundtrip() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("plain.pdf");
        fs::write(&src, create_minimal_pdf_bytes(&["Top Secret Page"])).unwrap();

        let mut protect_opts = ToolOptions::new();
        protect_opts.insert("password", json!("correct-horse-battery"));
        let protected_res = PdfProtectTool
            .run(&make_ctx(vec![src], protect_opts))
            .unwrap();
        let locked_path = PathBuf::from(&protected_res.outputs[0].path);
        assert!(locked_path.exists());

        // Wrong password must fail with clear user message
        let mut wrong_opts = ToolOptions::new();
        wrong_opts.insert("password", json!("wrong-pass"));
        let wrong_err = PdfUnlockTool
            .run(&make_ctx(vec![locked_path.clone()], wrong_opts))
            .unwrap_err();
        assert!(wrong_err.user_message().to_lowercase().contains("password"));

        // Correct password unlocks cleanly
        let mut unlock_opts = ToolOptions::new();
        unlock_opts.insert("password", json!("correct-horse-battery"));
        let unlocked_res = PdfUnlockTool
            .run(&make_ctx(vec![locked_path], unlock_opts))
            .unwrap();
        let unlocked_path = PathBuf::from(&unlocked_res.outputs[0].path);
        assert_eq!(inspect_pdf_file(&unlocked_path).unwrap().page_count, 1);
    }
}
