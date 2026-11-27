use crate::output::{resolve_safe_output_dir, resolve_safe_output_path};
use crate::schema::{ToolDescriptor, ToolInputSpec, ToolOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Structured user-friendly error returned by tools and the job engine.
#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum ToolError {
    #[error("{message}")]
    InvalidInput { message: String },

    #[error("{message}")]
    UnsupportedFormat { message: String },

    #[error("{message}")]
    CorruptFile {
        path: String,
        message: String,
        details: Option<String>,
    },

    #[error("Operation cancelled")]
    Cancelled,

    #[error("{message}")]
    ProcessingFailed {
        message: String,
        details: Option<String>,
    },

    #[error("Tool '{tool_id}' is not registered")]
    UnknownTool { tool_id: String },
}

impl ToolError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: msg.into(),
        }
    }

    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::UnsupportedFormat {
            message: msg.into(),
        }
    }

    pub fn corrupt(path: &Path, msg: impl Into<String>, details: Option<String>) -> Self {
        Self::CorruptFile {
            path: path.display().to_string(),
            message: msg.into(),
            details,
        }
    }

    pub fn failed(msg: impl Into<String>, details: Option<String>) -> Self {
        Self::ProcessingFailed {
            message: msg.into(),
            details,
        }
    }

    pub fn user_message(&self) -> String {
        self.to_string()
    }

    pub fn technical_details(&self) -> Option<String> {
        match self {
            Self::CorruptFile { details, .. } | Self::ProcessingFailed { details, .. } => {
                details.clone()
            }
            _ => None,
        }
    }
}

type CancelHook = Box<dyn Fn() + Send + Sync>;

/// Shared cancellation handle that supports cooperative polling and immediate cancel hooks
/// (e.g. terminating a spawned FFmpeg child process).
#[derive(Clone, Default)]
pub struct CancelHandle {
    cancelled: Arc<AtomicBool>,
    hooks: Arc<Mutex<Vec<CancelHook>>>,
}

impl CancelHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            if let Ok(hooks) = self.hooks.lock() {
                for hook in hooks.iter() {
                    hook();
                }
            }
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn check(&self) -> Result<(), ToolError> {
        if self.is_cancelled() {
            Err(ToolError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn on_cancel<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        if self.is_cancelled() {
            callback();
            return;
        }
        if let Ok(mut hooks) = self.hooks.lock() {
            hooks.push(Box::new(callback));
        }
    }
}

pub type ProgressCallback = Arc<dyn Fn(f32, String) + Send + Sync>;

/// Execution context passed to every `Tool::run(ctx)` invocation.
pub struct ToolContext {
    pub job_id: String,
    pub inputs: Vec<PathBuf>,
    pub options: ToolOptions,
    pub output_dir: Option<PathBuf>,
    pub cancel: CancelHandle,
    progress_cb: ProgressCallback,
}

impl ToolContext {
    pub fn new(
        job_id: impl Into<String>,
        inputs: Vec<PathBuf>,
        options: ToolOptions,
        output_dir: Option<PathBuf>,
        cancel: CancelHandle,
        progress_cb: ProgressCallback,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            inputs,
            options,
            output_dir,
            cancel,
            progress_cb,
        }
    }

    /// Reports clamped `[0.0, 1.0]` progress and a human-friendly status message.
    pub fn report_progress(&self, fraction: f32, step: impl Into<String>) {
        let clamped = fraction.clamp(0.0, 1.0);
        (self.progress_cb)(clamped, step.into());
    }

    /// Checks whether cancellation was requested, returning `Err(ToolError::Cancelled)` if so.
    pub fn check_cancelled(&self) -> Result<(), ToolError> {
        self.cancel.check()
    }

    /// Computes a collision-free output file path next to `source` or in `self.output_dir`.
    pub fn output_path_for(
        &self,
        source: &Path,
        suffix: &str,
        new_extension: Option<&str>,
    ) -> PathBuf {
        resolve_safe_output_path(source, self.output_dir.as_deref(), suffix, new_extension)
    }

    /// Computes a collision-free output folder path next to `source` or in `self.output_dir`.
    pub fn output_dir_for(&self, source: &Path, suffix: &str) -> PathBuf {
        resolve_safe_output_dir(source, self.output_dir.as_deref(), suffix)
    }
}

/// Metadata describing one produced output file or folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputArtifact {
    pub path: String,
    pub label: String,
    pub size_bytes: u64,
    pub original_size_bytes: Option<u64>,
}

/// Result returned when a `Tool` finishes successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRunResult {
    pub summary: String,
    pub outputs: Vec<OutputArtifact>,
    pub warnings: Vec<String>,
    /// Optional structured result payload (e.g. computed file hashes, duplicate groups, archive listing).
    pub report_json: Option<Value>,
}

impl ToolRunResult {
    pub fn with_outputs(summary: impl Into<String>, outputs: Vec<OutputArtifact>) -> Self {
        Self {
            summary: summary.into(),
            outputs,
            warnings: Vec::new(),
            report_json: None,
        }
    }
}

/// Standard input validation helper shared across all `Tool` implementations.
pub fn validate_inputs_against_spec(
    inputs: &[PathBuf],
    spec: &ToolInputSpec,
) -> Result<(), ToolError> {
    if inputs.len() < spec.min_files {
        return Err(ToolError::invalid(format!(
            "Please select at least {} file{}.",
            spec.min_files,
            if spec.min_files == 1 { "" } else { "s" }
        )));
    }
    if let Some(max) = spec.max_files {
        if inputs.len() > max {
            return Err(ToolError::invalid(format!(
                "This tool accepts at most {max} file{}.",
                if max == 1 { "" } else { "s" }
            )));
        }
    }
    for path in inputs {
        let meta = std::fs::metadata(path).map_err(|_| {
            ToolError::invalid(format!(
                "File not found or unreadable: '{}'.",
                path.display()
            ))
        })?;
        if meta.is_dir() {
            if !spec.allow_directories {
                return Err(ToolError::invalid(format!(
                    "'{}' is a folder, but this tool expects files.",
                    path.display()
                )));
            }
            continue;
        }
        if !spec.accepted_extensions.is_empty() {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if !spec
                .accepted_extensions
                .iter()
                .any(|allowed| allowed == &ext)
            {
                return Err(ToolError::unsupported(format!(
                    "Unsupported file type '.{}' for '{}'. Supported formats: {}.",
                    if ext.is_empty() { "none" } else { &ext },
                    path.file_name().and_then(|s| s.to_str()).unwrap_or("file"),
                    spec.accepted_extensions.join(", ")
                )));
            }
        }
    }
    Ok(())
}

/// Every tool in Tool Center implements this single trait.
/// Adding a new tool requires only implementing `Tool` and registering it in the `ToolRegistry`.
pub trait Tool: Send + Sync + 'static {
    /// Returns the static descriptor and UI option schema for this tool.
    fn descriptor(&self) -> ToolDescriptor;

    /// Validates inputs and options before scheduling or running the job.
    fn validate(&self, inputs: &[PathBuf], options: &ToolOptions) -> Result<(), ToolError> {
        let desc = self.descriptor();
        validate_inputs_against_spec(inputs, &desc.input_spec)?;
        let _ = options;
        Ok(())
    }

    /// Runs the tool on a background worker thread with progress reporting and cancellation.
    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError>;
}
