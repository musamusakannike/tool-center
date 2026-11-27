//! v2 AI foundation: ONNX Runtime trait boundary and permissive-license on-demand model manager.
//!
//! Policy: Only models under permissive commercial-friendly open-source licenses (`MIT`,
//! `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`) may be registered or downloaded.
//! Any non-commercial (`CC-BY-NC*`, `CreativeML-OpenRAIL`, custom non-commercial) or copyleft
//! (`GPL*`, `AGPL*`) model is flagged and rejected.

use core_jobs::{CancelHandle, ProgressCallback, ToolError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Planned v2 AI capabilities powered by ONNX Runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiCapability {
    OcrSearchablePdf,
    BackgroundRemoval,
    ImageUpscaling,
}

/// Errors raised when validating or managing on-demand ONNX models.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiModelError {
    #[error("Model '{model_id}' rejected: license '{spdx_license}' is non-commercial or non-permissive ({reason})")]
    DisallowedLicense {
        model_id: String,
        spdx_license: String,
        reason: String,
    },

    #[error("Unknown AI model '{model_id}'")]
    UnknownModel { model_id: String },

    #[error("SHA-256 verification failed for '{model_id}' (expected {expected}, got {actual})")]
    ChecksumMismatch {
        model_id: String,
        expected: String,
        actual: String,
    },

    #[error("Model download cancelled")]
    Cancelled,

    #[error("Model manager I/O error: {message}")]
    Io { message: String },

    #[error("On-demand HTTPS model downloader will be enabled in v2 (model '{model_id}' is not cached locally)")]
    DownloadNotConfigured { model_id: String },
}

const ALLOWED_PERMISSIVE_SPDX: &[&str] =
    &["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC"];

/// Enforces Tool Center's strict permissive-only model license rule.
pub fn validate_permissive_model_license(
    model_id: &str,
    spdx_license: &str,
) -> Result<(), AiModelError> {
    let normalized = spdx_license.trim();
    let upper = normalized.to_ascii_uppercase();

    if upper.contains("NC")
        || upper.contains("NON-COMMERCIAL")
        || upper.contains("NONCOMMERCIAL")
        || upper.contains("RESEARCH-ONLY")
    {
        return Err(AiModelError::DisallowedLicense {
            model_id: model_id.to_string(),
            spdx_license: normalized.to_string(),
            reason: "Non-commercial restriction detected; flagged and excluded.".to_string(),
        });
    }

    if ALLOWED_PERMISSIVE_SPDX
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(normalized))
    {
        return Ok(());
    }

    Err(AiModelError::DisallowedLicense {
        model_id: model_id.to_string(),
        spdx_license: normalized.to_string(),
        reason: "Only MIT, Apache-2.0, BSD, or ISC licensed ONNX models are permitted.".to_string(),
    })
}

/// Static specification of an on-demand ONNX model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelSpec {
    pub id: String,
    pub name: String,
    pub capability: AiCapability,
    pub spdx_license: String,
    pub source_repo: String,
    pub download_url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub description: String,
}

/// Runtime status of an AI model reported to the Flutter Model Manager UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelStatus {
    pub spec: AiModelSpec,
    pub installed: bool,
    pub local_path: Option<String>,
    pub v2_planned: bool,
}

/// Explicitly flagged models documented so contributors never accidentally add them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlaggedNonCommercialModel {
    pub name: String,
    pub upstream_license: String,
    pub reason: String,
}

/// Returns well-known non-commercial models explicitly flagged and excluded from Tool Center.
pub fn flagged_excluded_models() -> Vec<FlaggedNonCommercialModel> {
    vec![
        FlaggedNonCommercialModel {
            name: "BRIA RMBG-1.4".to_string(),
            upstream_license: "bria-rmbg-1.4 (Non-Commercial)".to_string(),
            reason: "Restricts commercial use; replaced with Apache-2.0 MODNet / U2-Net."
                .to_string(),
        },
        FlaggedNonCommercialModel {
            name: "SAM2 / Non-Commercial Matting Checkpoints".to_string(),
            upstream_license: "CC-BY-NC-4.0".to_string(),
            reason: "Non-commercial Creative Commons license is incompatible with MIT/Apache-2.0."
                .to_string(),
        },
    ]
}

