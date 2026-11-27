use crate::manifest::{
    flagged_excluded_models, format_human_bytes, validate_permissive_model_license,
    AiDevicePreference, AiModelError, AiModelSpec, AiModelStatus, ModelFileEntry,
    ModelInstallState, ModelsManifest,
};
use core_jobs::CancelHandle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::broadcast;

/// Indicates whether a transport honored `Range: bytes=N-` or restarted from byte 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeMode {
    AppendedFromOffset,
    RestartedFromZero,
}

/// Parameters describing an HTTP range download request.
#[derive(Debug, Clone, Copy)]
pub struct RangeRequest<'a> {
    pub model_id: &'a str,
    pub url: &'a str,
    pub resume_from: u64,
    pub expected_total_size: u64,
}

/// Pluggable streaming HTTP transport supporting HTTP `Range` resumption and cancellation.
pub trait ModelTransport: Send + Sync {
    fn fetch_range_stream(
        &self,
        request: RangeRequest<'_>,
        open_sink: &mut dyn FnMut(ResumeMode) -> Result<Box<dyn Write>, AiModelError>,
        cancel: &CancelHandle,
        on_bytes_written: &mut dyn FnMut(u64),
    ) -> Result<(), AiModelError>;
}

/// Production HTTPS model downloader powered by `ureq` + pure-Rust `rustls`.
#[derive(Default, Clone)]
pub struct HttpModelTransport;

impl ModelTransport for HttpModelTransport {
    fn fetch_range_stream(
        &self,
        request: RangeRequest<'_>,
        open_sink: &mut dyn FnMut(ResumeMode) -> Result<Box<dyn Write>, AiModelError>,
        cancel: &CancelHandle,
        on_bytes_written: &mut dyn FnMut(u64),
    ) -> Result<(), AiModelError> {
        let model_id = request.model_id;
        let url = request.url;
        let resume_from = request.resume_from;
        if cancel.is_cancelled() {
            return Err(AiModelError::Cancelled);
        }

        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .build()
            .new_agent();

        let mut req = agent
            .get(url)
            .header("User-Agent", "ToolCenter-ModelManager/0.2");

        if resume_from > 0 {
            req = req.header("Range", &format!("bytes={resume_from}-"));
        }

        let mut resp = req.call().map_err(|e| AiModelError::Network {
            model_id: model_id.to_string(),
            message: format!("Connection failed: {e}"),
        })?;

        let status = resp.status().as_u16();
        let mode = if resume_from > 0 && status == 206 {
            ResumeMode::AppendedFromOffset
        } else if status == 200 {
            ResumeMode::RestartedFromZero
        } else {
            return Err(AiModelError::Network {
                model_id: model_id.to_string(),
                message: format!("Server returned HTTP {status} for {url}"),
            });
        };

        let mut sink = open_sink(mode)?;
        let mut reader = resp.body_mut().as_reader();
        let mut buf = [0u8; 64 * 1024];

        loop {
            if cancel.is_cancelled() {
                let _ = sink.flush();
                return Err(AiModelError::Cancelled);
            }
            let n = reader.read(&mut buf).map_err(|e| AiModelError::Network {
                model_id: model_id.to_string(),
                message: format!("Download stream interrupted: {e}"),
            })?;
            if n == 0 {
                break;
            }
            sink.write_all(&buf[..n]).map_err(|e| AiModelError::Io {
                message: e.to_string(),
            })?;
            on_bytes_written(n as u64);
        }

        sink.flush().map_err(|e| AiModelError::Io {
            message: e.to_string(),
        })?;

        if cancel.is_cancelled() {
            return Err(AiModelError::Cancelled);
        }
        Ok(())
    }
}

/// Manages local on-demand AI model storage, resumable downloads, SHA-256 verification, and manual imports.
#[derive(Clone)]
pub struct ModelManager {
    models_dir: PathBuf,
    manifest: ModelsManifest,
}

