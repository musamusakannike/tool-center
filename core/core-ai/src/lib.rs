//! v2 AI engine: versioned `models.json` manifest, permissive-license gate, resumable SHA-256
//! downloader, manual offline model import, and ONNX Runtime (`ort`) inference foundation.

pub mod bg_remove;
pub mod downloader;
pub mod manifest;
pub mod ocr;
pub mod onnx_fixture;
pub mod runtime;
pub mod upscale;

pub use bg_remove::AiRemoveBgTool;
pub use downloader::{
    sha256_file_streamed, AiModelsCatalogPayload, AiRuntimeHub, AiSettingsSnapshot,
    HttpModelTransport, ModelManager, ModelTransport, RangeRequest, ResumeMode,
};
pub use manifest::{
    flagged_excluded_models, format_human_bytes, validate_permissive_model_license,
    validate_sha256_hex, AiDevicePreference, AiModelError, AiModelSpec, AiModelStatus, AiTask,
    FlaggedNonCommercialModel, ModelFileEntry, ModelInstallState, ModelsManifest,
    EMBEDDED_MODELS_JSON,
};
pub use ocr::{AiOcrTool, OcrPageReport, OcrRegionItem};
pub use runtime::{
    ensure_ort_environment, locate_onnxruntime_dylib, query_available_providers,
    ActiveProviderInfo, OrtSessionHandle, SessionPool,
};
pub use upscale::{select_auto_tile_size, AiUpscaleTool, LANCZOS_FALLBACK_ID};

use core_jobs::{CancelHandle, ToolError, ToolRegistry};
use std::path::Path;
use std::sync::Arc;

/// Registers all v2 AI tools (`ai.ocr`, `ai.remove_bg`, `ai.upscale`) into the shared `ToolRegistry`.
pub fn register_ai_tools(registry: &ToolRegistry, hub: &Arc<AiRuntimeHub>) {
    registry.register(AiOcrTool::new(Arc::clone(hub)));
    registry.register(AiRemoveBgTool::new(Arc::clone(hub)));
    registry.register(AiUpscaleTool::new(Arc::clone(hub)));
}

/// Trait boundary for an ONNX Runtime (`ort`) execution session.
pub trait OnnxInferenceSession: Send + Sync {
    fn model_id(&self) -> &str;
    fn run_nchw_f32(
        &self,
        input_shape: [usize; 4],
        input_tensor: &[f32],
        cancel: &CancelHandle,
    ) -> Result<(Vec<usize>, Vec<f32>), ToolError>;
}

/// Trait boundary for creating ONNX Runtime sessions from verified local `.onnx` files.
pub trait OnnxRuntimeEngine: Send + Sync {
    fn load_session(
        &self,
        model_id: &str,
        onnx_path: &Path,
    ) -> Result<Box<dyn OnnxInferenceSession>, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tempfile::tempdir;

    fn sha256_bytes(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        format!("{:x}", h.finalize())
    }

    /// Mock HTTP Range server supporting mid-stream cancellation and `206 Partial Content` resume.
    struct RangeMockTransport {
        full_payload: Vec<u8>,
        cancel_after_bytes_on_first_try: Option<usize>,
        calls: AtomicUsize,
        last_resume_offset: AtomicUsize,
    }

    impl ModelTransport for RangeMockTransport {
        fn fetch_range_stream(
            &self,
            request: RangeRequest<'_>,
            open_sink: &mut dyn FnMut(ResumeMode) -> Result<Box<dyn Write>, AiModelError>,
            cancel: &CancelHandle,
            on_bytes_written: &mut dyn FnMut(u64),
        ) -> Result<(), AiModelError> {
            let resume_from = request.resume_from;
            let call_idx = self.calls.fetch_add(1, Ordering::SeqCst);
            self.last_resume_offset
                .store(resume_from as usize, Ordering::SeqCst);

            let mode = if resume_from > 0 {
                ResumeMode::AppendedFromOffset
            } else {
                ResumeMode::RestartedFromZero
            };
            let mut sink = open_sink(mode)?;
            let slice = &self.full_payload[resume_from as usize..];

            let mut written = 0usize;
            for chunk in slice.chunks(1024) {
                if call_idx == 0 {
                    if let Some(limit) = self.cancel_after_bytes_on_first_try {
                        if written >= limit {
                            cancel.cancel();
                            let _ = sink.flush();
                            return Err(AiModelError::Cancelled);
                        }
                    }
                }
                sink.write_all(chunk).map_err(|e| AiModelError::Io {
                    message: e.to_string(),
                })?;
                written += chunk.len();
                on_bytes_written(chunk.len() as u64);
            }
            sink.flush().map_err(|e| AiModelError::Io {
                message: e.to_string(),
            })?;
            Ok(())
        }
    }

