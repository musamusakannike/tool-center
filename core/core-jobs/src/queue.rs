use crate::schema::{ToolDescriptor, ToolOptions};
use crate::tool::{CancelHandle, OutputArtifact, Tool, ToolContext, ToolError, ToolRunResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, Semaphore};
use uuid::Uuid;

/// Lifecycle state of a background job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}

/// Serializable point-in-time view of a job for the Flutter UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSnapshot {
    pub job_id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub status: JobStatus,
    pub progress: f32,
    pub step_message: String,
    pub input_count: usize,
    pub outputs: Vec<OutputArtifact>,
    pub summary: Option<String>,
    pub warnings: Vec<String>,
    pub report_json: Option<String>,
    pub error_message: Option<String>,
    pub error_details: Option<String>,
    pub created_at_ms: u64,
    pub duration_ms: Option<u64>,
}

/// Real-time event broadcast whenever a job transitions or reports progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEvent {
    pub snapshot: JobSnapshot,
}

/// Input payload to enqueue a tool execution job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub tool_id: String,
    pub inputs: Vec<PathBuf>,
    pub options: ToolOptions,
    pub output_dir: Option<PathBuf>,
}

/// Thread-safe registry of all self-contained `Tool` modules.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Arc<RwLock<Vec<Arc<dyn Tool>>>>,
    by_id: Arc<RwLock<HashMap<String, Arc<dyn Tool>>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T: Tool>(&self, tool: T) {
        self.register_arc(Arc::new(tool));
    }

    pub fn register_arc(&self, tool: Arc<dyn Tool>) {
        let id = tool.descriptor().id.clone();
        if let Ok(mut by_id) = self.by_id.write() {
            match by_id.entry(id) {
                std::collections::hash_map::Entry::Occupied(mut existing) => {
                    existing.insert(tool);
                    return;
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(Arc::clone(&tool));
                }
            }
        }
        if let Ok(mut list) = self.tools.write() {
            list.push(tool);
        }
    }

    pub fn get(&self, tool_id: &str) -> Option<Arc<dyn Tool>> {
        self.by_id.read().ok()?.get(tool_id).cloned()
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools
            .read()
            .map(|list| list.iter().map(|t| t.descriptor()).collect())
            .unwrap_or_default()
    }
}

struct ActiveJobRecord {
    snapshot: JobSnapshot,
    cancel: CancelHandle,
}

/// Concurrent background job queue that runs tools on blocking worker threads
/// without ever blocking the caller or UI isolate.
#[derive(Clone)]
pub struct JobManager {
    registry: ToolRegistry,
    jobs: Arc<RwLock<HashMap<String, ActiveJobRecord>>>,
    order: Arc<RwLock<Vec<String>>>,
    semaphore: Arc<Semaphore>,
    events_tx: broadcast::Sender<JobEvent>,
}

impl JobManager {
    pub fn new(registry: ToolRegistry, max_concurrency: usize) -> Self {
        let concurrency = max_concurrency.max(1);
        let (events_tx, _) = broadcast::channel(512);
        Self {
            registry,
            jobs: Arc::new(RwLock::new(HashMap::new())),
            order: Arc::new(RwLock::new(Vec::new())),
            semaphore: Arc::new(Semaphore::new(concurrency)),
            events_tx,
        }
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.events_tx.subscribe()
    }

    /// Validates and enqueues a new job onto the background worker pool, returning its `job_id`.
    pub fn submit(
        &self,
        handle: &tokio::runtime::Handle,
        request: JobRequest,
    ) -> Result<String, ToolError> {
        let tool = self
            .registry
            .get(&request.tool_id)
            .ok_or_else(|| ToolError::UnknownTool {
                tool_id: request.tool_id.clone(),
            })?;

        let descriptor = tool.descriptor();
        let merged_options = request.options.with_defaults(&descriptor.options_schema);
        tool.validate(&request.inputs, &merged_options)?;

        let job_id = Uuid::new_v4().to_string();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let initial_snapshot = JobSnapshot {
            job_id: job_id.clone(),
            tool_id: descriptor.id.clone(),
            tool_name: descriptor.name.clone(),
            status: JobStatus::Queued,
            progress: 0.0,
            step_message: "Queued".to_string(),
            input_count: request.inputs.len(),
            outputs: Vec::new(),
            summary: None,
            warnings: Vec::new(),
            report_json: None,
            error_message: None,
            error_details: None,
            created_at_ms: now_ms,
            duration_ms: None,
        };

        let cancel = CancelHandle::new();
        {
            let mut jobs = self.jobs.write().unwrap();
            jobs.insert(
                job_id.clone(),
                ActiveJobRecord {
                    snapshot: initial_snapshot.clone(),
                    cancel: cancel.clone(),
                },
            );
            let mut order = self.order.write().unwrap();
            order.push(job_id.clone());
        }
        let _ = self.events_tx.send(JobEvent {
            snapshot: initial_snapshot,
        });

        let manager = self.clone();
        let job_id_bg = job_id.clone();
        let inputs = request.inputs;
        let output_dir = request.output_dir;

        handle.spawn(async move {
            let permit = match manager.semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };

            if cancel.is_cancelled() {
                manager.finish_cancelled(&job_id_bg, 0);
                drop(permit);
                return;
            }

            let started = Instant::now();
            manager.update_progress(&job_id_bg, JobStatus::Running, 0.0, "Starting...".into());

            let progress_mgr = manager.clone();
            let progress_job_id = job_id_bg.clone();
            let progress_cb = Arc::new(move |fraction: f32, message: String| {
                progress_mgr.update_progress(
                    &progress_job_id,
                    JobStatus::Running,
                    fraction,
                    message,
                );
            });

            let ctx = ToolContext::new(
                job_id_bg.clone(),
                inputs,
                merged_options,
                output_dir,
                cancel.clone(),
                progress_cb,
            );

            let run_res = tokio::task::spawn_blocking(move || tool.run(&ctx)).await;
            let elapsed_ms = started.elapsed().as_millis() as u64;
            drop(permit);

            match run_res {
                Ok(Ok(result)) => {
                    if cancel.is_cancelled() {
                        manager.finish_cancelled(&job_id_bg, elapsed_ms);
                    } else {
                        manager.finish_completed(&job_id_bg, result, elapsed_ms);
                    }
                }
                Ok(Err(ToolError::Cancelled)) => {
                    manager.finish_cancelled(&job_id_bg, elapsed_ms);
                }
                Ok(Err(err)) => {
                    manager.finish_failed(&job_id_bg, err, elapsed_ms);
                }
                Err(join_err) => {
                    manager.finish_failed(
                        &job_id_bg,
                        ToolError::failed(
                            "Internal worker thread terminated unexpectedly.",
                            Some(join_err.to_string()),
                        ),
                        elapsed_ms,
                    );
                }
            }
        });