impl ModelManager {
    pub fn new(
        models_dir: impl Into<PathBuf>,
        manifest: ModelsManifest,
    ) -> Result<Self, AiModelError> {
        for spec in &manifest.models {
            spec.validate()?;
        }
        Ok(Self {
            models_dir: models_dir.into(),
            manifest,
        })
    }

    pub fn with_embedded_manifest(models_dir: impl Into<PathBuf>) -> Result<Self, AiModelError> {
        let manifest = ModelsManifest::load_from_path_or_embedded(None)?;
        Self::new(models_dir, manifest)
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    pub fn manifest(&self) -> &ModelsManifest {
        &self.manifest
    }

    pub fn find_spec(&self, model_id: &str) -> Option<&AiModelSpec> {
        self.manifest.models.iter().find(|m| m.id == model_id)
    }

    /// Directory dedicated to a specific model ID inside `models_dir`.
    pub fn model_dir(&self, model_id: &str) -> PathBuf {
        let safe_id = model_id.replace(['/', '\\', ':', '.'], "_");
        self.models_dir.join(safe_id)
    }

    /// Final path of a component file inside the model's directory.
    pub fn component_path(&self, model_id: &str, filename: &str) -> PathBuf {
        self.model_dir(model_id).join(filename)
    }

    /// Temporary `.part` path used for resumable downloads and atomic installation.
    pub fn component_part_path(&self, model_id: &str, filename: &str) -> PathBuf {
        self.model_dir(model_id).join(format!("{filename}.part"))
    }

    /// Checks if every required file for `spec` is present on disk with matching byte size.
    pub fn is_spec_installed(&self, spec: &AiModelSpec) -> bool {
        spec.effective_files().iter().all(|f| {
            let p = self.component_path(&spec.id, &f.filename);
            fs::metadata(&p)
                .map(|m| m.is_file() && m.len() == f.size_bytes)
                .unwrap_or(false)
        })
    }

    /// Resolves verified local paths by role (`primary`, `det`, `cls`, `rec`, `dict`) or returns `ModelNotInstalled`.
    pub fn require_installed_files(
        &self,
        model_id: &str,
    ) -> Result<HashMap<String, PathBuf>, AiModelError> {
        let spec = self
            .find_spec(model_id)
            .ok_or_else(|| AiModelError::UnknownModel {
                model_id: model_id.to_string(),
            })?;
        if !self.is_spec_installed(spec) {
            return Err(AiModelError::ModelNotInstalled {
                model_id: spec.id.clone(),
                model_name: spec.display_name.clone(),
                size_bytes: spec.size_bytes,
                size_human: format_human_bytes(spec.size_bytes),
            });
        }
        let mut map = HashMap::new();
        for f in spec.effective_files() {
            map.insert(f.role.clone(), self.component_path(&spec.id, &f.filename));
        }
        Ok(map)
    }

    /// Computes already-saved bytes (installed components + partial `.part` files) for resume display.
    pub fn resumable_bytes_on_disk(&self, spec: &AiModelSpec) -> u64 {
        let mut total = 0u64;
        for f in spec.effective_files() {
            let final_p = self.component_path(&spec.id, &f.filename);
            if let Ok(meta) = fs::metadata(&final_p) {
                if meta.is_file() && meta.len() == f.size_bytes {
                    total += f.size_bytes;
                    continue;
                }
            }
            let part_p = self.component_part_path(&spec.id, &f.filename);
            if let Ok(meta) = fs::metadata(&part_p) {
                if meta.is_file() {
                    total += meta.len().min(f.size_bytes);
                }
            }
        }
        total
    }

    pub fn list_models(&self) -> Vec<AiModelStatus> {
        self.manifest
            .models
            .iter()
            .map(|spec| {
                let installed = self.is_spec_installed(spec);
                let resumable = if installed {
                    spec.size_bytes
                } else {
                    self.resumable_bytes_on_disk(spec)
                };
                let dir = self.model_dir(&spec.id);
                AiModelStatus {
                    spec: spec.clone(),
                    state: if installed {
                        ModelInstallState::Ready
                    } else {
                        ModelInstallState::NotInstalled
                    },
                    installed,
                    resumable_bytes: resumable,
                    downloaded_bytes: if installed {
                        spec.size_bytes
                    } else {
                        resumable
                    },
                    progress: if installed {
                        1.0
                    } else if spec.size_bytes > 0 {
                        (resumable as f32 / spec.size_bytes as f32).clamp(0.0, 0.99)
                    } else {
                        0.0
                    },
                    status_message: None,
                    error_message: None,
                    local_dir: installed.then(|| dir.display().to_string()),
                }
            })
            .collect()
    }

    /// Resumable, cancellable, SHA-256-verified, atomic model installer.
    ///
    /// - Writes to `<filename>.part`. If a partial `.part` exists, requests `Range: bytes=N-`.
    /// - On cancel, preserves `.part` on disk so the next download resumes immediately.
    /// - On SHA-256 mismatch, deletes `.part` and returns `AiModelError::ChecksumMismatch`.
    /// - On SHA-256 match, atomically renames `<filename>.part` -> `<filename>`.
    pub fn download_model<F>(
        &self,
        model_id: &str,
        transport: &dyn ModelTransport,
        cancel: &CancelHandle,
        mut on_progress: F,
    ) -> Result<PathBuf, AiModelError>
    where
        F: FnMut(u64, u64, String),
    {
        let spec = self
            .find_spec(model_id)
            .ok_or_else(|| AiModelError::UnknownModel {
                model_id: model_id.to_string(),
            })?
            .clone();

        validate_permissive_model_license(&spec.id, &spec.license)?;

        let model_dir = self.model_dir(&spec.id);
        fs::create_dir_all(&model_dir).map_err(|e| AiModelError::Io {
            message: format!("Cannot create model folder '{}': {e}", model_dir.display()),
        })?;

        let files = spec.effective_files();
        let total_pack_bytes = spec.size_bytes;
        let mut completed_prior_files_bytes = 0u64;

        for (idx, entry) in files.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(AiModelError::Cancelled);
            }

            let final_path = self.component_path(&spec.id, &entry.filename);
            if let Ok(meta) = fs::metadata(&final_path) {
                if meta.is_file() && meta.len() == entry.size_bytes {
                    let sha = sha256_file_streamed(&final_path)?;
                    if sha.eq_ignore_ascii_case(&entry.sha256) {
                        completed_prior_files_bytes += entry.size_bytes;
                        on_progress(
                            completed_prior_files_bytes,
                            total_pack_bytes,
                            format!("Verified cached {}", entry.filename),
                        );
                        continue;
                    }
                    let _ = fs::remove_file(&final_path);
                }
            }

            let part_path = self.component_part_path(&spec.id, &entry.filename);
            let existing_part_len = fs::metadata(&part_path)
                .ok()
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .unwrap_or(0);

            // If `.part` already equals expected size from an earlier interrupted step, check its SHA-256 first
            if existing_part_len == entry.size_bytes && entry.size_bytes > 0 {
                if let Ok(sha) = sha256_file_streamed(&part_path) {
                    if sha.eq_ignore_ascii_case(&entry.sha256) {
                        atomic_install_file(&part_path, &final_path)?;
                        completed_prior_files_bytes += entry.size_bytes;
                        continue;
                    }
                }
                let _ = fs::remove_file(&part_path);
            } else if existing_part_len > entry.size_bytes {
                let _ = fs::remove_file(&part_path);
            }

            let resume_from = fs::metadata(&part_path)
                .ok()
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .unwrap_or(0);

            let current_file_bytes = std::cell::Cell::new(resume_from);
            on_progress(
                completed_prior_files_bytes + current_file_bytes.get(),
                total_pack_bytes,
                if resume_from > 0 {
                    format!(
                        "Resuming {} ({}/{})...",
                        entry.filename,
                        idx + 1,
                        files.len()
                    )
                } else {
                    format!(
                        "Downloading {} ({}/{})...",
                        entry.filename,
                        idx + 1,
                        files.len()
                    )
                },
            );

            let part_for_open = part_path.clone();
            let mut open_sink = |mode: ResumeMode| -> Result<Box<dyn Write>, AiModelError> {
                match mode {
                    ResumeMode::AppendedFromOffset => {
                        let f = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&part_for_open)
                            .map_err(|e| AiModelError::Io {
                                message: e.to_string(),
                            })?;
                        Ok(Box::new(f))
                    }
                    ResumeMode::RestartedFromZero => {
                        current_file_bytes.set(0);
                        let f = File::create(&part_for_open).map_err(|e| AiModelError::Io {
                            message: e.to_string(),
                        })?;
                        Ok(Box::new(f))
                    }
                }
            };