    #[test]
    fn embedded_models_json_passes_strict_license_and_sha256_audit() {
        let dir = tempdir().unwrap();
        let mgr = ModelManager::with_embedded_manifest(dir.path()).unwrap();
        let models = mgr.list_models();
        assert!(models.len() >= 7, "Expected full v2 model catalog");

        for task in [AiTask::BackgroundRemoval, AiTask::Upscale, AiTask::Ocr] {
            assert!(
                models.iter().any(|m| m.spec.task == task),
                "Missing model for task {task:?}"
            );
            assert!(
                models
                    .iter()
                    .any(|m| m.spec.task == task && m.spec.default_for_task),
                "Missing default model for task {task:?}"
            );
        }

        // Verify flagged non-commercial / ambiguous models include RMBG and IS-Net/DIS
        let flagged = flagged_excluded_models();
        assert!(flagged.iter().any(|f| f.name.contains("RMBG")));
        assert!(flagged.iter().any(|f| f.name.contains("IS-Net")));
    }

    #[test]
    fn rejects_non_commercial_research_only_and_copyleft_licenses() {
        assert!(validate_permissive_model_license("m1", "MIT").is_ok());
        assert!(validate_permissive_model_license("m2", "Apache-2.0").is_ok());
        assert!(validate_permissive_model_license("m3", "BSD-3-Clause").is_ok());

        for disallowed in [
            "CC-BY-NC-4.0",
            "Research-Only",
            "Academic-NonCommercial",
            "GPL-3.0-only",
            "AGPL-3.0",
        ] {
            let err = validate_permissive_model_license("bad_model", disallowed).unwrap_err();
            assert!(matches!(err, AiModelError::DisallowedLicense { .. }));
        }
    }

    #[test]
    fn resumable_download_preserves_part_on_cancel_resumes_and_verifies_sha256() {
        let dir = tempdir().unwrap();
        let payload: Vec<u8> = (0..12_500u32).map(|i| (i % 251) as u8).collect();
        let digest = sha256_bytes(&payload);

        let manifest = ModelsManifest {
            schema_version: 1,
            updated_at: "2026-11-27".to_string(),
            models: vec![AiModelSpec {
                id: "bg.test_resume".to_string(),
                display_name: "Test Resumable Model".to_string(),
                task: AiTask::BackgroundRemoval,
                default_for_task: true,
                language: None,
                scale: None,
                url: "https://example.com/test_resume.onnx".to_string(),
                size_bytes: payload.len() as u64,
                sha256: digest.clone(),
                license: "Apache-2.0".to_string(),
                source_url: "https://example.com".to_string(),
                attribution: "Test".to_string(),
                description: "Resumable test".to_string(),
                files: vec![ModelFileEntry {
                    role: "primary".to_string(),
                    filename: "test_resume.onnx".to_string(),
                    url: "https://example.com/test_resume.onnx".to_string(),
                    size_bytes: payload.len() as u64,
                    sha256: digest,
                }],
            }],
        };

        let mgr = ModelManager::new(dir.path(), manifest).unwrap();
        let transport = RangeMockTransport {
            full_payload: payload.clone(),
            cancel_after_bytes_on_first_try: Some(4096),
            calls: AtomicUsize::new(0),
            last_resume_offset: AtomicUsize::new(0),
        };

        // 1st attempt: cancelled after 4096 bytes -> `.part` preserved!
        let cancel1 = CancelHandle::new();
        let err = mgr
            .download_model("bg.test_resume", &transport, &cancel1, |_, _, _| {})
            .unwrap_err();
        assert_eq!(err, AiModelError::Cancelled);
        assert_eq!(mgr.list_models()[0].resumable_bytes, 4096);
        assert!(!mgr.list_models()[0].installed);

        // 2nd attempt: resumes from exact byte 4096, verifies full SHA-256, atomically renames!
        let cancel2 = CancelHandle::new();
        let installed_dir = mgr
            .download_model("bg.test_resume", &transport, &cancel2, |_, _, _| {})
            .unwrap();
        assert_eq!(transport.last_resume_offset.load(Ordering::SeqCst), 4096);
        assert!(installed_dir.join("test_resume.onnx").is_file());
        assert!(!installed_dir.join("test_resume.onnx.part").exists());
        assert!(mgr.list_models()[0].installed);
    }

