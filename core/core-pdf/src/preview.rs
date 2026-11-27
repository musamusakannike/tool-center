use core_jobs::ToolError;
use image::ImageFormat;
use lopdf::Document;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Summary metadata extracted from a PDF file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PdfInspectInfo {
    pub page_count: u32,
    pub pdf_version: String,
    pub encrypted: bool,
    pub title: Option<String>,
    pub author: Option<String>,
    pub producer: Option<String>,
}

/// Rendered single PDF page payload for the Flutter PDF viewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfRenderedPage {
    pub page_index: u32,
    pub page_count: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub png_bytes: Vec<u8>,
}

/// Inspects a PDF's page count, version, encryption flag, and document info dictionary.
pub fn inspect_pdf_file(path: &Path) -> Result<PdfInspectInfo, ToolError> {
    // Try fast structural parse via `lopdf` first; fall back to `pdfium-render` if needed.
    match Document::load(path) {
        Ok(doc) => {
            let encrypted = doc.trailer.get(b"Encrypt").is_ok() && doc.get_pages().is_empty();
            let mut info = PdfInspectInfo {
                page_count: doc.get_pages().len() as u32,
                pdf_version: doc.version.clone(),
                encrypted,
                ..Default::default()
            };
            if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
                if let Ok(dict) = doc.get_dictionary(info_ref) {
                    info.title = dict_string(dict, b"Title");
                    info.author = dict_string(dict, b"Author");
                    info.producer = dict_string(dict, b"Producer");
                }
            }
            if info.page_count == 0 {
                if let Ok(pdfium) = bind_pdfium() {
                    if let Ok(pdoc) = pdfium.load_pdf_from_file(path, None) {
                        info.page_count = pdoc.pages().len() as u32;
                    }
                }
            }
            Ok(info)
        }
        Err(lopdf_err) => {
            if let Ok(pdfium) = bind_pdfium() {
                match pdfium.load_pdf_from_file(path, None) {
                    Ok(pdoc) => Ok(PdfInspectInfo {
                        page_count: pdoc.pages().len() as u32,
                        pdf_version: format!("{:?}", pdoc.version()),
                        encrypted: false,
                        ..Default::default()
                    }),
                    Err(PdfiumError::PdfiumLibraryInternalError(
                        PdfiumInternalError::PasswordError,
                    )) => Ok(PdfInspectInfo {
                        page_count: 0,
                        pdf_version: "Encrypted".to_string(),
                        encrypted: true,
                        ..Default::default()
                    }),
                    Err(_) => Err(ToolError::corrupt(
                        path,
                        "Invalid or corrupted PDF document.",
                        Some(lopdf_err.to_string()),
                    )),
                }
            } else {
                Err(ToolError::corrupt(
                    path,
                    "Invalid or password-protected PDF document.",
                    Some(lopdf_err.to_string()),
                ))
            }
        }
    }
}

fn dict_string(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    let obj = dict.get(key).ok()?;
    let bytes = obj.as_str().ok()?;
    let text = String::from_utf8_lossy(bytes).trim().to_string();
    (!text.is_empty()).then_some(text)
}

static PDFIUM_INSTANCE: std::sync::OnceLock<Pdfium> = std::sync::OnceLock::new();

/// Returns a process-wide thread-safe `Pdfium` singleton bound to the bundled PDFium library.
pub fn bind_pdfium() -> Result<&'static Pdfium, ToolError> {
    if let Some(existing) = PDFIUM_INSTANCE.get() {
        return Ok(existing);
    }

    let initialized = init_pdfium_once()?;
    let _ = PDFIUM_INSTANCE.set(initialized);
    PDFIUM_INSTANCE
        .get()
        .ok_or_else(|| ToolError::failed("Failed to access initialized PDFium instance", None))
}

fn init_pdfium_once() -> Result<Pdfium, ToolError> {
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();

    if let Ok(custom) = env::var("TOOL_CENTER_PDFIUM_DIR") {
        candidate_dirs.push(PathBuf::from(custom));
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidate_dirs.push(exe_dir.to_path_buf());
            candidate_dirs.push(exe_dir.join("lib"));
            candidate_dirs.push(exe_dir.join("..").join("Frameworks"));
            candidate_dirs.push(exe_dir.join("..").join("Resources"));
        }
    }

    let host_target = host_vendor_target();
    if let Ok(cwd) = env::current_dir() {
        for ancestor in cwd.ancestors().take(5) {
            let base = ancestor.join("vendor").join(host_target).join("pdfium");
            candidate_dirs.push(base.join("lib"));
            candidate_dirs.push(base.join("bin"));
            candidate_dirs.push(base);
            let uni = ancestor
                .join("vendor")
                .join("macos-universal")
                .join("pdfium")
                .join("lib");
            candidate_dirs.push(uni);
        }
    }

    for dir in candidate_dirs {
        let lib_name = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if lib_name.exists() {
            if let Ok(bindings) = Pdfium::bind_to_library(&lib_name) {
                return Ok(Pdfium::new(bindings));
            }
        }
    }

    Pdfium::bind_to_system_library()
        .map(Pdfium::new)
        .map_err(|e| {
            ToolError::failed(
                "PDFium runtime library was not found. Run `python3 scripts/fetch_vendor.py` or bundle PDFium.",
                Some(e.to_string()),
            )
        })
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

/// Renders a single 0-indexed page of a PDF file to PNG bytes at `target_width` pixels.
pub fn render_pdf_page_to_png(
    path: &Path,
    page_index: u32,
    target_width: u32,
    password: Option<&str>,
) -> Result<PdfRenderedPage, ToolError> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(path, password)
        .map_err(|err| match err {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                ToolError::invalid("This PDF is password-protected. Please enter its password.")
            }
            other => ToolError::corrupt(
                path,
                "Could not open PDF document for rendering.",
                Some(other.to_string()),
            ),
        })?;

    let page_count = doc.pages().len() as u32;
    if page_count == 0 {
        return Err(ToolError::invalid("PDF document has 0 pages."));
    }
    if page_index >= page_count {
        return Err(ToolError::invalid(format!(
            "Requested page {} exceeds document page count ({page_count}).",
            page_index + 1
        )));
    }

    let page = doc
        .pages()
        .get(page_index as i32)
        .map_err(|e| ToolError::failed("Failed to load PDF page", Some(e.to_string())))?;

    let clamped_width = target_width.clamp(200, 3200) as i32;
    let render_config = PdfRenderConfig::new()
        .set_target_width(clamped_width)
        .set_maximum_height(4200)
        .render_form_data(true)
        .render_annotations(true);

    let bitmap = page
        .render_with_config(&render_config)
        .map_err(|e| ToolError::failed("Failed to rasterize PDF page", Some(e.to_string())))?;

    let dyn_image = bitmap
        .as_image()
        .map_err(|e| ToolError::failed("Failed to convert PDF bitmap", Some(e.to_string())))?;
    let (w, h) = (dyn_image.width(), dyn_image.height());
    let mut png_buf = Cursor::new(Vec::new());
    dyn_image
        .write_to(&mut png_buf, ImageFormat::Png)
        .map_err(|e| {
            ToolError::failed("Failed to encode rendered PDF page", Some(e.to_string()))
        })?;

    Ok(PdfRenderedPage {
        page_index,
        page_count,
        width_px: w,
        height_px: h,
        png_bytes: png_buf.into_inner(),
    })
}
