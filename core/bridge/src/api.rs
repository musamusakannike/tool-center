use crate::frb_generated::StreamSink;
use crate::preview::{inspect_file_full, read_bounded_text_preview};
use crate::state::engine;
use core_ai::{flagged_excluded_models, ModelManager};
use core_image::render_image_preview_png;
use core_jobs::{JobRequest, ToolOptions};
use core_pdf::render_pdf_page_to_png;
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

/// Inspects any local file for preview classification and rich metadata without blocking Flutter UI.
pub async fn inspect_file_json(path: String) -> Result<String, String> {
    let p = PathBuf::from(path);
    let payload = tokio::task::spawn_blocking(move || inspect_file_full(&p))
        .await
        .map_err(|e| format!("Inspector worker error: {e}"))??;
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

/// Reads a bounded UTF-8 text / code / Markdown / CSV / JSON preview without loading huge files into RAM.
pub async fn read_text_preview_json(path: String, max_bytes: u32) -> Result<String, String> {
    let p = PathBuf::from(path);
    let payload = tokio::task::spawn_blocking(move || read_bounded_text_preview(&p, max_bytes))
        .await
        .map_err(|e| format!("Text reader worker error: {e}"))??;
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

/// Decodes HEIC, TIFF, or AVIF images on a background thread into PNG bytes for UI display.
pub async fn render_decoded_image_preview(
    path: String,
    max_dimension: u32,
) -> Result<Vec<u8>, String> {
    let p = PathBuf::from(path);
    tokio::task::spawn_blocking(move || {
        render_image_preview_png(&p, max_dimension).map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| format!("Image preview worker error: {e}"))?
}

/// Renders a single 0-indexed PDF page via `pdfium-render` into PNG bytes + page count JSON header.
pub async fn render_pdf_page_preview_json(
    path: String,
    page_index: u32,
    target_width: u32,
    password: Option<String>,
) -> Result<Vec<u8>, String> {
    let p = PathBuf::from(path);
    let rendered = tokio::task::spawn_blocking(move || {
        render_pdf_page_to_png(&p, page_index, target_width, password.as_deref())
            .map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| format!("PDF render worker error: {e}"))??;
    Ok(rendered.png_bytes)
}

/// Returns total page count of a PDF (supporting optional user password for locked PDFs).
pub async fn get_pdf_page_count(path: String, password: Option<String>) -> Result<u32, String> {
    let p = PathBuf::from(path);
    tokio::task::spawn_blocking(move || {
        let rendered = render_pdf_page_to_png(&p, 0, 240, password.as_deref())
            .map_err(|e| e.user_message())?;
        Ok(rendered.page_count)
    })
    .await
    .map_err(|e| format!("PDF worker error: {e}"))?
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
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_core_version_and_full_tool_registry_schemas() {
        assert_eq!(core_version(), "0.1.0");
        let tools_raw = list_tools_json();
        let tools: Vec<serde_json::Value> = serde_json::from_str(&tools_raw).unwrap();
        assert_eq!(tools.len(), 38, "Expected all 38 v1 tools to be registered");

        for cat in ["images", "video", "audio", "pdf", "files", "convert"] {
            assert!(
                tools.iter().any(|t| t["category"] == cat),
                "Missing tools in category {cat}"
            );
        }

        // Every option across every tool must carry a non-empty plain-language label and hint
        for tool in &tools {
            let id = tool["id"].as_str().unwrap_or("");
            for opt in tool["options_schema"].as_array().unwrap() {
                assert!(
                    !opt["label"].as_str().unwrap_or("").trim().is_empty(),
                    "Tool {id} has an option with an empty label"
                );
                assert!(
                    !opt["hint"].as_str().unwrap_or("").trim().is_empty(),
                    "Tool {id} has an option with an empty hint"
                );
            }
        }
        assert!(list_jobs_json().starts_with('['));
    }

    #[tokio::test]
    async fn test_preview_inspection_and_bounded_streaming_reader() {
        let dir = tempdir().unwrap();

        // DOCX Office fallback classification
        let docx = dir.path().join("report.docx");
        fs::write(&docx, b"PK\x03\x04dummy").unwrap();
        let docx_json = inspect_file_json(docx.display().to_string()).await.unwrap();
        assert!(docx_json.contains("office_coming_soon"));

        // Bounded CSV streaming preview (even when truncated)
        let csv_path = dir.path().join("table.csv");
        let mut csv_data = String::from("name,role,score\nAlice,Engineer,98\nBob,Designer,95\n");
        for i in 0..500 {
            csv_data.push_str(&format!("User{i},Member,{i}\n"));
        }
        fs::write(&csv_path, csv_data.as_bytes()).unwrap();
        let preview_json = read_text_preview_json(csv_path.display().to_string(), 4096)
            .await
            .unwrap();
        assert!(preview_json.contains("\"truncated\":true"));
        assert!(preview_json.contains("\"csv_headers\":[\"name\",\"role\",\"score\"]"));
    }

    #[test]
    fn test_ai_models_endpoint() {
        let dir = std::env::temp_dir().join("tool_center_bridge_ai_test");
        let json = list_ai_models_json(dir.display().to_string()).unwrap();
        assert!(json.contains("Apache-2.0"));
        assert!(json.contains("flagged_excluded"));
    }
}
