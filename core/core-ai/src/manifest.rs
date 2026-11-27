use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Embedded repo-root `models.json` manifest compiled into the binary as a reliable fallback.
pub const EMBEDDED_MODELS_JSON: &str = include_str!("../../../models.json");

const ALLOWED_PERMISSIVE_SPDX: &[&str] =
    &["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC"];

/// Supported AI tasks in Tool Center v2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTask {
    Ocr,
    BackgroundRemoval,
    Upscale,
}

impl AiTask {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::BackgroundRemoval => "background_removal",
            Self::Upscale => "upscale",
        }
    }
}

/// Hardware execution preference for ONNX Runtime sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiDevicePreference {
    #[default]
    Auto,
    Cpu,
    CoreMl,
    DirectMl,
    Cuda,
}

impl AiDevicePreference {
    pub fn from_str_lossy(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "cpu" => Self::Cpu,
            "coreml" | "core_ml" => Self::CoreMl,
            "directml" | "direct_ml" | "dml" => Self::DirectMl,
            "cuda" => Self::Cuda,
            _ => Self::Auto,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::CoreMl => "coreml",
            Self::DirectMl => "directml",
            Self::Cuda => "cuda",
        }
    }
}

/// Errors raised when validating, downloading, importing, or managing AI models.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiModelError {
    #[error("Model '{model_id}' rejected: license '{spdx_license}' is non-commercial or non-permissive ({reason})")]
    DisallowedLicense {
        model_id: String,
        spdx_license: String,
        reason: String,
    },

    #[error("Invalid model manifest entry for '{model_id}': {reason}")]
    InvalidManifest { model_id: String, reason: String },

    #[error("Unknown AI model '{model_id}'")]
    UnknownModel { model_id: String },

    #[error("Required AI model '{model_name}' ({size_human}) is not installed yet. Open AI Models or click Download Model to install it.")]
    ModelNotInstalled {
        model_id: String,
        model_name: String,
        size_bytes: u64,
        size_human: String,
    },

    #[error("Checksum verification failed for '{model_id}' ({filename}): expected SHA-256 {expected}, got {actual}. The corrupted download was removed — please retry.")]
    ChecksumMismatch {
        model_id: String,
        filename: String,
        expected: String,
        actual: String,
    },

    #[error("Model download cancelled (partial progress saved for instant resume)")]
    Cancelled,

    #[error("Network error while downloading '{model_id}': {message}")]
    Network { model_id: String, message: String },

    #[error("Model storage error: {message}")]
    Io { message: String },
}

/// Enforces Tool Center's strict permissive-only model license rule (`MIT`, `Apache-2.0`, `BSD`, `ISC`).
pub fn validate_permissive_model_license(
    model_id: &str,
    spdx_license: &str,
) -> Result<(), AiModelError> {
    let normalized = spdx_license.trim();
    let upper = normalized.to_ascii_uppercase();

    if upper.contains("NC")
        || upper.contains("NON-COMMERCIAL")
        || upper.contains("NONCOMMERCIAL")
        || upper.contains("RESEARCH")
        || upper.contains("ACADEMIC")
    {
        return Err(AiModelError::DisallowedLicense {
            model_id: model_id.to_string(),
            spdx_license: normalized.to_string(),
            reason: "Non-commercial or research-only restriction detected; flagged and excluded."
                .to_string(),
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

pub fn validate_sha256_hex(model_id: &str, sha256: &str) -> Result<(), AiModelError> {
    let trimmed = sha256.trim();
    if trimmed.len() != 64 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AiModelError::InvalidManifest {
            model_id: model_id.to_string(),
            reason: format!("SHA-256 '{trimmed}' must be a 64-character hexadecimal digest"),
        });
    }
    Ok(())
}

/// One downloadable file belonging to an `AiModelSpec` (single `.onnx` or multi-file OCR pack).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFileEntry {
    pub role: String,
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
}

/// Specification of a model entry from `models.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiModelSpec {
    pub id: String,
    pub display_name: String,
    pub task: AiTask,
    #[serde(default)]
    pub default_for_task: bool,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub scale: Option<u32>,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub license: String,
    pub source_url: String,
    pub attribution: String,
    pub description: String,
    #[serde(default)]
    pub files: Vec<ModelFileEntry>,
}

