use crate::manifest::AiDevicePreference;
use crate::{OnnxInferenceSession, OnnxRuntimeEngine};
use core_jobs::{CancelHandle, ToolError};
use ort::environment::Environment;
use ort::ep::{self, ExecutionProvider};
use ort::session::{RunOptions, Session};
use ort::value::TensorRef;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static ORT_ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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

/// Locates the bundled ONNX Runtime dynamic library (`libonnxruntime.so` / `libonnxruntime.dylib` / `onnxruntime.dll`).
pub fn locate_onnxruntime_dylib() -> Result<PathBuf, ToolError> {
    if let Ok(explicit) = env::var("ORT_DYLIB_PATH") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
    }

    let lib_filename = if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };

    let vendor_subdir = if cfg!(target_os = "windows") {
        "bin"
    } else {
        "lib"
    };

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("lib").join(lib_filename));
            candidates.push(exe_dir.join(lib_filename));
            candidates.push(exe_dir.join("..").join("Frameworks").join(lib_filename));
        }
    }

    let target = host_vendor_target();
    if let Ok(cwd) = env::current_dir() {
        for ancestor in cwd.ancestors().take(5) {
            candidates.push(
                ancestor
                    .join("vendor")
                    .join(target)
                    .join("onnxruntime")
                    .join(vendor_subdir)
                    .join(lib_filename),
            );
            candidates.push(
                ancestor
                    .join("vendor")
                    .join("macos-universal")
                    .join("onnxruntime")
                    .join("lib")
                    .join(lib_filename),
            );
        }
    }

    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(ToolError::failed(
        "ONNX Runtime library was not found. Run `python scripts/fetch_vendor.py --component onnxruntime` or reinstall Tool Center.",
        None,
    ))
}

/// Lazily initializes the process-wide ONNX Runtime `Environment` from the bundled dynamic library
/// with telemetry explicitly disabled. Never runs at app startup — only when an AI job begins.
pub fn ensure_ort_environment() -> Result<&'static Environment, ToolError> {
    let res = ORT_ENVIRONMENT.get_or_init(|| {
        let dylib_path = locate_onnxruntime_dylib().map_err(|e| e.user_message())?;

        // On Windows, prepend the ONNX Runtime directory to PATH so companion DLLs (e.g. DirectML.dll) resolve cleanly
        #[cfg(target_os = "windows")]
        if let Some(parent) = dylib_path.parent() {
            let existing = env::var_os("PATH").unwrap_or_default();
            let mut paths = vec![parent.to_path_buf()];
            paths.extend(env::split_paths(&existing));
            if let Ok(joined) = env::join_paths(paths) {
                env::set_var("PATH", joined);
            }
        }

        let builder = ort::init_from(&dylib_path).map_err(|e| {
            format!(
                "Failed loading ONNX Runtime library '{}': {e}",
                dylib_path.display()
            )
        })?;
        builder
            .with_name("tool-center")
            .with_telemetry(false)
            .build()
            .map_err(|e| format!("Failed initializing ONNX Runtime environment: {e}"))
    });

    match res {
        Ok(env) => Ok(env),
        Err(msg) => Err(ToolError::failed(msg.clone(), None)),
    }
}

/// Probes which Execution Providers are compiled into the active ONNX Runtime dynamic library.
/// If ONNX Runtime has not been loaded yet, returns static platform defaults without loading the dylib
/// so startup and idle memory remain completely unaffected.
pub fn query_available_providers(load_if_needed: bool) -> Vec<String> {
    if !load_if_needed && ORT_ENVIRONMENT.get().is_none() {
        let mut defaults = vec!["cpu".to_string()];
        if cfg!(target_os = "macos") {
            defaults.push("coreml".to_string());
        } else if cfg!(target_os = "windows") {
            defaults.push("directml".to_string());
            defaults.push("cuda".to_string());
        } else if cfg!(target_os = "linux") {
            defaults.push("cuda".to_string());
        }
        return defaults;
    }

    if ensure_ort_environment().is_err() {
        return vec!["cpu".to_string()];
    }

    let mut providers = vec!["cpu".to_string()];
    if ep::CoreML::is_available().unwrap_or(false) {
        providers.push("coreml".to_string());
    }
    if ep::DirectML::is_available().unwrap_or(false) {
        providers.push("directml".to_string());
    }
    if ep::CUDA::is_available().unwrap_or(false) {
        providers.push("cuda".to_string());
    }
    providers
}

