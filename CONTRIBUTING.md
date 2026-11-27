# Contributing to Tool Center

Thank you for contributing to Tool Center! Because every tool in Tool Center is a self-contained Rust module with a declarative schema, **adding a new tool requires zero changes to the Flutter UI core**.

---

## Core Design Principles

1. **Never block the UI:** All file parsing, conversion, and compression runs inside Rust worker threads (`core-jobs`).
2. **Stream large files:** Use bounded chunked buffers (`64 KiB` `Read`/`Write` loops) rather than reading entire files into `Vec<u8>` whenever streaming is supported.
3. **Never overwrite originals:** Always resolve output paths through `ctx.output_path_for(source, "_suffix", new_ext)` or `ctx.output_dir_for(source, "_suffix")`.
4. **Plain-language UX:** Every `OptionField` must have both a jargon-free `label` and a concise, helpful `hint`.
5. **Strict Licensing Hygiene:**
   - Crate dependencies must be permissive (`MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib`, `CC0-1.0`).
   - Native bundled engines (`FFmpeg`, `libmpv`) must remain strictly **LGPL** (dynamically linked or spawned as an unmodified sidecar binary — never `--enable-gpl` or `--enable-nonfree`).
   - v2 ONNX models (`core-ai`) must be **MIT or Apache-2.0 only**; non-commercial models (`CC-BY-NC`, custom research licenses) are strictly forbidden.

---

## How to Add a New Tool (Step-by-Step)

### 1. Pick the Domain Crate
Choose the matching crate under `core/`:
- `core-image` — raster/vector image operations
- `core-pdf` — PDF structural or rendering operations
- `core-media` — audio/video operations via the LGPL FFmpeg sidecar
- `core-archive` — archive packing/unpacking
- `core-convert` — cross-domain conversions and general file utilities
- `core-ai` — v2 ONNX Runtime tools

### 2. Implement the `Tool` Trait
Create a struct implementing `core_jobs::Tool`:

```rust
use core_jobs::{
    OptionField, OptionKind, OutputArtifact, Tool, ToolCategory, ToolContext,
    ToolDescriptor, ToolError, ToolInputSpec, ToolRunResult,
};
use serde_json::json;

pub struct MyExampleTool;

impl Tool for MyExampleTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "files.my_example".into(),
            name: "My Example Tool".into(),
            name_key: "tool_my_example".into(),
            description: "One-sentence plain-language summary.".into(),
            description_key: "tool_my_example_desc".into(),
            category: ToolCategory::Files,
            icon: "build".into(),
            keywords: vec!["example".into(), "sample".into()],
            input_spec: ToolInputSpec::single_or_batch(&["txt"]),
            options_schema: vec![OptionField {
                key: "uppercase".into(),
                label: "Convert text to UPPERCASE".into(),
                label_key: "opt_uppercase".into(),
                hint: "Transforms ASCII letters while leaving punctuation intact.".into(),
                hint_key: "opt_uppercase_hint".into(),
                kind: OptionKind::Boolean,
                default_value: json!(true),
                required: false,
                visible_when: None,
            }],
            default_suffix: "_processed".into(),
        }
    }

    fn run(&self, ctx: &ToolContext) -> Result<ToolRunResult, ToolError> {
        let upper = ctx.options.get_bool("uppercase", true);
        let mut outputs = Vec::new();

        for (i, input) in ctx.inputs.iter().enumerate() {
            // 1. Always check cancellation in loops
            ctx.check_cancelled()?;

            // 2. Report [0.0..1.0] progress
            ctx.report_progress(
                i as f32 / ctx.inputs.len() as f32,
                format!("Processing file {} of {}", i + 1, ctx.inputs.len()),
            );

            // 3. Never overwrite originals!
            let out_path = ctx.output_path_for(input, "_processed", None);
            let content = std::fs::read_to_string(input)
                .map_err(|e| ToolError::corrupt(input, "Invalid UTF-8 text file.", Some(e.to_string())))?;
            let transformed = if upper { content.to_uppercase() } else { content };
            std::fs::write(&out_path, transformed.as_bytes())
                .map_err(|e| ToolError::failed("Failed writing output", Some(e.to_string())))?;

            outputs.push(OutputArtifact {
                path: out_path.display().to_string(),
                label: out_path.file_name().unwrap_or_default().to_string_lossy().into(),
                size_bytes: transformed.len() as u64,
                original_size_bytes: Some(content.len() as u64),
            });
        }

        ctx.report_progress(1.0, "Completed");
        Ok(ToolRunResult::with_outputs("Processed files.", outputs))
    }
}
```

### 3. Register the Tool
Add `registry.register(MyExampleTool);` to your crate's `register_*_tools` function (called by `core/bridge/src/state.rs`).

That's it! On next build, your tool automatically appears in:
- The **Home Grid** and its category filter pill
- The **`Ctrl+K` / `Cmd+K` Command Palette**
- The **File Preview "Quick Actions" sidebar** for matching file extensions
- The **3-Step Schema-Driven Workspace** (`SchemaOptionsForm` renders your `options_schema` automatically)

---

## Pre-PR Verification Checklist

Before opening a Pull Request, verify both Rust and Flutter checks pass with zero warnings:

```bash
cargo fmt --manifest-path core/Cargo.toml --all -- --check
cargo clippy --manifest-path core/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path core/Cargo.toml --workspace

cd app
flutter gen-l10n
flutter analyze --fatal-infos
flutter test
```

If you modify FFI signatures in `core/bridge/src/api.rs`, regenerate bindings:

```bash
flutter_rust_bridge_codegen generate --config-file flutter_rust_bridge.yaml
```

## License of Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Tool Center shall be dual-licensed under `MIT OR Apache-2.0`, without any additional terms or conditions.