    #[test]
    fn corrupt_download_is_rejected_and_part_removed_and_offline_import_works() {
        let dir = tempdir().unwrap();
        let good_bytes = b"valid-onnx-model-binary-content".to_vec();
        let digest = sha256_bytes(&good_bytes);

        let manifest = ModelsManifest {
            schema_version: 1,
            updated_at: "2026-11-27".to_string(),
            models: vec![AiModelSpec {
                id: "upscale.test_import".to_string(),
                display_name: "Test Import Model".to_string(),
                task: AiTask::Upscale,
                default_for_task: true,
                language: None,
                scale: Some(2),
                url: "https://example.com/model.onnx".to_string(),
                size_bytes: good_bytes.len() as u64,
                sha256: digest.clone(),
                license: "BSD-3-Clause".to_string(),
                source_url: "https://example.com".to_string(),
                attribution: "Test".to_string(),
                description: "Offline import test".to_string(),
                files: vec![],
            }],
        };

        let mgr = ModelManager::new(dir.path().join("cache"), manifest).unwrap();

        // Corrupt download must fail with ChecksumMismatch and delete `.part`
        let bad_transport = RangeMockTransport {
            full_payload: b"tampered-bytes-not-matching!!".to_vec(),
            cancel_after_bytes_on_first_try: None,
            calls: AtomicUsize::new(0),
            last_resume_offset: AtomicUsize::new(0),
        };
        let bad_err = mgr
            .download_model(
                "upscale.test_import",
                &bad_transport,
                &CancelHandle::new(),
                |_, _, _| {},
            )
            .unwrap_err();
        assert!(matches!(bad_err, AiModelError::ChecksumMismatch { .. }));
        assert_eq!(mgr.list_models()[0].resumable_bytes, 0);

        // Offline import of wrong file is rejected
        let wrong_file = dir.path().join("wrong.onnx");
        fs::write(&wrong_file, b"wrong").unwrap();
        assert!(matches!(
            mgr.import_offline_model("upscale.test_import", &wrong_file),
            Err(AiModelError::ChecksumMismatch { .. })
        ));

        // Offline import of verified file succeeds atomically
        let valid_file = dir.path().join("offline_copy.onnx");
        fs::write(&valid_file, &good_bytes).unwrap();
        mgr.import_offline_model("upscale.test_import", &valid_file)
            .unwrap();
        assert!(mgr.list_models()[0].installed);

        // Delete removes installed directory cleanly
        assert!(mgr.delete_model("upscale.test_import").unwrap());
        assert!(!mgr.list_models()[0].installed);
    }