/// Active execution provider selected for a loaded session (after any fallback to CPU).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProviderInfo {
    pub requested: AiDevicePreference,
    pub active_provider: String,
    pub fell_back_to_cpu: bool,
    pub fallback_reason: Option<String>,
}

/// Thread-safe handle to a loaded ONNX Runtime `Session` supporting mid-inference cancellation.
pub struct OrtSessionHandle {
    model_id: String,
    session: Mutex<Session>,
    provider_info: ActiveProviderInfo,
    last_used_ms: AtomicU64,
}

impl OrtSessionHandle {
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn provider_info(&self) -> &ActiveProviderInfo {
        &self.provider_info
    }

    pub fn touch(&self) {
        self.last_used_ms.store(now_epoch_ms(), Ordering::Relaxed);
    }

    pub fn idle_duration(&self) -> Duration {
        let last = self.last_used_ms.load(Ordering::Relaxed);
        let now = now_epoch_ms();
        Duration::from_millis(now.saturating_sub(last))
    }

    /// Runs a single `[N, C, H, W]` `f32` input tensor through the ONNX graph with live cancellation,
    /// returning `(output_shape, output_f32_vec)` from the primary output tensor.
    pub fn run_f32_tensor(
        &self,
        input_dims: &[usize],
        input_data: &[f32],
        cancel: &CancelHandle,
    ) -> Result<(Vec<usize>, Vec<f32>), ToolError> {
        cancel.check()?;
        self.touch();

        let expected_numel: usize = input_dims.iter().product();
        if expected_numel != input_data.len() {
            return Err(ToolError::invalid(format!(
                "Tensor shape {:?} expects {} elements, got {}",
                input_dims,
                expected_numel,
                input_data.len()
            )));
        }

        let shape_i64: Vec<i64> = input_dims.iter().map(|&d| d as i64).collect();
        let tensor_ref = TensorRef::from_array_view((shape_i64, input_data)).map_err(|e| {
            ToolError::failed(
                "Failed preparing input tensor for ONNX Runtime.",
                Some(e.to_string()),
            )
        })?;

        let run_opts = Arc::new(RunOptions::new().map_err(|e| {
            ToolError::failed("Failed creating ONNX RunOptions", Some(e.to_string()))
        })?);

        let opts_for_cancel = Arc::clone(&run_opts);
        cancel.on_cancel(move || {
            let _ = opts_for_cancel.terminate();
        });

        let mut guard = self
            .session
            .lock()
            .map_err(|_| ToolError::failed("ONNX session mutex poisoned", None))?;

        let outputs = match guard.run_with_options(ort::inputs![tensor_ref], &run_opts) {
            Ok(out) => out,
            Err(err) => {
                if cancel.is_cancelled() {
                    return Err(ToolError::Cancelled);
                }
                let raw = err.to_string();
                let lower = raw.to_ascii_lowercase();
                if lower.contains("terminate") && cancel.is_cancelled() {
                    return Err(ToolError::Cancelled);
                }
                if lower.contains("bad_alloc")
                    || lower.contains("out of memory")
                    || lower.contains("not enough memory")
                    || lower.contains("failed to allocate")
                {
                    return Err(ToolError::failed(
                        "Not enough memory to finish AI inference on this image. Try a smaller tile size or close other memory-heavy apps.",
                        Some(raw),
                    ));
                }
                return Err(ToolError::failed(
                    format!("AI model '{}' failed during inference.", self.model_id),
                    Some(raw),
                ));
            }
        };

        cancel.check()?;
        if outputs.len() == 0 {
            return Err(ToolError::failed(
                format!("ONNX model '{}' produced zero outputs.", self.model_id),
                None,
            ));
        }

        let (out_shape, out_slice) = outputs[0].try_extract_tensor::<f32>().map_err(|e| {
            ToolError::failed(
                format!(
                    "Failed extracting float32 output tensor from '{}'.",
                    self.model_id
                ),
                Some(e.to_string()),
            )
        })?;

        let dims_usize: Vec<usize> = out_shape.iter().map(|&d| d.max(0) as usize).collect();
        let data_vec = out_slice.to_vec();
        self.touch();
        Ok((dims_usize, data_vec))
    }
}