            let label_name = entry.filename.clone();
            let file_count = files.len();
            let prior = completed_prior_files_bytes;
            transport.fetch_range_stream(
                RangeRequest {
                    model_id: &spec.id,
                    url: &entry.url,
                    resume_from,
                    expected_total_size: entry.size_bytes,
                },
                &mut open_sink,
                cancel,
                &mut |delta| {
                    let next = current_file_bytes.get() + delta;
                    current_file_bytes.set(next);
                    on_progress(
                        (prior + next).min(total_pack_bytes),
                        total_pack_bytes,
                        format!("Downloading {label_name} ({}/{file_count})...", idx + 1),
                    );
                },
            )?;

            on_progress(
                (completed_prior_files_bytes + entry.size_bytes).min(total_pack_bytes),
                total_pack_bytes,
                format!("Verifying SHA-256 for {}...", entry.filename),
            );

            let actual_sha = sha256_file_streamed(&part_path)?;
            if !actual_sha.eq_ignore_ascii_case(&entry.sha256) {
                let _ = fs::remove_file(&part_path);
                return Err(AiModelError::ChecksumMismatch {
                    model_id: spec.id.clone(),
                    filename: entry.filename.clone(),
                    expected: entry.sha256.clone(),
                    actual: actual_sha,
                });
            }