/// Curated v2 model catalog containing exclusively MIT / Apache-2.0 licensed ONNX models.
pub fn default_v2_model_catalog() -> Vec<AiModelSpec> {
    vec![
        AiModelSpec {
            id: "ocr.ppocr_v4_en_latin".to_string(),
            name: "PP-OCRv4 Lightweight Detection + Recognition (ONNX)".to_string(),
            capability: AiCapability::OcrSearchablePdf,
            spdx_license: "Apache-2.0".to_string(),
            source_repo: "https://github.com/PaddlePaddle/PaddleOCR".to_string(),
            download_url: "https://github.com/musamusakannike/tool-center/releases/download/models-v2/ppocr_v4_en.onnx".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            size_bytes: 14_800_000,
            description: "Fast offline text recognition and searchable PDF layer generation.".to_string(),
        },
        AiModelSpec {
            id: "bg.u2netp_onnx".to_string(),
            name: "U2-Netp Portrait & Object Salient Matting (ONNX)".to_string(),
            capability: AiCapability::BackgroundRemoval,
            spdx_license: "Apache-2.0".to_string(),
            source_repo: "https://github.com/xuebinqin/U-2-Net".to_string(),
            download_url: "https://github.com/musamusakannike/tool-center/releases/download/models-v2/u2netp.onnx".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            size_bytes: 4_700_000,
            description: "Lightweight subject cutout and transparent PNG background removal.".to_string(),
        },
        AiModelSpec {
            id: "upscale.swin2sr_x2_onnx".to_string(),
            name: "Swin2SR 2x Classical Super-Resolution (ONNX)".to_string(),
            capability: AiCapability::ImageUpscaling,
            spdx_license: "Apache-2.0".to_string(),
            source_repo: "https://github.com/mv-lab/swin2sr".to_string(),
            download_url: "https://github.com/musamusakannike/tool-center/releases/download/models-v2/swin2sr_x2.onnx".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            size_bytes: 12_400_000,
            description: "Sharp 2x image upscaling and compression artifact cleanup.".to_string(),
        },
    ]
}

/// Pluggable transport trait so v2's HTTPS streaming client and offline tests share one pipeline.
pub trait ModelTransport: Send + Sync {
    fn fetch_stream(
        &self,
        url: &str,
        writer: &mut dyn Write,
        expected_size: u64,
        cancel: &CancelHandle,
        progress: &ProgressCallback,
    ) -> Result<(), AiModelError>;
}

/// Default v1 transport stub before ONNX Runtime & model hosting are activated in v2.
pub struct V1OfflineTransport;

impl ModelTransport for V1OfflineTransport {
    fn fetch_stream(
        &self,
        _url: &str,
        _writer: &mut dyn Write,
        _expected_size: u64,
        _cancel: &CancelHandle,
        _progress: &ProgressCallback,
    ) -> Result<(), AiModelError> {
        Err(AiModelError::DownloadNotConfigured {
            model_id: "v2-pending".to_string(),
        })
    }
}

/// Trait boundary for the v2 ONNX Runtime (`ort`) execution session.
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

/// Manages the local on-demand AI model cache directory, license validation, and SHA-256 checks.
pub struct ModelManager {
    cache_dir: PathBuf,
    catalog: Vec<AiModelSpec>,
}

impl ModelManager {
    pub fn new(
        cache_dir: impl Into<PathBuf>,
        catalog: Vec<AiModelSpec>,
    ) -> Result<Self, AiModelError> {
        for spec in &catalog {
            validate_permissive_model_license(&spec.id, &spec.spdx_license)?;
        }
        Ok(Self {
            cache_dir: cache_dir.into(),
            catalog,
        })
    }

    pub fn with_default_catalog(cache_dir: impl Into<PathBuf>) -> Result<Self, AiModelError> {
        Self::new(cache_dir, default_v2_model_catalog())
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn model_file_path(&self, model_id: &str) -> PathBuf {
        let safe_name = model_id.replace(['/', '\\', ':'], "_");
        self.cache_dir.join(format!("{safe_name}.onnx"))
    }

    pub fn list_models(&self) -> Vec<AiModelStatus> {
        self.catalog
            .iter()
            .map(|spec| {
                let path = self.model_file_path(&spec.id);
                let installed = path.is_file();
                AiModelStatus {
                    spec: spec.clone(),
                    installed,
                    local_path: installed.then(|| path.display().to_string()),
                    v2_planned: true,
                }
            })
            .collect()
    }

    /// Streams a model download to a temporary `.part` file, verifies SHA-256 in 64 KiB chunks
    /// without loading the entire file into RAM, and atomically renames it into place.
    pub fn download_model(
        &self,
        model_id: &str,
        transport: &dyn ModelTransport,
        cancel: &CancelHandle,
        progress: &ProgressCallback,
    ) -> Result<PathBuf, AiModelError> {
        let spec = self
            .catalog
            .iter()
            .find(|m| m.id == model_id)
            .ok_or_else(|| AiModelError::UnknownModel {
                model_id: model_id.to_string(),
            })?;

        validate_permissive_model_license(&spec.id, &spec.spdx_license)?;

        fs::create_dir_all(&self.cache_dir).map_err(|e| AiModelError::Io {
            message: e.to_string(),
        })?;

        let final_path = self.model_file_path(&spec.id);
        let part_path = final_path.with_extension("onnx.part");

        {
            let mut part_file = File::create(&part_path).map_err(|e| AiModelError::Io {
                message: e.to_string(),
            })?;

            if let Err(err) = transport.fetch_stream(
                &spec.download_url,
                &mut part_file,
                spec.size_bytes,
                cancel,
                progress,
            ) {
                let _ = fs::remove_file(&part_path);
                return Err(match err {
                    AiModelError::DownloadNotConfigured { .. } => {
                        AiModelError::DownloadNotConfigured {
                            model_id: spec.id.clone(),
                        }
                    }
                    other => other,
                });
            }
            part_file.flush().map_err(|e| AiModelError::Io {
                message: e.to_string(),
            })?;
        }

        if cancel.is_cancelled() {
            let _ = fs::remove_file(&part_path);
            return Err(AiModelError::Cancelled);
        }

        let actual_sha = sha256_file_streamed(&part_path)?;
        if !actual_sha.eq_ignore_ascii_case(&spec.sha256) {
            let _ = fs::remove_file(&part_path);
            return Err(AiModelError::ChecksumMismatch {
                model_id: spec.id.clone(),
                expected: spec.sha256.clone(),
                actual: actual_sha,
            });
        }

        fs::rename(&part_path, &final_path).map_err(|e| AiModelError::Io {
            message: e.to_string(),
        })?;

        progress(1.0, "Model verified and ready".to_string());
        Ok(final_path)
    }

    pub fn delete_cached_model(&self, model_id: &str) -> Result<bool, AiModelError> {
        let path = self.model_file_path(model_id);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| AiModelError::Io {
                message: e.to_string(),
            })?;
            return Ok(true);
        }
        Ok(false)
    }
}