impl OnnxInferenceSession for Arc<OrtSessionHandle> {
    fn model_id(&self) -> &str {
        OrtSessionHandle::model_id(self)
    }

    fn run_nchw_f32(
        &self,
        input_shape: [usize; 4],
        input_tensor: &[f32],
        cancel: &CancelHandle,
    ) -> Result<(Vec<usize>, Vec<f32>), ToolError> {
        self.run_f32_tensor(&input_shape, input_tensor, cancel)
    }
}

fn try_commit_session(
    env: &Environment,
    onnx_path: &Path,
    preference: AiDevicePreference,
) -> Result<(Session, ActiveProviderInfo), ToolError> {
    let intra_threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 4))
        .unwrap_or(2);

    // Helper closure to build a clean CPU-only session (guaranteed baseline)
    let build_cpu_session = |fallback_reason: Option<String>, fell_back: bool| {
        let mut builder = Session::builder(env).map_err(|e| {
            ToolError::failed("Failed creating ONNX session builder", Some(e.to_string()))
        })?;
        builder = builder.with_intra_threads(intra_threads).map_err(|e| {
            ToolError::failed("Failed setting ONNX thread count", Some(e.to_string()))
        })?;
        let sess = builder.commit_from_file(onnx_path).map_err(|e| {
            ToolError::corrupt(
                onnx_path,
                "Could not load ONNX model file — it may be corrupted or incompatible.",
                Some(e.to_string()),
            )
        })?;
        Ok((
            sess,
            ActiveProviderInfo {
                requested: preference,
                active_provider: "cpu".to_string(),
                fell_back_to_cpu: fell_back,
                fallback_reason,
            },
        ))
    };

    if preference == AiDevicePreference::Cpu {
        return build_cpu_session(None, false);
    }

    // Determine candidate hardware execution providers in priority order
    let mut candidate_eps = Vec::new();
    let mut primary_label = "cpu";

    match preference {
        AiDevicePreference::CoreMl => {
            if ep::CoreML::is_available().unwrap_or(false) {
                candidate_eps.push(ep::CoreML::default().build());
                primary_label = "coreml";
            } else {
                return build_cpu_session(
                    Some("CoreML is not available on this system; using CPU.".to_string()),
                    true,
                );
            }
        }
        AiDevicePreference::DirectMl => {
            if ep::DirectML::is_available().unwrap_or(false) {
                candidate_eps.push(ep::DirectML::default().build());
                primary_label = "directml";
            } else {
                return build_cpu_session(
                    Some("DirectML is not available on this system; using CPU.".to_string()),
                    true,
                );
            }
        }
        AiDevicePreference::Cuda => {
            if ep::CUDA::is_available().unwrap_or(false) {
                candidate_eps.push(ep::CUDA::default().build());
                primary_label = "cuda";
            } else {
                return build_cpu_session(
                    Some("CUDA runtime is not available on this system; using CPU.".to_string()),
                    true,
                );
            }
        }
        AiDevicePreference::Auto => {
            if cfg!(target_os = "macos") && ep::CoreML::is_available().unwrap_or(false) {
                candidate_eps.push(ep::CoreML::default().build());
                primary_label = "coreml";
            } else if cfg!(target_os = "windows") && ep::DirectML::is_available().unwrap_or(false) {
                candidate_eps.push(ep::DirectML::default().build());
                primary_label = "directml";
            } else if ep::CUDA::is_available().unwrap_or(false) {
                candidate_eps.push(ep::CUDA::default().build());
                primary_label = "cuda";
            }
        }
        AiDevicePreference::Cpu => unreachable!(),
    }

    if candidate_eps.is_empty() {
        return build_cpu_session(None, false);
    }
    candidate_eps.push(ep::CPU::default().build());

    // Attempt hardware-accelerated session creation; automatically fall back to CPU on any EP error!
    let accel_attempt = (|| -> Result<Session, String> {
        let mut builder = Session::builder(env)
            .map_err(|e| e.to_string())?
            .with_intra_threads(intra_threads)
            .map_err(|e| e.to_string())?
            .with_execution_providers(candidate_eps)
            .map_err(|e| e.to_string())?;
        builder
            .commit_from_file(onnx_path)
            .map_err(|e| e.to_string())
    })();

    match accel_attempt {
        Ok(sess) => Ok((
            sess,
            ActiveProviderInfo {
                requested: preference,
                active_provider: primary_label.to_string(),
                fell_back_to_cpu: false,
                fallback_reason: None,
            },
        )),
        Err(accel_err) => build_cpu_session(
            Some(format!(
                "Hardware accelerator ({primary_label}) unavailable ({accel_err}); fell back to CPU."
            )),
            true,
        ),
    }
}