        Ok(job_id)
    }

    /// Requests immediate cancellation of a queued or running job.
    pub fn cancel(&self, job_id: &str) -> bool {
        let maybe_cancel = self.jobs.read().ok().and_then(|jobs| {
            jobs.get(job_id)
                .map(|rec| (rec.snapshot.status, rec.cancel.clone()))
        });

        if let Some((status, handle)) = maybe_cancel {
            if matches!(status, JobStatus::Queued | JobStatus::Running) {
                handle.cancel();
                return true;
            }
        }
        false
    }

    pub fn get_job(&self, job_id: &str) -> Option<JobSnapshot> {
        self.jobs
            .read()
            .ok()?
            .get(job_id)
            .map(|rec| rec.snapshot.clone())
    }

    /// Returns all tracked jobs in newest-first order.
    pub fn list_jobs(&self) -> Vec<JobSnapshot> {
        let order = match self.order.read() {
            Ok(o) => o.clone(),
            Err(_) => return Vec::new(),
        };
        let jobs = match self.jobs.read() {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };
        order
            .iter()
            .rev()
            .filter_map(|id| jobs.get(id).map(|rec| rec.snapshot.clone()))
            .collect()
    }

    /// Removes completed, cancelled, and failed jobs from history.
    pub fn clear_finished(&self) {
        let mut jobs = match self.jobs.write() {
            Ok(j) => j,
            Err(_) => return,
        };
        let mut order = match self.order.write() {
            Ok(o) => o,
            Err(_) => return,
        };
        jobs.retain(|_, rec| matches!(rec.snapshot.status, JobStatus::Queued | JobStatus::Running));
        order.retain(|id| jobs.contains_key(id));
    }

    fn update_progress(&self, job_id: &str, status: JobStatus, progress: f32, message: String) {
        let updated = {
            let mut jobs = match self.jobs.write() {
                Ok(j) => j,
                Err(_) => return,
            };
            let Some(rec) = jobs.get_mut(job_id) else {
                return;
            };
            if matches!(
                rec.snapshot.status,
                JobStatus::Completed | JobStatus::Cancelled | JobStatus::Failed
            ) {
                return;
            }
            rec.snapshot.status = status;
            rec.snapshot.progress = progress.clamp(0.0, 1.0);
            rec.snapshot.step_message = message;
            rec.snapshot.clone()
        };
        let _ = self.events_tx.send(JobEvent { snapshot: updated });
    }

    fn finish_completed(&self, job_id: &str, result: ToolRunResult, elapsed_ms: u64) {
        let updated = {
            let mut jobs = match self.jobs.write() {
                Ok(j) => j,
                Err(_) => return,
            };
            let Some(rec) = jobs.get_mut(job_id) else {
                return;
            };
            rec.snapshot.status = JobStatus::Completed;
            rec.snapshot.progress = 1.0;
            rec.snapshot.step_message = "Completed".to_string();
            rec.snapshot.outputs = result.outputs;
            rec.snapshot.summary = Some(result.summary);
            rec.snapshot.warnings = result.warnings;
            rec.snapshot.report_json = result
                .report_json
                .and_then(|v| serde_json::to_string(&v).ok());
            rec.snapshot.duration_ms = Some(elapsed_ms);
            rec.snapshot.clone()
        };
        let _ = self.events_tx.send(JobEvent { snapshot: updated });
    }

    fn finish_cancelled(&self, job_id: &str, elapsed_ms: u64) {
        let updated = {
            let mut jobs = match self.jobs.write() {
                Ok(j) => j,
                Err(_) => return,
            };
            let Some(rec) = jobs.get_mut(job_id) else {
                return;
            };
            rec.snapshot.status = JobStatus::Cancelled;
            rec.snapshot.step_message = "Cancelled".to_string();
            rec.snapshot.duration_ms = Some(elapsed_ms);
            rec.snapshot.clone()
        };
        let _ = self.events_tx.send(JobEvent { snapshot: updated });
    }

    fn finish_failed(&self, job_id: &str, error: ToolError, elapsed_ms: u64) {
        let updated = {
            let mut jobs = match self.jobs.write() {
                Ok(j) => j,
                Err(_) => return,
            };
            let Some(rec) = jobs.get_mut(job_id) else {
                return;
            };
            rec.snapshot.status = JobStatus::Failed;
            rec.snapshot.step_message = "Failed".to_string();
            rec.snapshot.error_message = Some(error.user_message());
            rec.snapshot.error_details = error.technical_details();
            rec.snapshot.duration_ms = Some(elapsed_ms);
            rec.snapshot.clone()
        };
        let _ = self.events_tx.send(JobEvent { snapshot: updated });
    }
}
