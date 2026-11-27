//! v2 AI engine: versioned `models.json` manifest, permissive-license gate, resumable SHA-256
//! downloader, manual offline model import, and ONNX Runtime (`ort`) inference foundation.

pub mod downloader;
pub mod manifest;
pub mod onnx_fixture;
pub mod runtime;

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
pub use runtime::{
    ensure_ort_environment, locate_onnxruntime_dylib, query_available_providers,
    ActiveProviderInfo, OrtSessionHandle, SessionPool,
};

use core_jobs::{CancelHandle, ToolError};
use std::path::Path;

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
}
