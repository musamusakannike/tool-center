use core_image::{encode_image_to_path, load_image_any};
use core_jobs::{CancelHandle, ToolError};
use lopdf::content::{Content, Operation};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::{dictionary, Document, Object, ObjectId, SaveOptions, Stream};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// Parses a 1-based page specification like `"1, 3, 5-8"` into 1-based page numbers within `1..=max_page`.
/// Supports `"all"` (all pages in order) and `"reverse"` (all pages reversed).
pub fn parse_page_selection(spec: &str, max_page: u32) -> Result<Vec<u32>, ToolError> {
    if max_page == 0 {
        return Err(ToolError::invalid("PDF document contains 0 pages."));
    }
    let trimmed = spec.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("all") {
        return Ok((1..=max_page).collect());
    }
    if trimmed.eq_ignore_ascii_case("reverse") {
        return Ok((1..=max_page).rev().collect());
    }

    let mut pages = Vec::new();
    for token in trimmed.split(',') {
        let part = token.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let start: u32 = a.trim().parse().map_err(|_| {
                ToolError::invalid(format!(
                    "Invalid page number '{}' in range '{part}'.",
                    a.trim()
                ))
            })?;
            let end: u32 = b.trim().parse().map_err(|_| {
                ToolError::invalid(format!(
                    "Invalid page number '{}' in range '{part}'.",
                    b.trim()
                ))
            })?;
            if start == 0 || end == 0 || start > max_page || end > max_page {
                return Err(ToolError::invalid(format!(
                    "Page range '{part}' is out of bounds (document has {max_page} page(s))."
                )));
            }
            if start <= end {
                pages.extend(start..=end);
            } else {
                pages.extend((end..=start).rev());
            }
        } else {
            let p: u32 = part
                .parse()
                .map_err(|_| ToolError::invalid(format!("Invalid page number '{part}'.")))?;
            if p == 0 || p > max_page {
                return Err(ToolError::invalid(format!(
                    "Page {p} is out of bounds (document has {max_page} page(s))."
                )));
            }
            pages.push(p);
        }
    }

    if pages.is_empty() {
        return Err(ToolError::invalid("No valid pages were selected."));
    }
    Ok(pages)
}

/// Loads a `lopdf::Document` with clear user-facing errors for corrupt or encrypted files.
pub fn load_lopdf_doc(path: &Path, password: Option<&str>) -> Result<Document, ToolError> {
    let res = match password.filter(|s| !s.is_empty()) {
        Some(pw) => Document::load_with_password(path, pw),
        None => Document::load(path),
    };
    res.map_err(|e| {
        let msg = e.to_string();
        if msg.to_lowercase().contains("password") || msg.to_lowercase().contains("encrypt") {
            ToolError::invalid(format!(
                "'{}' is password-protected. Unlock it first or supply the correct password.",
                path.file_name().and_then(|s| s.to_str()).unwrap_or("PDF")
            ))
        } else {
            ToolError::corrupt(path, "Invalid or corrupted PDF file.", Some(msg))
        }
    })
}

