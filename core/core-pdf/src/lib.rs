//! PDF rendering (`pdfium-render`) and structural manipulation (`lopdf`) tools.

pub mod preview;

pub use preview::{
    bind_pdfium, inspect_pdf_file, render_pdf_page_to_png, PdfInspectInfo, PdfRenderedPage,
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
    use std::fs;
    use tempfile::tempdir;

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
}