    #[test]
    fn ort_dynamic_runtime_executes_tiny_fixtures_falls_back_to_cpu_and_evicts_idle_sessions() {
        let dir = tempdir().unwrap();

        // 1. Write tiny ~200-byte synthetic ONNX fixtures to disk (zero network!)
        let mul_path = dir.path().join("mul3.onnx");
        let mat_path = dir.path().join("matting.onnx");
        let up2_path = dir.path().join("upscale2x.onnx");
        fs::write(&mul_path, onnx_fixture::build_tiny_mul_onnx(3.0)).unwrap();
        fs::write(&mat_path, onnx_fixture::build_tiny_matting_onnx()).unwrap();
        fs::write(&up2_path, onnx_fixture::build_tiny_upscale_onnx(2)).unwrap();

        let pool = SessionPool::new(Duration::from_secs(60));
        assert_eq!(pool.cached_count(), 0, "Pool starts completely empty");

        // 2. Request CUDA explicitly (unavailable in CPU CI) -> must fall back cleanly to CPU without crashing!
        let sess = pool
            .get_or_load("fixture.mul3", &mul_path, AiDevicePreference::Cuda)
            .unwrap();
        assert_eq!(sess.provider_info().active_provider, "cpu");
        assert!(sess.provider_info().fell_back_to_cpu);
        assert_eq!(pool.cached_count(), 1);

        let (shape, out) = sess
            .run_f32_tensor(&[1, 1, 2, 2], &[1.0, 2.0, 3.0, 4.0], &CancelHandle::new())
            .unwrap();
        assert_eq!(shape, vec![1, 1, 2, 2]);
        assert_eq!(out, vec![3.0, 6.0, 9.0, 12.0]);

        // 3. Matting fixture (`[1, 3, 4, 4]` -> `[1, 1, 4, 4]` in `[0..1]`)
        let mat_sess = pool
            .get_or_load("fixture.matting", &mat_path, AiDevicePreference::Auto)
            .unwrap();
        let (m_shape, m_out) = mat_sess
            .run_f32_tensor(&[1, 3, 4, 4], &[0.0f32; 48], &CancelHandle::new())
            .unwrap();
        assert_eq!(m_shape, vec![1, 1, 4, 4]);
        assert!((m_out[0] - 0.5).abs() < 1e-4, "Sigmoid(0.0) == 0.5");

        // 4. Upscale 2x fixture (`[1, 3, 4, 4]` -> `[1, 3, 8, 8]`)
        let up_sess = pool
            .get_or_load("fixture.up2", &up2_path, AiDevicePreference::Cpu)
            .unwrap();
        let (u_shape, u_out) = up_sess
            .run_f32_tensor(&[1, 3, 4, 4], &[0.75f32; 48], &CancelHandle::new())
            .unwrap();
        assert_eq!(u_shape, vec![1, 3, 8, 8]);
        assert_eq!(u_out.len(), 192);
        assert!((u_out[0] - 0.75).abs() < 1e-5);

        // 5. Cancellation check returns Err(ToolError::Cancelled)
        let cancelled = CancelHandle::new();
        cancelled.cancel();
        assert!(matches!(
            up_sess.run_f32_tensor(&[1, 3, 4, 4], &[0.0f32; 48], &cancelled),
            Err(ToolError::Cancelled)
        ));

        // 6. Drop strong leases and evict idle sessions -> memory released!
        drop(sess);
        drop(mat_sess);
        drop(up_sess);
        std::thread::sleep(Duration::from_millis(5));
        let evicted = pool.evict_idle_older_than(Duration::from_millis(1));
        assert_eq!(evicted, 3);
        assert_eq!(pool.cached_count(), 0);
    }