/// Builds a new PDF containing the requested 1-based pages (in exact requested order, allowing reordering or subsets)
/// from `source_path`.
pub fn build_subset_or_reordered_pdf(
    source_path: &Path,
    ordered_1based_pages: &[u32],
    out_path: &Path,
) -> Result<u64, ToolError> {
    if ordered_1based_pages.is_empty() {
        return Err(ToolError::invalid("Page list cannot be empty."));
    }

    let base_doc = load_lopdf_doc(source_path, None)?;
    let total = base_doc.get_pages().len() as u32;
    for &p in ordered_1based_pages {
        if p == 0 || p > total {
            return Err(ToolError::invalid(format!(
                "Page {p} exceeds document page count ({total})."
            )));
        }
    }

    // Check if selection has no duplicates and is strictly ascending -> fast `delete_pages` path
    let mut unique = HashSet::new();
    let strictly_increasing = ordered_1based_pages.windows(2).all(|w| w[0] < w[1])
        && ordered_1based_pages.iter().all(|p| unique.insert(*p));

    if strictly_increasing {
        let mut doc = base_doc;
        let to_delete: Vec<u32> = (1..=total).filter(|p| !unique.contains(p)).collect();
        if !to_delete.is_empty() {
            doc.delete_pages(&to_delete);
            doc.prune_objects();
        }
        return save_pdf_clean(&mut doc, out_path);
    }

    // General path (supports arbitrary page reordering & repeated pages):
    // Clone page tree kids array order when all pages are unique, or merge single-page slices.
    if unique.len() == ordered_1based_pages.len() {
        let mut doc = base_doc;
        let page_map = doc.get_pages();
        let to_delete: Vec<u32> = (1..=total).filter(|p| !unique.contains(p)).collect();
        if !to_delete.is_empty() {
            doc.delete_pages(&to_delete);
        }
        // Update root `/Pages` `/Kids` order to match `ordered_1based_pages`
        let new_kids: Vec<Object> = ordered_1based_pages
            .iter()
            .filter_map(|p| page_map.get(p).copied().map(Object::Reference))
            .collect();

        let pages_root_id = doc
            .catalog()
            .and_then(|cat| cat.get(b"Pages"))
            .and_then(Object::as_reference)
            .map_err(|e| {
                ToolError::corrupt(source_path, "Missing PDF Pages tree.", Some(e.to_string()))
            })?;

        // Ensure every page's `/Parent` points directly to `pages_root_id`
        for &p in ordered_1based_pages {
            if let Some(&pid) = page_map.get(&p) {
                if let Ok(page_dict) = doc.get_object_mut(pid).and_then(Object::as_dict_mut) {
                    page_dict.set("Parent", pages_root_id);
                }
            }
        }

        if let Ok(pages_dict) = doc
            .get_object_mut(pages_root_id)
            .and_then(Object::as_dict_mut)
        {
            pages_dict.set("Kids", new_kids);
            pages_dict.set("Count", ordered_1based_pages.len() as i64);
        }
        doc.prune_objects();
        return save_pdf_clean(&mut doc, out_path);
    }

    // Fallback when user repeats the same page multiple times (e.g. `1, 1, 2`)
    let mut single_docs = Vec::with_capacity(ordered_1based_pages.len());
    for &target_p in ordered_1based_pages {
        let mut copy = base_doc.clone();
        let others: Vec<u32> = (1..=total).filter(|&p| p != target_p).collect();
        if !others.is_empty() {
            copy.delete_pages(&others);
            copy.prune_objects();
        }
        single_docs.push(copy);
    }
    let mut merged = merge_lopdf_documents(single_docs)?;
    save_pdf_clean(&mut merged, out_path)
}

