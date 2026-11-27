use crate::frb_generated::StreamSink;
use crate::state::engine;
use core_ai::{flagged_excluded_models, ModelManager};
use core_jobs::{JobRequest, ToolOptions};
use flutter_rust_bridge::frb;
use serde_json::json;
use std::path::PathBuf;

/// Returns the Tool Center Rust core version string.
#[frb(sync)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Returns all registered tools and their declarative option schemas as a JSON array.
#[frb(sync)]
pub fn list_tools_json() -> String {
    let descriptors = engine().jobs.registry().descriptors();
    serde_json::to_string(&descriptors).unwrap_or_else(|_| "[]".to_string())
}

/// Validates and submits a background tool execution job, returning the new `job_id`.
pub fn submit_tool_job(
    tool_id: String,
    inputs: Vec<String>,
    options_json: String,
    output_dir: Option<String>,
) -> Result<String, String> {
    let options = ToolOptions::from_json_str(&options_json)
        .map_err(|e| format!("Invalid tool options JSON: {e}"))?;
    let input_paths: Vec<PathBuf> = inputs.into_iter().map(PathBuf::from).collect();
    let out_path = output_dir
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);

    let state = engine();
    state
        .jobs
        .submit(
            state.runtime.handle(),
            JobRequest {
                tool_id,
                inputs: input_paths,
                options,
                output_dir: out_path,
            },
        )
        .map_err(|e| e.user_message())
}

/// Streams real-time `JobSnapshot` JSON updates to Flutter without polling or blocking the UI.
pub fn subscribe_job_events(sink: StreamSink<String>) -> Result<(), String> {
    let state = engine();
    let mut rx = state.jobs.subscribe();
    state.runtime.spawn(async move {
        loop {
            match rx.recv().await {
                Ok(evt) => {
                    if let Ok(payload) = serde_json::to_string(&evt.snapshot) {
                        if sink.add(payload).is_err() {
                            break;
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    Ok(())
}

/// Returns all tracked jobs in newest-first order as a JSON array.
#[frb(sync)]
pub fn list_jobs_json() -> String {
    let jobs = engine().jobs.list_jobs();
    serde_json::to_string(&jobs).unwrap_or_else(|_| "[]".to_string())
}

/// Returns a single job snapshot by ID as JSON, if found.
#[frb(sync)]
pub fn get_job_json(job_id: String) -> Option<String> {
    engine()
        .jobs
        .get_job(&job_id)
        .and_then(|snap| serde_json::to_string(&snap).ok())
}

/// Cancels a queued or running job immediately (including any spawned sidecar processes).
#[frb(sync)]
pub fn cancel_job(job_id: String) -> bool {
    engine().jobs.cancel(&job_id)
}

/// Clears finished, failed, and cancelled jobs from the queue history.
#[frb(sync)]
pub fn clear_finished_jobs() {
    engine().jobs.clear_finished();
}

/// Returns the v2 permissive-licensed ONNX model catalog, cache status, and excluded non-commercial models.
pub fn list_ai_models_json(cache_dir: String) -> Result<String, String> {
    let mgr = ModelManager::with_default_catalog(cache_dir).map_err(|e| e.to_string())?;
    let payload = json!({
        "models": mgr.list_models(),
        "flagged_excluded": flagged_excluded_models(),
    });
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

/// Deletes a cached on-demand ONNX model file from disk.
pub fn delete_ai_model(cache_dir: String, model_id: String) -> Result<bool, String> {
    let mgr = ModelManager::with_default_catalog(cache_dir).map_err(|e| e.to_string())?;
    mgr.delete_cached_model(&model_id)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_version_and_empty_jobs() {
        assert_eq!(core_version(), "0.1.0");
        assert!(list_tools_json().starts_with('['));
        assert!(list_jobs_json().starts_with('['));
    }

    #[test]
    fn test_ai_models_endpoint() {
        let dir = std::env::temp_dir().join("tool_center_bridge_ai_test");
        let json = list_ai_models_json(dir.display().to_string()).unwrap();
        assert!(json.contains("Apache-2.0"));
        assert!(json.contains("flagged_excluded"));
    }
}