    #[test]
    fn ai_remove_bg_preserves_full_resolution_and_supports_all_background_modes() {
        use core_jobs::{Tool, ToolContext, ToolOptions};
        use image::{ImageBuffer, Rgba};
        use serde_json::json;
        use std::sync::Arc;

        let dir = tempdir().unwrap();
        let models_dir = dir.path().join("models");
        let work_dir = dir.path().join("work");
        fs::create_dir_all(&work_dir).unwrap();

        // Install synthetic matting ONNX fixture via SHA-256-verified offline import
        let onnx_bytes = onnx_fixture::build_tiny_matting_onnx();
        let sha = sha256_bytes(&onnx_bytes);
        let staged_onnx = dir.path().join("u2netp_fixture.onnx");
        fs::write(&staged_onnx, &onnx_bytes).unwrap();

        let manifest = ModelsManifest {
            schema_version: 1,
            updated_at: "2026-11-27".into(),
            models: vec![AiModelSpec {
                id: "bg.u2netp".into(),
                display_name: "U²-Netp Fixture".into(),
                task: AiTask::BackgroundRemoval,
                url: "https://example.com/u2netp.onnx".into(),
                size_bytes: onnx_bytes.len() as u64,
                sha256: sha.clone(),
                license: "Apache-2.0".into(),
                source_url: "https://github.com/xuebinqin/U-2-Net".into(),
                attribution: "U-2-Net (Apache-2.0)".into(),
                description: "Test fixture".into(),
                default_for_task: true,
                language: None,
                scale: None,
                files: vec![ModelFileEntry {
                    role: "primary".into(),
                    filename: "u2netp.onnx".into(),
                    url: "https://example.com/u2netp.onnx".into(),
                    size_bytes: onnx_bytes.len() as u64,
                    sha256: sha,
                }],
            }],
        };

        let hub = Arc::new(AiRuntimeHub::with_manifest(models_dir, manifest).unwrap());
        hub.manager()
            .unwrap()
            .import_offline_model("bg.u2netp", &staged_onnx)
            .unwrap();

        let tool = AiRemoveBgTool::new(Arc::clone(&hub));

        // Create high-resolution non-square subject image (540×380 > 320×320 inference square!)
        // Dark border -> low saliency (~0 alpha); bright center box -> high saliency (~1 alpha).
        let src_img_path = work_dir.join("subject.png");
        let mut img = ImageBuffer::from_pixel(540, 380, Rgba([10u8, 12, 15, 255]));
        for y in 90..290 {
            for x in 140..400 {
                img.put_pixel(x, y, Rgba([245, 240, 235, 255]));
            }
        }
        img.save(&src_img_path).unwrap();

        let src_img_path_2 = work_dir.join("subject_batch2.png");
        img.save(&src_img_path_2).unwrap();

        let bg_replacement_path = work_dir.join("backdrop.png");
        ImageBuffer::from_pixel(120, 90, Rgba([30u8, 140, 220, 255]))
            .save(&bg_replacement_path)
            .unwrap();

        let make_ctx = |inputs: Vec<std::path::PathBuf>, opts: ToolOptions| {
            ToolContext::new(
                "test-bg",
                inputs,
                opts,
                None,
                CancelHandle::new(),
                Arc::new(|_, _| {}),
            )
        };

        // 1. Transparent PNG batch mode (2 inputs) -> preserves exact 540×380 resolution & transparent corners
        let mut opts_trans = ToolOptions::new();
        opts_trans.insert("model_id", json!("bg.u2netp"));
        opts_trans.insert("bg_mode", json!("transparent"));
        let res_trans = tool
            .run(&make_ctx(
                vec![src_img_path.clone(), src_img_path_2.clone()],
                opts_trans,
            ))
            .unwrap();
        assert_eq!(res_trans.outputs.len(), 2);
        let out_trans = core_image::load_image_any(Path::new(&res_trans.outputs[0].path))
            .unwrap()
            .to_rgba8();
        assert_eq!((out_trans.width(), out_trans.height()), (540, 380));
        assert!(
            out_trans.get_pixel(5, 5)[3] < 20,
            "Dark background corner must become transparent"
        );
        assert!(
            out_trans.get_pixel(270, 190)[3] > 235,
            "Bright subject center must remain opaque"
        );

        // 2. Solid color mode (#108040)
        let mut opts_solid = ToolOptions::new();
        opts_solid.insert("model_id", json!("bg.u2netp"));
        opts_solid.insert("bg_mode", json!("solid_color"));
        opts_solid.insert("bg_color", json!("#108040"));
        let res_solid = tool
            .run(&make_ctx(vec![src_img_path.clone()], opts_solid))
            .unwrap();
        let out_solid = core_image::load_image_any(Path::new(&res_solid.outputs[0].path))
            .unwrap()
            .to_rgba8();
        assert_eq!((out_solid.width(), out_solid.height()), (540, 380));
        let corner_px = out_solid.get_pixel(5, 5);
        assert!((corner_px[0] as i32 - 0x10).abs() <= 4);
        assert!((corner_px[1] as i32 - 0x80).abs() <= 4);
        assert_eq!(corner_px[3], 255);

        // 3. Gaussian blur mode
        let mut opts_blur = ToolOptions::new();
        opts_blur.insert("model_id", json!("bg.u2netp"));
        opts_blur.insert("bg_mode", json!("blur"));
        opts_blur.insert("blur_sigma", json!(12));
        let res_blur = tool
            .run(&make_ctx(vec![src_img_path.clone()], opts_blur))
            .unwrap();
        let out_blur = core_image::load_image_any(Path::new(&res_blur.outputs[0].path))
            .unwrap()
            .to_rgba8();
        assert_eq!((out_blur.width(), out_blur.height()), (540, 380));
        assert_eq!(out_blur.get_pixel(270, 190)[3], 255);

        // 4. Custom background image mode
        let mut opts_img = ToolOptions::new();
        opts_img.insert("model_id", json!("bg.u2netp"));
        opts_img.insert("bg_mode", json!("custom_image"));
        opts_img.insert(
            "bg_image_path",
            json!(bg_replacement_path.display().to_string()),
        );
        let res_img = tool
            .run(&make_ctx(vec![src_img_path.clone()], opts_img))
            .unwrap();
        let out_img = core_image::load_image_any(Path::new(&res_img.outputs[0].path))
            .unwrap()
            .to_rgba8();
        assert_eq!((out_img.width(), out_img.height()), (540, 380));
        let bg_corner = out_img.get_pixel(5, 5);
        assert!((bg_corner[2] as i32 - 220).abs() <= 6);
    }

