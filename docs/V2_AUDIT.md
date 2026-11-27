# Tool Center v2 — Step 0 Architectural & Licensing Audit

## 1. Existing v1 Architecture Summary

- **`Tool` Modules & Schema (`core-jobs/src/{tool.rs,schema.rs}`):**
  Every tool is a self-contained struct implementing `core_jobs::Tool`:
  - `descriptor(&self) -> ToolDescriptor`: declares `id`, plain-language `name`/`description`, `ToolCategory`, `ToolInputSpec` (extensions, min/max files, folder support), declarative `options_schema: Vec<OptionField>`, and `default_suffix`.
  - `validate(&self, inputs, options)`: validates files and parameters synchronously before queueing.
  - `run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError>`: runs on a background blocking thread, reporting progress via `ctx.report_progress(fraction, step)`, checking cancellation via `ctx.check_cancelled()?`, resolving collision-free paths via `ctx.output_path_for(...)` (never overwriting originals), and returning `OutputArtifact`s plus optional structured `report_json` (ideal for OCR bounding boxes & recognized text!).
- **Concurrent Job Queue (`core-jobs/src/queue.rs`):**
  `JobManager` bounds concurrency with a `tokio::sync::Semaphore` (`2..=8` workers), spawns blocking tasks via `tokio::task::spawn_blocking`, stores `JobSnapshot` state in an `RwLock<HashMap>`, and pushes every state/progress update onto a `tokio::sync::broadcast::Sender<JobEvent>`.
- **Bridge Progress & Cancellation (`core/bridge/src/api.rs`):**
  - `submit_tool_job(...) -> Result<String, String>` validates and returns `job_id` immediately without blocking the Flutter UI isolate.
  - `subscribe_job_events(sink: StreamSink<String>)` forwards `JobSnapshot` JSON events from the broadcast channel over `flutter_rust_bridge` v2's `StreamSink` with zero UI polling.
  - `cancel_job(job_id)` (`#[frb(sync)]`) flips `CancelHandle`'s `AtomicBool` and invokes registered `on_cancel` callbacks immediately.
- **Existing `core-ai` Skeleton (`core/core-ai/src/lib.rs` & `ai_models_dialog.dart`):**
  - Had `validate_permissive_model_license` (`MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`), `flagged_excluded_models()`, `OnnxRuntimeEngine` / `OnnxInferenceSession` traits, and a basic `ModelManager` writing `.onnx.part` files.
  - Used a hardcoded 3-item Rust placeholder list with dummy `e3b0c44...` hashes, `V1OfflineTransport` stub (no HTTPS client or `Range` resume), and a read-only Flutter dialog (`ai_models_dialog.dart`).

---

## 2. v1 Gaps That Block v2 & Minimal Non-Breaking Fixes

| # | v1 Gap / Blocker | Minimal Non-Breaking Fix |
| :--- | :--- | :--- |
| 1 | `ToolCategory` enum (`core-jobs` & Dart `ToolCategoryId`) only has `Images, Video, Audio, Pdf, Files, Convert` (unknown values fall back to `Files` in Dart). | Add `ToolCategory::Ai` (`"ai"`) in Rust and `ToolCategoryId.ai` + `categoryAi` pill in Flutter. Zero changes to existing 38 tools. |
| 2 | Model catalog was hardcoded in Rust with placeholder URLs/hashes, single-file-only layout, and `V1OfflineTransport` stub. | Add versioned repo-root `models.json` supporting single-file models and multi-file packs (e.g. PaddleOCR det + cls + rec + dict), backed by `ureq` + `rustls` resumable `Range` `.part` downloads, live progress/cancel stream, custom directory setting, and manual SHA-256-verified offline import. |
| 3 | `ToolContext` does not carry AI settings (custom model folder, hardware execution provider, session cache). | Pass a shared `Arc<AiEngineContext>` (holding active model dir, acceleration preference, and idle-evicting ONNX session pool) into `core_ai::register_ai_tools(&registry, ai_ctx)` when building `EngineState` — keeping `ToolContext` and all 38 v1 tools untouched. |
| 4 | Preview sidebar (`preview_view.dart` `_MetadataSidebar`) truncates `compatibleTools.take(8)`, which hides AI tools behind the 8 v1 image/PDF tools. | Surface dedicated contextual **AI Actions** (`Run OCR`, `Remove Background`, `Upscale`) at the top of the Preview sidebar for images and PDFs. |

---

## 3. Verified Upstream Model License Audit

Every candidate model was audited directly against its official upstream repository `LICENSE` and `README.md`:

- **Included (Verified Permissive):**
  - **U²-Netp (`4.4 MB`, default fast) & U²-Net (`167.8 MB`, high detail):** `Apache-2.0` (`xuebinqin/U-2-Net`, ONNX export via `danielgatis/rembg` `MIT`).
  - **BiRefNet Swin-Tiny General (`213.6 MB`, high-precision edges):** `MIT` (`ZhengPeng7/BiRefNet`).
  - **Real-ESRGAN `x2plus`, `x4plus`, and `x4plus_anime_6B`:** `BSD-3-Clause` (`xinntao/Real-ESRGAN`, Copyright 2021 Xintao Wang).
  - **PP-OCRv4 Detection, Angle Classification, English/Latin Recognition, and CJK+EN Recognition:** `Apache-2.0` (`PaddlePaddle/PaddleOCR` / `RapidAI/RapidOCR`).
- **Excluded & Flagged:**
  - **IS-Net (`xuebinqin/DIS`):** Upstream `README.md` Section 7 states `Apache-2.0` for *"code and evaluation metric"* while referencing separate `DIS5K-Dataset-Terms-of-Use.pdf` (academic/non-commercial dataset terms). Excluded due to weight commercial-use ambiguity.
  - **BRIA RMBG-1.4 & RMBG-2.0 (`briaai`):** Explicitly non-commercial; excluded.
  - **MODNet (`ZHKKKe/MODNet`):** Verified `Apache-2.0` for both code and weights, but portrait-only; `U²-Netp` + `U²-Net` + `BiRefNet-Tiny` cover both portraits and general objects cleanly.