/// Merges multiple `lopdf::Document` instances into a single cohesive `Document`.
pub fn merge_lopdf_documents(documents: Vec<Document>) -> Result<Document, ToolError> {
    if documents.is_empty() {
        return Err(ToolError::invalid("At least one PDF document is required."));
    }

    let mut max_id = 1u32;
    let mut documents_pages: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut documents_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut merged = Document::with_version("1.5");

    for mut doc in documents {
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        let pages = doc.get_pages();
        for (_, object_id) in pages {
            if let Ok(obj) = doc.get_object(object_id) {
                documents_pages.insert(object_id, obj.to_owned());
            }
        }
        documents_objects.extend(doc.objects);
    }

    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    for (object_id, object) in documents_objects.iter() {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                catalog_object = Some((
                    if let Some((id, _)) = catalog_object {
                        id
                    } else {
                        *object_id
                    },
                    object.clone(),
                ));
            }
            b"Pages" => {
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();
                    if let Some((_, ref old_obj)) = pages_object {
                        if let Ok(old_dict) = old_obj.as_dict() {
                            dictionary.extend(old_dict);
                        }
                    }
                    pages_object = Some((
                        if let Some((id, _)) = pages_object {
                            id
                        } else {
                            *object_id
                        },
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                merged.objects.insert(*object_id, object.clone());
            }
        }
    }

    let (pages_id, pages_obj) =
        pages_object.ok_or_else(|| ToolError::failed("Merged PDF is missing Pages root.", None))?;
    let (catalog_id, catalog_obj) = catalog_object
        .ok_or_else(|| ToolError::failed("Merged PDF is missing Catalog root.", None))?;

    for (object_id, object) in documents_pages.iter() {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", pages_id);
            merged
                .objects
                .insert(*object_id, Object::Dictionary(dictionary));
        }
    }

    if let Ok(dictionary) = pages_obj.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Count", documents_pages.len() as i64);
        dictionary.set(
            "Kids",
            documents_pages
                .keys()
                .map(|&id| Object::Reference(id))
                .collect::<Vec<_>>(),
        );
        merged
            .objects
            .insert(pages_id, Object::Dictionary(dictionary));
    }

    if let Ok(dictionary) = catalog_obj.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Pages", pages_id);
        dictionary.remove(b"Outlines");
        merged
            .objects
            .insert(catalog_id, Object::Dictionary(dictionary));
    }

    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.keys().map(|(id, _)| *id).max().unwrap_or(1);
    merged.renumber_objects();
    merged.compress();
    Ok(merged)
}

/// Rotates selected 1-based pages of `source_path` by `delta_deg` (`90`, `180`, `270`).
pub fn rotate_pdf_pages(
    source_path: &Path,
    pages_spec: &str,
    delta_deg: i64,
    out_path: &Path,
) -> Result<u64, ToolError> {
    let mut doc = load_lopdf_doc(source_path, None)?;
    let page_map = doc.get_pages();
    let total = page_map.len() as u32;
    let selected: HashSet<u32> = parse_page_selection(pages_spec, total)?
        .into_iter()
        .collect();

    for (page_num, page_id) in page_map {
        if !selected.contains(&page_num) {
            continue;
        }
        if let Ok(dict) = doc.get_object_mut(page_id).and_then(Object::as_dict_mut) {
            let existing = dict.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
            let next = (existing + delta_deg).rem_euclid(360);
            dict.set("Rotate", next);
        }
    }

    save_pdf_clean(&mut doc, out_path)
}

/// Optimizes and compresses a PDF using object pruning, stream Flate compression, and PDF 1.5+ object/xref streams.
pub fn compress_pdf_file(source_path: &Path, out_path: &Path) -> Result<u64, ToolError> {
    let mut doc = load_lopdf_doc(source_path, None)?;
    doc.prune_objects();
    doc.delete_zero_length_streams();
    doc.renumber_objects();
    doc.compress();

    let mut file = File::create(out_path)
        .map_err(|e| ToolError::failed("Cannot create compressed PDF", Some(e.to_string())))?;
    let opts = SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .compression_level(9)
        .build();
    doc.save_with_options(&mut file, opts)
        .map_err(|e| ToolError::failed("Failed to write compressed PDF", Some(e.to_string())))?;

    Ok(fs::metadata(out_path).map(|m| m.len()).unwrap_or(0))
}