    #[test]
    fn ai_upscale_tiled_blending_has_zero_seams_and_lanczos_fallback_works() {
        use core_jobs::{Tool, ToolContext, ToolOptions};
        use image::{ImageBuffer, Rgba};
        use serde_json::json;
        use std::sync::Arc;

        let dir = tempdir().unwrap();
        let models_dir = dir.path().join("models");
        let work_dir = dir.path().join("work");
        fs::create_dir_all(&work_dir).unwrap();

        let onnx_bytes = onnx_fixture::build_tiny_upscale_onnx(2);
        let sha = sha256_bytes(&onnx_bytes);
        let staged_onnx = dir.path().join("esrgan_x2_fixture.onnx");
        fs::write(&staged_onnx, &onnx_bytes).unwrap();

        let manifest = ModelsManifest {
            schema_version: 1,
            updated_at: "2026-11-27".into(),
            models: vec![AiModelSpec {
                id: "upscale.realesrgan_x2plus".into(),
                display_name: "Real-ESRGAN x2plus Fixture".into(),
                task: AiTask::Upscale,
                url: "https://example.com/x2.onnx".into(),
                size_bytes: onnx_bytes.len() as u64,
                sha256: sha.clone(),
                license: "BSD-3-Clause".into(),
                source_url: "https://github.com/xinntao/Real-ESRGAN".into(),
                attribution: "Real-ESRGAN (BSD-3-Clause)".into(),
                description: "Test fixture".into(),
                default_for_task: true,
                language: None,
                scale: Some(2),
                files: vec![ModelFileEntry {
                    role: "primary".into(),
                    filename: "real_esrgan_x2.onnx".into(),
                    url: "https://example.com/x2.onnx".into(),
                    size_bytes: onnx_bytes.len() as u64,
                    sha256: sha,
                }],
            }],
        };

        let hub = Arc::new(AiRuntimeHub::with_manifest(models_dir, manifest).unwrap());
        let tool = AiUpscaleTool::new(Arc::clone(&hub));

        // 173×119 odd dimensions (not divisible by 4!) to exercise divisible-by-4 border padding + multi-tile blending
        let grad_path = work_dir.join("gradient_odd.png");
        let mut grad = ImageBuffer::new(173, 119);
        for (x, y, px) in grad.enumerate_pixels_mut() {
            let r = ((x * 200) / 173 + 20) as u8;
            let g = ((y * 180) / 119 + 30) as u8;
            *px = Rgba([r, g, 140, 255]);
        }
        grad.save(&grad_path).unwrap();

        let make_ctx = |inputs: Vec<std::path::PathBuf>, opts: ToolOptions| {
            ToolContext::new(
                "test-upscale",
                inputs,
                opts,
                None,
                CancelHandle::new(),
                Arc::new(|_, _| {}),
            )
        };

        // 1. Before ANY model is installed: Lanczos3 fallback works offline immediately!
        let mut fb_opts = ToolOptions::new();
        fb_opts.insert("scale", json!("2"));
        fb_opts.insert("model_id", json!(LANCZOS_FALLBACK_ID));
        let fb_res = tool
            .run(&make_ctx(vec![grad_path.clone()], fb_opts))
            .unwrap();
        let fb_out = core_image::load_image_any(Path::new(&fb_res.outputs[0].path))
            .unwrap()
            .to_rgba8();
        assert_eq!((fb_out.width(), fb_out.height()), (346, 238));

        // 2. Install ONNX fixture and run tiled 2× super-resolution with small 32px tiles (6×4 = 24 tiles!)
        hub.manager()
            .unwrap()
            .import_offline_model("upscale.realesrgan_x2plus", &staged_onnx)
            .unwrap();

        let mut tile_opts = ToolOptions::new();
        tile_opts.insert("scale", json!("2"));
        tile_opts.insert("model_id", json!("upscale.realesrgan_x2plus"));
        tile_opts.insert("tile_size", json!("32"));
        tile_opts.insert("output_format", json!("png"));
        let tile_res = tool
            .run(&make_ctx(vec![grad_path.clone()], tile_opts))
            .unwrap();
        let tile_out = core_image::load_image_any(Path::new(&tile_res.outputs[0].path))
            .unwrap()
            .to_rgba8();
        assert_eq!((tile_out.width(), tile_out.height()), (346, 238));

        // Verify zero seam spikes across all horizontal & vertical tile boundaries!
        for x in 1..(tile_out.width() - 1) {
            let left = tile_out.get_pixel(x - 1, 100)[0] as i32;
            let cur = tile_out.get_pixel(x, 100)[0] as i32;
            assert!(
                (cur - left).abs() <= 3,
                "Detected seam jump at x={x}: {left} -> {cur}"
            );
        }
        for y in 1..(tile_out.height() - 1) {
            let up = tile_out.get_pixel(150, y - 1)[1] as i32;
            let cur = tile_out.get_pixel(150, y)[1] as i32;
            assert!(
                (cur - up).abs() <= 4,
                "Detected seam jump at y={y}: {up} -> {cur}"
            );
        }

        // 3. Two-pass 4× enlargement with the x2 model -> exact 692×476
        let mut x4_opts = ToolOptions::new();
        x4_opts.insert("scale", json!("4"));
        x4_opts.insert("model_id", json!("upscale.realesrgan_x2plus"));
        x4_opts.insert("tile_size", json!("64"));
        let x4_res = tool
            .run(&make_ctx(vec![grad_path.clone()], x4_opts))
            .unwrap();
        let x4_out = core_image::load_image_any(Path::new(&x4_res.outputs[0].path))
            .unwrap()
            .to_rgba8();
        assert_eq!((x4_out.width(), x4_out.height()), (692, 476));
    }