impl AiModelSpec {
    /// Returns normalized component files (synthesizing a primary entry if `files` was omitted).
    pub fn effective_files(&self) -> Vec<ModelFileEntry> {
        if !self.files.is_empty() {
            return self.files.clone();
        }
        let fallback_name = self
            .url
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}.onnx", self.id.replace('.', "_")));
        vec![ModelFileEntry {
            role: "primary".to_string(),
            filename: fallback_name,
            url: self.url.clone(),
            size_bytes: self.size_bytes,
            sha256: self.sha256.clone(),
        }]
    }

    pub fn validate(&self) -> Result<(), AiModelError> {
        validate_permissive_model_license(&self.id, &self.license)?;
        validate_sha256_hex(&self.id, &self.sha256)?;
        if self.size_bytes == 0 {
            return Err(AiModelError::InvalidManifest {
                model_id: self.id.clone(),
                reason: "size_bytes must be > 0".to_string(),
            });
        }
        let files = self.effective_files();
        let mut sum_bytes = 0u64;
        for f in &files {
            validate_sha256_hex(&self.id, &f.sha256)?;
            if f.filename.contains("..") || f.filename.contains('/') || f.filename.contains('\\') {
                return Err(AiModelError::InvalidManifest {
                    model_id: self.id.clone(),
                    reason: format!("Unsafe filename '{}'", f.filename),
                });
            }
            sum_bytes = sum_bytes.saturating_add(f.size_bytes);
        }
        if sum_bytes != self.size_bytes {
            return Err(AiModelError::InvalidManifest {
                model_id: self.id.clone(),
                reason: format!(
                    "Top-level size_bytes ({}) does not match sum of files ({})",
                    self.size_bytes, sum_bytes
                ),
            });
        }
        Ok(())
    }
}

/// Root versioned `models.json` manifest structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsManifest {
    pub schema_version: u32,
    pub updated_at: String,
    pub models: Vec<AiModelSpec>,
}

impl ModelsManifest {
    pub fn from_json_str(raw: &str) -> Result<Self, AiModelError> {
        let parsed: Self =
            serde_json::from_str(raw).map_err(|e| AiModelError::InvalidManifest {
                model_id: "models.json".to_string(),
                reason: e.to_string(),
            })?;
        if parsed.schema_version == 0 {
            return Err(AiModelError::InvalidManifest {
                model_id: "models.json".to_string(),
                reason: "schema_version must be >= 1".to_string(),
            });
        }
        for model in &parsed.models {
            model.validate()?;
        }
        Ok(parsed)
    }

    pub fn load_from_path_or_embedded(manifest_path: Option<&Path>) -> Result<Self, AiModelError> {
        if let Some(p) = manifest_path.filter(|p| p.is_file()) {
            let raw = fs::read_to_string(p).map_err(|e| AiModelError::Io {
                message: format!("Failed reading {}: {e}", p.display()),
            })?;
            return Self::from_json_str(&raw);
        }
        Self::from_json_str(EMBEDDED_MODELS_JSON)
    }
}

/// Installation lifecycle state for a model in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelInstallState {
    NotInstalled,
    Downloading,
    Ready,
    Failed,
}

/// Live status of a single model returned to the Flutter Models UI & tool prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelStatus {
    pub spec: AiModelSpec,
    pub state: ModelInstallState,
    pub installed: bool,
    pub resumable_bytes: u64,
    pub downloaded_bytes: u64,
    pub progress: f32,
    pub status_message: Option<String>,
    pub error_message: Option<String>,
    pub local_dir: Option<String>,
}

/// Explicitly flagged models documented so contributors never accidentally add them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlaggedNonCommercialModel {
    pub name: String,
    pub upstream_license: String,
    pub reason: String,
}

/// Returns non-commercial or license-ambiguous models explicitly flagged and excluded from Tool Center.
pub fn flagged_excluded_models() -> Vec<FlaggedNonCommercialModel> {
    vec![
        FlaggedNonCommercialModel {
            name: "BRIA RMBG-1.4 & RMBG-2.0".to_string(),
            upstream_license: "bria-rmbg (Non-Commercial)".to_string(),
            reason: "Restricts commercial use; excluded in favor of Apache-2.0 U²-Net and MIT BiRefNet."
                .to_string(),
        },
        FlaggedNonCommercialModel {
            name: "IS-Net / DIS (xuebinqin/DIS)".to_string(),
            upstream_license: "Code Apache-2.0 / Dataset DIS5K Academic Terms".to_string(),
            reason: "Upstream README specifies Apache-2.0 for code/metrics while referencing academic DIS5K dataset terms; left out due to weight license ambiguity."
                .to_string(),
        },
        FlaggedNonCommercialModel {
            name: "CC-BY-NC-4.0 Matting & OCR Checkpoints".to_string(),
            upstream_license: "CC-BY-NC-4.0".to_string(),
            reason: "Non-commercial Creative Commons restriction is incompatible with MIT/Apache-2.0."
                .to_string(),
        },
    ]
}

pub fn format_human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
