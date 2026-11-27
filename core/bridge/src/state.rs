use core_ai::AiRuntimeHub;
use core_jobs::{JobManager, ToolRegistry};
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;

pub struct EngineState {
    pub runtime: Runtime,
    pub jobs: JobManager,
    pub ai: Arc<AiRuntimeHub>,
}

static ENGINE: OnceLock<EngineState> = OnceLock::new();

pub fn engine() -> &'static EngineState {
    ENGINE.get_or_init(|| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("tool-center-worker")
            .build()
            .expect("Failed to initialize Tokio background runtime");

        let default_ai_dir = std::env::temp_dir().join("tool_center_ai_models");
        let ai = Arc::new(
            AiRuntimeHub::new(default_ai_dir)
                .expect("Embedded models.json failed integrity/license validation"),
        );

        let registry = build_default_registry();
        let concurrency = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 8))
            .unwrap_or(4);
        let jobs = JobManager::new(registry, concurrency);

        EngineState { runtime, jobs, ai }
    })
}

fn build_default_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    core_image::register_image_tools(&registry);
    core_pdf::register_pdf_tools(&registry);
    core_media::register_media_tools(&registry);
    core_archive::register_archive_tools(&registry);
    core_convert::register_convert_and_utility_tools(&registry);
    registry
}