/// Stamps a clean vector text watermark across every page of `source_path`.
pub fn watermark_pdf_pages(
    source_path: &Path,
    text: &str,
    font_size: f32,
    opacity_pct: u32,
    diagonal: bool,
    position: &str,
    out_path: &Path,
) -> Result<u64, ToolError> {
    let clean_text = text.trim();
    if clean_text.is_empty() {
        return Err(ToolError::invalid("Watermark text cannot be empty."));
    }

    let mut doc = load_lopdf_doc(source_path, None)?;
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
    });

    let alpha = (opacity_pct.clamp(5, 100) as f32) / 100.0;
    let gs_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => alpha,
        "CA" => alpha,
    });

    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for page_id in page_ids {
        let (pw, ph) = extract_page_dimensions(&doc, page_id);

        // Attach `/TcWmFont` and `/TcWmGS` to the page's `/Resources` dictionary
        ensure_page_watermark_resources(&mut doc, page_id, font_id, gs_id);

        let est_text_w = clean_text.chars().count() as f32 * font_size * 0.55;
        let (tx, ty) = match position {
            "top_left" => (40.0, (ph - font_size - 36.0).max(24.0)),
            "top_right" => (
                (pw - est_text_w - 40.0).max(24.0),
                (ph - font_size - 36.0).max(24.0),
            ),
            "bottom_left" => (40.0, 36.0),
            "bottom_right" => ((pw - est_text_w - 40.0).max(24.0), 36.0),
            _ => {
                if diagonal {
                    ((pw * 0.25).max(36.0), (ph * 0.42).max(36.0))
                } else {
                    (((pw - est_text_w) * 0.5).max(24.0), (ph * 0.5).max(24.0))
                }
            }
        };

        let tm_ops: Vec<Object> = if diagonal {
            let c = std::f32::consts::FRAC_1_SQRT_2;
            vec![
                c.into(),
                c.into(),
                (-c).into(),
                c.into(),
                tx.into(),
                ty.into(),
            ]
        } else {
            vec![1.into(), 0.into(), 0.into(), 1.into(), tx.into(), ty.into()]
        };

        let wm_content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new("gs", vec![Object::Name(b"TcWmGS".to_vec())]),
                Operation::new("rg", vec![0.35.into(), 0.38.into(), 0.45.into()]),
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![Object::Name(b"TcWmFont".to_vec()), font_size.into()],
                ),
                Operation::new("Tm", tm_ops),
                Operation::new("Tj", vec![Object::string_literal(clean_text)]),
                Operation::new("ET", vec![]),
                Operation::new("Q", vec![]),
            ],
        };

        if let Ok(encoded) = wm_content.encode() {
            let _ =
                doc.add_to_page_content(page_id, Content::decode(&encoded).unwrap_or(wm_content));
        }
    }

    save_pdf_clean(&mut doc, out_path)
}

fn extract_page_dimensions(doc: &Document, page_id: ObjectId) -> (f32, f32) {
    if let Ok(dict) = doc.get_dictionary(page_id) {
        if let Ok(arr) = dict.get(b"MediaBox").and_then(Object::as_array) {
            if arr.len() == 4 {
                let w = obj_to_f32(&arr[2]).unwrap_or(595.0) - obj_to_f32(&arr[0]).unwrap_or(0.0);
                let h = obj_to_f32(&arr[3]).unwrap_or(842.0) - obj_to_f32(&arr[1]).unwrap_or(0.0);
                if w > 0.0 && h > 0.0 {
                    return (w, h);
                }
            }
        }
    }
    (595.0, 842.0)
}

fn obj_to_f32(obj: &Object) -> Option<f32> {
    obj.as_f32()
        .ok()
        .or_else(|| obj.as_i64().ok().map(|i| i as f32))
}