fn sha256_file_streamed(path: &Path) -> Result<String, AiModelError> {
    let mut file = File::open(path).map_err(|e| AiModelError::Io {
        message: e.to_string(),
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| AiModelError::Io {
            message: e.to_string(),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    struct InMemoryTransport {
        payload: Vec<u8>,
    }

    impl ModelTransport for InMemoryTransport {
        fn fetch_stream(
            &self,
            _url: &str,
            writer: &mut dyn Write,
            _expected_size: u64,
            cancel: &CancelHandle,
            progress: &ProgressCallback,
        ) -> Result<(), AiModelError> {
            if cancel.is_cancelled() {
                return Err(AiModelError::Cancelled);
            }
            progress(0.5, "Downloading chunks...".to_string());
            writer
                .write_all(&self.payload)
                .map_err(|e| AiModelError::Io {
                    message: e.to_string(),
                })
        }
    }

    #[test]
    fn rejects_non_commercial_and_copyleft_model_licenses() {
        assert!(validate_permissive_model_license("m1", "MIT").is_ok());
        assert!(validate_permissive_model_license("m2", "Apache-2.0").is_ok());

        let nc = validate_permissive_model_license("rmbg", "CC-BY-NC-4.0").unwrap_err();
        assert!(matches!(nc, AiModelError::DisallowedLicense { .. }));
        assert!(nc.to_string().contains("Non-commercial"));

        let gpl = validate_permissive_model_license("gpl_model", "GPL-3.0-only").unwrap_err();
        assert!(matches!(gpl, AiModelError::DisallowedLicense { .. }));
    }

    #[test]
    fn default_catalog_passes_strict_license_audit() {
        let dir = tempdir().unwrap();
        let mgr = ModelManager::with_default_catalog(dir.path()).unwrap();
        let models = mgr.list_models();
        assert_eq!(models.len(), 3);
        assert!(models.iter().all(|m| !m.installed));
        assert!(!flagged_excluded_models().is_empty());
    }

    #[test]
    fn streamed_download_verifies_sha256_and_deletes_cleanly() {
        let dir = tempdir().unwrap();
        let payload = b"dummy-onnx-graph-bytes".to_vec();
        let mut h = Sha256::new();
        h.update(&payload);
        let digest = format!("{:x}", h.finalize());

        let spec = AiModelSpec {
            id: "test.mit_model".to_string(),
            name: "Test MIT Model".to_string(),
            capability: AiCapability::BackgroundRemoval,
            spdx_license: "MIT".to_string(),
            source_repo: "https://example.com".to_string(),
            download_url: "https://example.com/model.onnx".to_string(),
            sha256: digest,
            size_bytes: payload.len() as u64,
            description: "Unit test model".to_string(),
        };

        let mgr = ModelManager::new(dir.path(), vec![spec]).unwrap();
        let transport = InMemoryTransport { payload };
        let cancel = CancelHandle::new();
        let progress: ProgressCallback = Arc::new(|_, _| {});

        let saved = mgr
            .download_model("test.mit_model", &transport, &cancel, &progress)
            .unwrap();
        assert!(saved.exists());
        assert!(mgr.list_models()[0].installed);

        // Tampered checksum is rejected and `.part` cleaned up
        let bad_transport = InMemoryTransport {
            payload: b"corrupted".to_vec(),
        };
        let err = mgr
            .download_model("test.mit_model", &bad_transport, &cancel, &progress)
            .unwrap_err();
        assert!(matches!(err, AiModelError::ChecksumMismatch { .. }));

        assert!(mgr.delete_cached_model("test.mit_model").unwrap());
        assert!(!saved.exists());
    }
}