    #[test]
    fn ai_ocr_processes_image_and_multipage_pdf_into_txt_md_and_searchable_pdf() {
        use core_jobs::{Tool, ToolContext, ToolOptions};
        use image::{ImageBuffer, Rgba};
        use serde_json::json;
        use std::sync::Arc;

        let dir = tempdir().unwrap();
        let models_dir = dir.path().join("models");
        let pack_staging = dir.path().join("ocr_pack_staging");
        let work_dir = dir.path().join("work");
        fs::create_dir_all(&pack_staging).unwrap();
        fs::create_dir_all(&work_dir).unwrap();

        let det_bytes = onnx_fixture::build_tiny_ocr_det_onnx();
        let cls_bytes = onnx_fixture::build_tiny_ocr_cls_onnx();
        let rec_bytes = onnx_fixture::build_tiny_ocr_rec_onnx();
        let dict_bytes = b"O\nK\n1\n".to_vec();

        fs::write(pack_staging.join("det.onnx"), &det_bytes).unwrap();
        fs::write(pack_staging.join("cls.onnx"), &cls_bytes).unwrap();
        fs::write(pack_staging.join("rec.onnx"), &rec_bytes).unwrap();
        fs::write(pack_staging.join("en_dict.txt"), &dict_bytes).unwrap();

        let total_size =
            (det_bytes.len() + cls_bytes.len() + rec_bytes.len() + dict_bytes.len()) as u64;

        let manifest = ModelsManifest {
            schema_version: 1,
            updated_at: "2026-11-27".into(),
            models: vec![AiModelSpec {
                id: "ocr.ppocr_v4_en".into(),
                display_name: "PP-OCRv4 English Fixture".into(),
                task: AiTask::Ocr,
                url: "https://example.com/rec.onnx".into(),
                size_bytes: total_size,
                sha256: sha256_bytes(&rec_bytes),
                license: "Apache-2.0".into(),
                source_url: "https://github.com/PaddlePaddle/PaddleOCR".into(),
                attribution: "PaddleOCR (Apache-2.0)".into(),
                description: "OCR test fixture".into(),
                default_for_task: true,
                language: Some("en".into()),
                scale: None,
                files: vec![
                    ModelFileEntry {
                        role: "det".into(),
                        filename: "det.onnx".into(),
                        url: "https://example.com/det.onnx".into(),
                        size_bytes: det_bytes.len() as u64,
                        sha256: sha256_bytes(&det_bytes),
                    },
                    ModelFileEntry {
                        role: "cls".into(),
                        filename: "cls.onnx".into(),
                        url: "https://example.com/cls.onnx".into(),
                        size_bytes: cls_bytes.len() as u64,
                        sha256: sha256_bytes(&cls_bytes),
                    },
                    ModelFileEntry {
                        role: "rec".into(),
                        filename: "rec.onnx".into(),
                        url: "https://example.com/rec.onnx".into(),
                        size_bytes: rec_bytes.len() as u64,
                        sha256: sha256_bytes(&rec_bytes),
                    },
                    ModelFileEntry {
                        role: "dict".into(),
                        filename: "en_dict.txt".into(),
                        url: "https://example.com/en_dict.txt".into(),
                        size_bytes: dict_bytes.len() as u64,
                        sha256: sha256_bytes(&dict_bytes),
                    },
                ],
            }],
        };

        let hub = Arc::new(AiRuntimeHub::with_manifest(models_dir, manifest).unwrap());
        hub.manager()
            .unwrap()
            .import_offline_model("ocr.ppocr_v4_en", &pack_staging)
            .unwrap();

        let tool = AiOcrTool::new(Arc::clone(&hub));

        // 1. Create a scanned image with 2 dark horizontal text bars on a white page
        let scan_img_path = work_dir.join("scan.png");
        let mut page = ImageBuffer::from_pixel(320, 224, Rgba([250u8, 250, 250, 255]));
        for y in 40..64 {
            for x in 36..240 {
                page.put_pixel(x, y, Rgba([12, 12, 12, 255]));
            }
        }
        for y in 120..144 {
            for x in 36..210 {
                page.put_pixel(x, y, Rgba([15, 15, 15, 255]));
            }
        }
        page.save(&scan_img_path).unwrap();

        let make_ctx = |inputs: Vec<std::path::PathBuf>, opts: ToolOptions| {
            ToolContext::new(
                "test-ocr",
                inputs,
                opts,
                None,
                CancelHandle::new(),
                Arc::new(|_, _| {}),
            )
        };

        // Test .txt & .md export + preprocessing toggles (contrast + deskew)
        let mut txt_opts = ToolOptions::new();
        txt_opts.insert("model_id", json!("ocr.ppocr_v4_en"));
        txt_opts.insert("output_format", json!("txt"));
        txt_opts.insert("enhance_contrast", json!(true));
        txt_opts.insert("deskew", json!(true));
        let txt_res = tool
            .run(&make_ctx(vec![scan_img_path.clone()], txt_opts))
            .unwrap();
        let txt_written = fs::read_to_string(&txt_res.outputs[0].path).unwrap();
        assert!(
            txt_written.contains("OK 1"),
            "Expected CTC decoded 'OK 1', got: {txt_written}"
        );
        assert_eq!(txt_written.lines().count(), 2, "Expected 2 detected lines");

        let mut md_opts = ToolOptions::new();
        md_opts.insert("model_id", json!("ocr.ppocr_v4_en"));
        md_opts.insert("output_format", json!("md"));
        let md_res = tool
            .run(&make_ctx(vec![scan_img_path.clone()], md_opts))
            .unwrap();
        let md_written = fs::read_to_string(&md_res.outputs[0].path).unwrap();
        assert!(md_written.starts_with("# OCR — scan.png"));
        assert!(md_written.contains("OK 1"));

        // 2. Multi-page PDF input streamed via `pdfium-render` -> Searchable PDF output via `lopdf`!
        let src_pdf_path = work_dir.join("two_page_scan.pdf");
        let pdf_bytes = core_pdf::create_minimal_pdf_bytes(&["HEADER LINE ONE", "SECOND PAGE DOC"]);
        fs::write(&src_pdf_path, pdf_bytes).unwrap();

        let mut spdf_opts = ToolOptions::new();
        spdf_opts.insert("model_id", json!("ocr.ppocr_v4_en"));
        spdf_opts.insert("output_format", json!("searchable_pdf"));
        let spdf_res = tool
            .run(&make_ctx(vec![src_pdf_path.clone()], spdf_opts))
            .unwrap();
        assert_eq!(spdf_res.outputs.len(), 1);
        let spdf_path = Path::new(&spdf_res.outputs[0].path);
        assert!(spdf_path.exists());

        // Inspect resulting Searchable PDF with `lopdf` to verify 2 pages, embedded Image XObject, and `3 Tr` selectable text!
        let out_doc = lopdf::Document::load(spdf_path).unwrap();
        assert_eq!(out_doc.get_pages().len(), 2);
        let extracted_p1 = out_doc.extract_text(&[1]).unwrap();
        assert!(
            extracted_p1.contains("OK 1"),
            "Searchable PDF page 1 text layer must contain recognized OCR text, got: '{extracted_p1}'"
        );
        let has_image_xobject = out_doc.objects.values().any(|obj| {
            obj.as_stream()
                .ok()
                .and_then(|s| s.dict.get(b"Subtype").ok())
                .and_then(|v| v.as_name().ok())
                == Some(b"Image".as_slice())
        });
        assert!(
            has_image_xobject,
            "Searchable PDF must preserve full-page scan image XObject"
        );
    }
}