fn ensure_page_watermark_resources(
    doc: &mut Document,
    page_id: ObjectId,
    font_id: ObjectId,
    gs_id: ObjectId,
) {
    // Resolve or clone page `/Resources` dictionary directly onto the page so inherited resources aren't mutated globally
    let mut res_dict = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|d| d.get(b"Resources").ok())
        .and_then(|res_obj| match res_obj {
            Object::Reference(r) => doc.get_dictionary(*r).ok().cloned(),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let mut font_dict = res_dict
        .get(b"Font")
        .ok()
        .and_then(|o| match o {
            Object::Reference(r) => doc.get_dictionary(*r).ok().cloned(),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_default();
    font_dict.set("TcWmFont", font_id);
    res_dict.set("Font", Object::Dictionary(font_dict));

    let mut gs_dict = res_dict
        .get(b"ExtGState")
        .ok()
        .and_then(|o| match o {
            Object::Reference(r) => doc.get_dictionary(*r).ok().cloned(),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_default();
    gs_dict.set("TcWmGS", gs_id);
    res_dict.set("ExtGState", Object::Dictionary(gs_dict));

    if let Ok(page_dict) = doc.get_object_mut(page_id).and_then(Object::as_dict_mut) {
        page_dict.set("Resources", Object::Dictionary(res_dict));
    }
}

/// Builds a multi-page PDF from a list of input image files (`JPG`, `PNG`, `WebP`, `AVIF`, `TIFF`, `HEIC`, ...).
pub fn images_to_pdf_file(
    image_paths: &[std::path::PathBuf],
    page_size_mode: &str,
    margin_pt: f32,
    out_path: &Path,
    cancel: &CancelHandle,
    on_step: &dyn Fn(usize, usize),
) -> Result<u64, ToolError> {
    if image_paths.is_empty() {
        return Err(ToolError::invalid("Please select at least one image."));
    }

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::with_capacity(image_paths.len());

    let temp_jpg_dir = std::env::temp_dir().join(format!("tc_img2pdf_{}", Uuid::new_v4()));
    let _ = fs::create_dir_all(&temp_jpg_dir);

    for (idx, img_path) in image_paths.iter().enumerate() {
        cancel.check()?;
        on_step(idx, image_paths.len());

        let decoded = load_image_any(img_path)?;
        let (iw, ih) = (decoded.width().max(1), decoded.height().max(1));

        // Encode RGB8 JPEG via MozJPEG for compact high-fidelity `/DCTDecode` XObject embedding
        let tmp_jpg = temp_jpg_dir.join(format!("p_{idx}.jpg"));
        encode_image_to_path(&decoded, &tmp_jpg, "jpg", 90, false)?;
        let jpg_bytes = fs::read(&tmp_jpg).map_err(|e| {
            ToolError::failed("Failed to read encoded page image", Some(e.to_string()))
        })?;
        let _ = fs::remove_file(&tmp_jpg);

        let img_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => iw as i64,
                "Height" => ih as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpg_bytes,
        );
        let img_id = doc.add_object(img_stream);

        let (page_w, page_h, draw_x, draw_y, draw_w, draw_h) = match page_size_mode {
            "a4" => {
                let (pw, ph) = if iw > ih {
                    (841.89f32, 595.28f32)
                } else {
                    (595.28f32, 841.89f32)
                };
                let avail_w = (pw - margin_pt * 2.0).max(36.0);
                let avail_h = (ph - margin_pt * 2.0).max(36.0);
                let scale = (avail_w / iw as f32).min(avail_h / ih as f32);
                let dw = iw as f32 * scale;
                let dh = ih as f32 * scale;
                (pw, ph, (pw - dw) * 0.5, (ph - dh) * 0.5, dw, dh)
            }
            "letter" => {
                let (pw, ph) = if iw > ih {
                    (792.0f32, 612.0f32)
                } else {
                    (612.0f32, 792.0f32)
                };
                let avail_w = (pw - margin_pt * 2.0).max(36.0);
                let avail_h = (ph - margin_pt * 2.0).max(36.0);
                let scale = (avail_w / iw as f32).min(avail_h / ih as f32);
                let dw = iw as f32 * scale;
                let dh = ih as f32 * scale;
                (pw, ph, (pw - dw) * 0.5, (ph - dh) * 0.5, dw, dh)
            }
            _ => {
                // Fit page exactly to image (72 pt per 96 px ratio)
                let pw = (iw as f32 * 0.75) + margin_pt * 2.0;
                let ph = (ih as f32 * 0.75) + margin_pt * 2.0;
                (
                    pw,
                    ph,
                    margin_pt,
                    margin_pt,
                    iw as f32 * 0.75,
                    ih as f32 * 0.75,
                )
            }
        };

        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        draw_w.into(),
                        0.into(),
                        0.into(),
                        draw_h.into(),
                        draw_x.into(),
                        draw_y.into(),
                    ],
                ),
                Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content.encode().map_err(|e| {
                ToolError::failed("Failed to encode PDF page stream", Some(e.to_string()))
            })?,
        ));

        let resources_id = doc.add_object(dictionary! {
            "XObject" => dictionary! {
                "Im0" => img_id,
            },
        });

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), page_w.into(), page_h.into()],
        });
        kids.push(page_id.into());
    }

    let _ = fs::remove_dir_all(&temp_jpg_dir);

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => image_paths.len() as i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    save_pdf_clean(&mut doc, out_path)
}

