//! Core job queue, common `Tool` trait, declarative option schemas, and non-destructive output helpers.

pub mod output;
pub mod queue;
pub mod schema;
pub mod tool;

pub use output::{resolve_safe_output_dir, resolve_safe_output_path};
pub use queue::{JobEvent, JobManager, JobRequest, JobSnapshot, JobStatus, ToolRegistry};
pub use schema::{
    OptionField, OptionKind, SelectChoice, ToolCategory, ToolDescriptor, ToolInputSpec,
    ToolOptions, VisibilityCondition,
};
pub use tool::{
    validate_inputs_against_spec, CancelHandle, OutputArtifact, ProgressCallback, Tool,
    ToolContext, ToolError, ToolRunResult,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    struct MockCopyTool;

    impl Tool for MockCopyTool {
        fn descriptor(&self) -> ToolDescriptor {
            ToolDescriptor {
                id: "test.copy".to_string(),
                name: "Test Copy".to_string(),
                name_key: "tool_test_copy_name".to_string(),
                description: "Copies text files safely with progress.".to_string(),
                description_key: "tool_test_copy_desc".to_string(),
                category: ToolCategory::Files,
                icon: "copy".to_string(),
                keywords: vec!["copy".to_string()],
                input_spec: ToolInputSpec::single_or_batch(&["txt"]),
                options_schema: vec![OptionField {
                    key: "delay_ms".to_string(),
                    label: "Step delay".to_string(),
                    label_key: "opt_delay_ms".to_string(),
                    hint: "Artificial delay per step for testing.".to_string(),
                    hint_key: "opt_delay_ms_hint".to_string(),
                    kind: OptionKind::Integer {
                        min: Some(0),
                        max: Some(2000),
                        step: Some(10),
                        unit: Some("ms".to_string()),
                    },
                    default_value: json!(10),
                    required: false,
                    visible_when: None,
                }],
                default_suffix: "_copied".to_string(),
            }
        }

        fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
            let delay_ms = ctx.options.get_i64("delay_ms", 10).max(0) as u64;
            let mut outputs = Vec::new();
            for (idx, input) in ctx.inputs.iter().enumerate() {
                ctx.check_cancelled()?;
                ctx.report_progress(
                    idx as f32 / ctx.inputs.len() as f32,
                    format!("Copying file {}", idx + 1),
                );
                if delay_ms > 0 {
                    for _ in 0..5 {
                        ctx.check_cancelled()?;
                        thread::sleep(Duration::from_millis(delay_ms / 5 + 1));
                    }
                }
                ctx.check_cancelled()?;
                let out_path = ctx.output_path_for(input, "_copied", None);
                fs::copy(input, &out_path).map_err(|e| {
                    ToolError::failed("Failed to write output file", Some(e.to_string()))
                })?;
                let size = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
                outputs.push(OutputArtifact {
                    path: out_path.display().to_string(),
                    label: out_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("output")
                        .to_string(),
                    size_bytes: size,
                    original_size_bytes: Some(size),
                });
            }
            ctx.report_progress(1.0, "Finished".to_string());
            Ok(ToolRunResult::with_outputs("Copied files", outputs))
        }
    }

    #[test]
    fn non_destructive_output_never_overwrites_original_or_existing() {
        let dir = tempdir().unwrap();
        let original = dir.path().join("photo.jpg");
        fs::write(&original, b"original-bytes").unwrap();

        // Empty suffix with same extension must NOT return `photo.jpg`
        let safe_same = resolve_safe_output_path(&original, None, "", None);
        assert_ne!(safe_same, original);
        assert_eq!(safe_same, dir.path().join("photo_out.jpg"));

        // Standard suffix creates `photo_resized.jpg`, and if that exists, `photo_resized_2.jpg`
        let first = resolve_safe_output_path(&original, None, "_resized", None);
        assert_eq!(first, dir.path().join("photo_resized.jpg"));
        fs::write(&first, b"first").unwrap();

        let second = resolve_safe_output_path(&original, None, "_resized", None);
        assert_eq!(second, dir.path().join("photo_resized_2.jpg"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn job_manager_executes_concurrently_with_progress_and_results() {
        let dir = tempdir().unwrap();
        let f1 = dir.path().join("alpha.txt");
        let f2 = dir.path().join("beta.txt");
        fs::write(&f1, b"hello").unwrap();
        fs::write(&f2, b"world").unwrap();

        let registry = ToolRegistry::new();
        registry.register(MockCopyTool);
        let manager = JobManager::new(registry, 2);
        let mut rx = manager.subscribe();

        let handle = tokio::runtime::Handle::current();
        let job1 = manager
            .submit(
                &handle,
                JobRequest {
                    tool_id: "test.copy".to_string(),
                    inputs: vec![f1.clone()],
                    options: ToolOptions::new(),
                    output_dir: None,
                },
            )
            .unwrap();
        let job2 = manager
            .submit(
                &handle,
                JobRequest {
                    tool_id: "test.copy".to_string(),
                    inputs: vec![f2.clone()],
                    options: ToolOptions::new(),
                    output_dir: None,
                },
            )
            .unwrap();

        let mut completed = 0;
        while completed < 2 {
            let evt = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .unwrap()
                .unwrap();
            if evt.snapshot.status == JobStatus::Completed
                && (evt.snapshot.job_id == job1 || evt.snapshot.job_id == job2)
            {
                completed += 1;
            }
        }

        let s1 = manager.get_job(&job1).unwrap();
        assert_eq!(s1.status, JobStatus::Completed);
        assert_eq!(s1.outputs.len(), 1);
        assert!(PathBuf::from(&s1.outputs[0].path).exists());
        assert_eq!(fs::read(&f1).unwrap(), b"hello");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn job_manager_supports_mid_flight_cancellation() {
        let dir = tempdir().unwrap();
        let f1 = dir.path().join("slow.txt");
        fs::write(&f1, b"cancel-me").unwrap();

        let registry = ToolRegistry::new();
        registry.register(MockCopyTool);
        let manager = JobManager::new(registry, 2);
        let mut rx = manager.subscribe();

        let mut opts = ToolOptions::new();
        opts.insert("delay_ms", json!(800));

        let handle = tokio::runtime::Handle::current();
        let job_id = manager
            .submit(
                &handle,
                JobRequest {
                    tool_id: "test.copy".to_string(),
                    inputs: vec![f1],
                    options: opts,
                    output_dir: None,
                },
            )
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(manager.cancel(&job_id));

        loop {
            let evt = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .unwrap()
                .unwrap();
            if evt.snapshot.job_id == job_id && evt.snapshot.status == JobStatus::Cancelled {
                break;
            }
        }
        assert_eq!(
            manager.get_job(&job_id).unwrap().status,
            JobStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn validation_rejects_unsupported_extension_cleanly() {
        let dir = tempdir().unwrap();
        let bad = dir.path().join("image.png");
        fs::write(&bad, b"not-txt").unwrap();

        let registry = ToolRegistry::new();
        registry.register(MockCopyTool);
        let manager = JobManager::new(registry, 1);

        let err = manager
            .submit(
                &tokio::runtime::Handle::current(),
                JobRequest {
                    tool_id: "test.copy".to_string(),
                    inputs: vec![bad],
                    options: ToolOptions::new(),
                    output_dir: None,
                },
            )
            .unwrap_err();

        assert!(err.user_message().contains("Unsupported file type"));
    }
}