/// Lazy, idle-evicting ONNX Runtime session pool.
///
/// Sessions are loaded on demand when a job executes and automatically evicted after remaining
/// idle past `idle_ttl` (default 60s) so Tool Center's idle RAM footprint stays minimal.
pub struct SessionPool {
    sessions: RwLock<HashMap<(PathBuf, String), Arc<OrtSessionHandle>>>,
    idle_ttl: Duration,
}

impl Default for SessionPool {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

impl SessionPool {
    pub fn new(idle_ttl: Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            idle_ttl,
        }
    }

    pub fn cached_count(&self) -> usize {
        self.sessions.read().map(|m| m.len()).unwrap_or(0)
    }

    /// Evicts any cached sessions not currently held by a running job (`strong_count == 1`)
    /// whose idle time exceeds `max_idle`.
    pub fn evict_idle_older_than(&self, max_idle: Duration) -> usize {
        let Ok(mut map) = self.sessions.write() else {
            return 0;
        };
        let before = map.len();
        map.retain(|_, handle| Arc::strong_count(handle) > 1 || handle.idle_duration() < max_idle);
        before.saturating_sub(map.len())
    }

    /// Clears all cached sessions immediately (e.g. when changing device preference or model folder).
    pub fn clear_all(&self) {
        if let Ok(mut map) = self.sessions.write() {
            map.clear();
        }
    }

    /// Retrieves an existing warm session or lazily loads it from `onnx_path`.
    pub fn get_or_load(
        &self,
        model_id: &str,
        onnx_path: &Path,
        preference: AiDevicePreference,
    ) -> Result<Arc<OrtSessionHandle>, ToolError> {
        self.evict_idle_older_than(self.idle_ttl);

        let key = (onnx_path.to_path_buf(), preference.as_str().to_string());
        if let Ok(map) = self.sessions.read() {
            if let Some(existing) = map.get(&key) {
                existing.touch();
                return Ok(Arc::clone(existing));
            }
        }

        let env = ensure_ort_environment()?;
        let (session, provider_info) = try_commit_session(env, onnx_path, preference)?;
        let handle = Arc::new(OrtSessionHandle {
            model_id: model_id.to_string(),
            session: Mutex::new(session),
            provider_info,
            last_used_ms: AtomicU64::new(now_epoch_ms()),
        });

        if let Ok(mut map) = self.sessions.write() {
            map.insert(key, Arc::clone(&handle));
        }
        Ok(handle)
    }
}

impl OnnxRuntimeEngine for SessionPool {
    fn load_session(
        &self,
        model_id: &str,
        onnx_path: &Path,
    ) -> Result<Box<dyn OnnxInferenceSession>, ToolError> {
        let handle = self.get_or_load(model_id, onnx_path, AiDevicePreference::Auto)?;
        Ok(Box::new(handle))
    }
}