/// Encrypts a PDF document with strong AES-256 (`PDF 2.0 V5 / R6`) password protection.
pub fn encrypt_pdf_aes256(
    source_path: &Path,
    user_password: &str,
    owner_password: &str,
    allow_printing: bool,
    allow_copying: bool,
    out_path: &Path,
) -> Result<u64, ToolError> {
    if user_password.is_empty() && owner_password.is_empty() {
        return Err(ToolError::invalid(
            "Please provide a password to protect the PDF.",
        ));
    }

    let mut doc = load_lopdf_doc(source_path, None)?;
    // Ensure PDF version >= 2.0 header for V5 / R6 AES-256 compatibility
    doc.version = "2.0".to_string();

    // Generate 32 CSPRNG bytes for the AES-256 file encryption key
    let mut fek = [0u8; 32];
    fek[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    fek[16..].copy_from_slice(Uuid::new_v4().as_bytes());

    let mut perms = Permissions::COPYABLE_FOR_ACCESSIBILITY;
    if allow_printing {
        perms |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
    }
    if allow_copying {
        perms |= Permissions::COPYABLE;
    }

    let owner_password = if owner_password.is_empty() {
        user_password
    } else {
        owner_password
    };

    let mut crypt_filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
    crypt_filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));

    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters,
        file_encryption_key: &fek,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password, // ggignore: runtime variable, not a hardcoded secret
        user_password,
        permissions: perms,
    })
    .map_err(|e| {
        ToolError::failed(
            "Failed to initialize AES-256 PDF encryption",
            Some(e.to_string()),
        )
    })?;

    doc.encrypt(&state)
        .map_err(|e| ToolError::failed("Failed to encrypt PDF objects", Some(e.to_string())))?;

    doc.save(out_path)
        .map_err(|e| ToolError::failed("Failed to save encrypted PDF", Some(e.to_string())))?;
    Ok(fs::metadata(out_path).map(|m| m.len()).unwrap_or(0))
}

/// Authenticates and removes password encryption from a protected PDF.
pub fn unlock_pdf_file(
    source_path: &Path,
    password: &str,
    out_path: &Path,
) -> Result<u64, ToolError> {
    let mut doc = Document::load_with_password(source_path, password).map_err(|e| {
        let msg = e.to_string();
        if msg.to_lowercase().contains("password") {
            ToolError::invalid("Incorrect PDF password.")
        } else {
            ToolError::corrupt(
                source_path,
                "Could not unlock or parse PDF file.",
                Some(msg),
            )
        }
    })?;

    if doc.is_encrypted() {
        doc.decrypt(password)
            .map_err(|_| ToolError::invalid("Incorrect PDF password."))?;
    }
    doc.trailer.remove(b"Encrypt");
    doc.encryption_state = None;

    save_pdf_clean(&mut doc, out_path)
}

pub fn save_pdf_clean(doc: &mut Document, out_path: &Path) -> Result<u64, ToolError> {
    doc.save(out_path)
        .map_err(|e| ToolError::failed("Failed to save output PDF", Some(e.to_string())))?;
    Ok(fs::metadata(out_path).map(|m| m.len()).unwrap_or(0))
}
