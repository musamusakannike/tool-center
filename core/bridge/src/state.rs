use core_jobs::{JobManager, ToolRegistry};
use std::sync::OnceLock;
use tokio::runtime::Runtime;

pub struct EngineState {
    pub runtime: Runtime,
    pub jobs: JobManager,
}

static ENGINE: OnceLock<EngineState> = OnceLock::new();

pub fn engine() -> &'static EngineState {
    ENGINE.get_or_init(|| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .thread_name("tool-center-worker")
            .build()
            .expect("Failed to initialize Tokio background runtime");

        let registry = build_default_registry();
        let concurrency = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 8))
            .unwrap_or(4);
        let jobs = JobManager::new(registry, concurrency);

        EngineState { runtime, jobs }
    })
}

fn build_default_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    core_image::register_image_tools(&registry);
    core_pdf::register_pdf_tools(&registry);
    registry
}