            atomic_install_file(&part_path, &final_path)?;
            completed_prior_files_bytes += entry.size_bytes;
        }

        on_progress(
            total_pack_bytes,
            total_pack_bytes,
            "Model verified and ready".to_string(),
        );
        Ok(model_dir)
    }

    /// Manually imports an offline `.onnx` file (for single-file models) or a folder containing
    /// the required pack files (for multi-file models like OCR), verifying SHA-256 before atomic install.
    pub fn import_offline_model(
        &self,
        model_id: &str,
        source_path: &Path,
    ) -> Result<PathBuf, AiModelError> {
        let spec = self
            .find_spec(model_id)
            .ok_or_else(|| AiModelError::UnknownModel {
                model_id: model_id.to_string(),
            })?
            .clone();

        validate_permissive_model_license(&spec.id, &spec.license)?;

        if !source_path.exists() {
            return Err(AiModelError::Io {
                message: format!("Selected path does not exist: '{}'", source_path.display()),
            });
        }

        let files = spec.effective_files();
        let resolved_sources: Vec<(&ModelFileEntry, PathBuf)> = if files.len() == 1
            && source_path.is_file()
        {
            vec![(&files[0], source_path.to_path_buf())]
        } else if source_path.is_dir() {
            let mut pairs = Vec::with_capacity(files.len());
            for f in &files {
                let candidate = source_path.join(&f.filename);
                if !candidate.is_file() {
                    return Err(AiModelError::Io {
                        message: format!(
                            "Folder '{}' is missing required model file '{}'.",
                            source_path.display(),
                            f.filename
                        ),
                    });
                }
                pairs.push((f, candidate));
            }
            pairs
        } else {
            return Err(AiModelError::Io {
                message: format!(
                    "Model '{}' consists of {} files ({}). Please select a folder containing those files.",
                    spec.display_name,
                    files.len(),
                    files
                        .iter()
                        .map(|f| f.filename.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        };

        // Verify SHA-256 of every source file BEFORE copying/installing anything
        for (entry, src_file) in &resolved_sources {
            let actual_sha = sha256_file_streamed(src_file)?;
            if !actual_sha.eq_ignore_ascii_case(&entry.sha256) {
                return Err(AiModelError::ChecksumMismatch {
                    model_id: spec.id.clone(),
                    filename: entry.filename.clone(),
                    expected: entry.sha256.clone(),
                    actual: actual_sha,
                });
            }
        }

        let dest_dir = self.model_dir(&spec.id);
        fs::create_dir_all(&dest_dir).map_err(|e| AiModelError::Io {
            message: e.to_string(),
        })?;

        for (entry, src_file) in resolved_sources {
            let part = self.component_part_path(&spec.id, &entry.filename);
            let final_dest = self.component_path(&spec.id, &entry.filename);
            fs::copy(&src_file, &part).map_err(|e| AiModelError::Io {
                message: format!("Failed copying '{}': {e}", src_file.display()),
            })?;
            atomic_install_file(&part, &final_dest)?;
        }

        Ok(dest_dir)
    }

    /// Removes both installed model files and any `.part` temporary downloads for `model_id`.
    pub fn delete_model(&self, model_id: &str) -> Result<bool, AiModelError> {
        let dir = self.model_dir(model_id);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| AiModelError::Io {
                message: format!("Failed removing '{}': {e}", dir.display()),
            })?;
            return Ok(true);
        }
        // Backward compatibility with flat `<models_dir>/<safe_id>.onnx` from v1 skeleton
        let legacy = self
            .models_dir
            .join(format!("{}.onnx", model_id.replace(['/', '\\', ':'], "_")));
        if legacy.exists() {
            fs::remove_file(&legacy).map_err(|e| AiModelError::Io {
                message: e.to_string(),
            })?;
            return Ok(true);
        }
        Ok(false)
    }
}

fn atomic_install_file(part_path: &Path, final_path: &Path) -> Result<(), AiModelError> {
    if final_path.exists() {
        let _ = fs::remove_file(final_path);
    }
    fs::rename(part_path, final_path).map_err(|e| AiModelError::Io {
        message: format!(
            "Failed moving verified model into place ('{}' -> '{}'): {e}",
            part_path.display(),
            final_path.display()
        ),
    })
}

pub fn sha256_file_streamed(path: &Path) -> Result<String, AiModelError> {
    let mut file = File::open(path).map_err(|e| AiModelError::Io {
        message: format!("Cannot open '{}': {e}", path.display()),
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

// ============================================================================
// Live Thread-Safe AI Settings & Download State Coordinator (`AiRuntimeHub`)
// ============================================================================

#[derive(Clone)]
struct LiveDownloadEntry {
    state: ModelInstallState,
    downloaded_bytes: u64,
    total_bytes: u64,
    message: Option<String>,
    error: Option<String>,
    cancel: CancelHandle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettingsSnapshot {
    pub models_dir: String,
    pub is_custom_dir: bool,
    pub device_preference: String,
    pub available_providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelsCatalogPayload {
    pub schema_version: u32,
    pub settings: AiSettingsSnapshot,
    pub models: Vec<AiModelStatus>,
    pub flagged_excluded: Vec<crate::manifest::FlaggedNonCommercialModel>,
}

/// Coordinates active model directory, device preference, lazy ONNX sessions, concurrent downloads, and event broadcasts.
pub struct AiRuntimeHub {
    custom_models_dir: RwLock<Option<PathBuf>>,
    fallback_models_dir: RwLock<PathBuf>,
    device_pref: RwLock<AiDevicePreference>,
    manifest: ModelsManifest,
    sessions: crate::runtime::SessionPool,
    downloads: RwLock<HashMap<String, LiveDownloadEntry>>,
    events_tx: broadcast::Sender<String>,
    recent_errors: RwLock<VecDeque<(String, String)>>,
}

impl AiRuntimeHub {
    pub fn new(default_dir: PathBuf) -> Result<Self, AiModelError> {
        let manifest = ModelsManifest::load_from_path_or_embedded(None)?;
        let (events_tx, _) = broadcast::channel(256);
        Ok(Self {
            custom_models_dir: RwLock::new(None),
            fallback_models_dir: RwLock::new(default_dir),
            device_pref: RwLock::new(AiDevicePreference::Auto),
            manifest,
            sessions: crate::runtime::SessionPool::default(),
            downloads: RwLock::new(HashMap::new()),
            events_tx,
            recent_errors: RwLock::new(VecDeque::new()),
        })
    }

    pub fn sessions(&self) -> &crate::runtime::SessionPool {
        &self.sessions
    }

    pub fn evict_idle_sessions(&self) -> usize {
        self.sessions.evict_idle_older_than(Duration::from_secs(60))
    }

    pub fn update_default_dir_if_unset(&self, default_dir: &Path) {
        if default_dir.as_os_str().is_empty() {
            return;
        }
        if let Ok(mut fb) = self.fallback_models_dir.write() {
            *fb = default_dir.to_path_buf();
        }
    }

    pub fn effective_models_dir(&self) -> PathBuf {
        if let Ok(guard) = self.custom_models_dir.read() {
            if let Some(custom) = guard.as_ref() {
                return custom.clone();
            }
        }
        self.fallback_models_dir
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| std::env::temp_dir().join("tool_center_ai_models"))
    }

    pub fn set_custom_models_dir(&self, custom: Option<PathBuf>) -> Result<PathBuf, AiModelError> {
        if let Some(ref dir) = custom {
            fs::create_dir_all(dir).map_err(|e| AiModelError::Io {
                message: format!("Cannot access or create folder '{}': {e}", dir.display()),
            })?;
        }
        if let Ok(mut guard) = self.custom_models_dir.write() {
            *guard = custom;
        }
        self.sessions.clear_all();
        let eff = self.effective_models_dir();
        self.broadcast_catalog();
        Ok(eff)
    }

    pub fn device_preference(&self) -> AiDevicePreference {
        self.device_pref
            .read()
            .map(|g| *g)
            .unwrap_or(AiDevicePreference::Auto)
    }

    pub fn set_device_preference(&self, pref: AiDevicePreference) {
        if let Ok(mut guard) = self.device_pref.write() {
            *guard = pref;
        }
        self.sessions.clear_all();
        self.broadcast_catalog();
    }

    pub fn detected_execution_providers(&self) -> Vec<String> {
        crate::runtime::query_available_providers(false)
    }

    pub fn settings_snapshot(&self) -> AiSettingsSnapshot {
        let is_custom = self
            .custom_models_dir
            .read()
            .map(|g| g.is_some())
            .unwrap_or(false);
        AiSettingsSnapshot {
            models_dir: self.effective_models_dir().display().to_string(),
            is_custom_dir: is_custom,
            device_preference: self.device_preference().as_str().to_string(),
            available_providers: self.detected_execution_providers(),
        }
    }

    pub fn manager(&self) -> Result<ModelManager, AiModelError> {
        ModelManager::new(self.effective_models_dir(), self.manifest.clone())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.events_tx.subscribe()
    }

    pub fn catalog_payload(&self) -> Result<AiModelsCatalogPayload, AiModelError> {
        let mgr = self.manager()?;
        let mut statuses = mgr.list_models();

        if let Ok(dl_map) = self.downloads.read() {
            for item in &mut statuses {
                if let Some(live) = dl_map.get(&item.spec.id) {
                    if live.state == ModelInstallState::Downloading {
                        item.state = ModelInstallState::Downloading;
                        item.installed = false;
                        item.downloaded_bytes = live.downloaded_bytes;
                        item.progress = if live.total_bytes > 0 {
                            (live.downloaded_bytes as f32 / live.total_bytes as f32)
                                .clamp(0.0, 0.99)
                        } else {
                            0.0
                        };
                        item.status_message = live.message.clone();
                        item.error_message = None;
                    } else if live.state == ModelInstallState::Failed && !item.installed {
                        item.state = ModelInstallState::Failed;
                        item.error_message = live.error.clone();
                    }
                }
            }
        }

        Ok(AiModelsCatalogPayload {
            schema_version: self.manifest.schema_version,
            settings: self.settings_snapshot(),
            models: statuses,
            flagged_excluded: flagged_excluded_models(),
        })
    }

    pub fn broadcast_catalog(&self) {
        if let Ok(payload) = self.catalog_payload() {
            if let Ok(json) = serde_json::to_string(&payload) {
                let _ = self.events_tx.send(json);
            }
        }
    }

    /// Starts an asynchronous resumable download on `handle` if not already downloading.
    pub fn start_download(
        self: &Arc<Self>,
        handle: &tokio::runtime::Handle,
        model_id: String,
    ) -> Result<(), AiModelError> {
        let mgr = self.manager()?;
        let spec = mgr
            .find_spec(&model_id)
            .ok_or_else(|| AiModelError::UnknownModel {
                model_id: model_id.clone(),
            })?
            .clone();

        let cancel = CancelHandle::new();
        {
            let mut dl = self.downloads.write().map_err(|_| AiModelError::Io {
                message: "Download state lock poisoned".to_string(),
            })?;
            if let Some(existing) = dl.get(&model_id) {
                if existing.state == ModelInstallState::Downloading {
                    return Ok(());
                }
            }
            let initial_bytes = mgr.resumable_bytes_on_disk(&spec);
            dl.insert(
                model_id.clone(),
                LiveDownloadEntry {
                    state: ModelInstallState::Downloading,
                    downloaded_bytes: initial_bytes,
                    total_bytes: spec.size_bytes,
                    message: Some("Connecting...".to_string()),
                    error: None,
                    cancel: cancel.clone(),
                },
            );
        }
        self.broadcast_catalog();

        let hub = Arc::clone(self);
        handle.spawn(async move {
            let hub_progress = Arc::clone(&hub);
            let mid_progress = model_id.clone();
            let mid_run = model_id.clone();

            let res = tokio::task::spawn_blocking(move || {
                let mgr = hub_progress.manager()?;
                let transport = HttpModelTransport;
                let mut last_emit = std::time::Instant::now();
                mgr.download_model(&mid_run, &transport, &cancel, |done, total, step| {
                    if let Ok(mut dl) = hub_progress.downloads.write() {
                        if let Some(entry) = dl.get_mut(&mid_progress) {
                            entry.downloaded_bytes = done;
                            entry.total_bytes = total;
                            entry.message = Some(step);
                        }
                    }
                    if done >= total || last_emit.elapsed() >= Duration::from_millis(90) {
                        last_emit = std::time::Instant::now();
                        hub_progress.broadcast_catalog();
                    }
                })
            })
            .await;

            if let Ok(mut dl) = hub.downloads.write() {
                match res {
                    Ok(Ok(_)) => {
                        dl.remove(&model_id);
                    }
                    Ok(Err(AiModelError::Cancelled)) => {
                        dl.remove(&model_id);
                    }
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        if let Ok(mut errs) = hub.recent_errors.write() {
                            errs.push_back((model_id.clone(), msg.clone()));
                            while errs.len() > 16 {
                                errs.pop_front();
                            }
                        }
                        if let Some(entry) = dl.get_mut(&model_id) {
                            entry.state = ModelInstallState::Failed;
                            entry.error = Some(msg);
                        }
                    }
                    Err(join_err) => {
                        if let Some(entry) = dl.get_mut(&model_id) {
                            entry.state = ModelInstallState::Failed;
                            entry.error = Some(format!("Download worker panicked: {join_err}"));
                        }
                    }
                }
            }
            hub.broadcast_catalog();
        });

        Ok(())
    }

    pub fn cancel_download(&self, model_id: &str) -> bool {
        if let Ok(dl) = self.downloads.read() {
            if let Some(entry) = dl.get(model_id) {
                if entry.state == ModelInstallState::Downloading {
                    entry.cancel.cancel();
                    return true;
                }
            }
        }
        false
    }
}
