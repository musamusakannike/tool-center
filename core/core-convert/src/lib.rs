//! Cross-domain format conversion tools and streaming file utilities (bulk rename, checksums, duplicate finder).

pub mod conversions;
pub mod utilities;

use core_jobs::ToolRegistry;

pub use conversions::{
    register_conversion_tools, UnifiedAudioConvertTool, UnifiedImageConvertTool,
    UnifiedImagesToPdfTool, UnifiedPdfToImagesTool, UnifiedVideoConvertTool,
};
pub use utilities::{
    register_utility_tools, BulkRenameTool, DuplicateFinderTool, DuplicateGroup, FileChecksumTool,
};

pub fn register_convert_and_utility_tools(registry: &ToolRegistry) {
    register_conversion_tools(registry);
    register_utility_tools(registry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_jobs::{CancelHandle, Tool, ToolContext, ToolOptions};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_ctx(inputs: Vec<PathBuf>, options: ToolOptions) -> ToolContext {
        ToolContext::new(
            "util-test",
            inputs,
            options,
            None,
            CancelHandle::new(),
            Arc::new(|_, _| {}),
        )
    }

    #[test]
    fn bulk_rename_checksum_and_duplicate_finder_work_without_modifying_originals() {
        let dir = tempdir().unwrap();
        let f1 = dir.path().join("IMG 01.txt");
        let f2 = dir.path().join("IMG 02.txt");
        let f3 = dir.path().join("unique.txt");
        fs::write(&f1, b"identical-payload-bytes").unwrap();
        fs::write(&f2, b"identical-payload-bytes").unwrap();
        fs::write(&f3, b"different-content").unwrap();

        // 1. Bulk Rename (non-destructive!)
        let mut rename_opts = ToolOptions::new();
        rename_opts.insert("find_text", json!("IMG"));
        rename_opts.insert("replace_text", json!("photo"));
        rename_opts.insert("case_mode", json!("snake_case"));
        rename_opts.insert("add_numbering", json!(true));
        let rename_res = BulkRenameTool
            .run(&make_ctx(vec![f1.clone(), f2.clone()], rename_opts))
            .unwrap();
        assert_eq!(rename_res.outputs.len(), 2);
        // Originals remain untouched
        assert!(f1.exists());
        assert!(f2.exists());
        assert!(PathBuf::from(&rename_res.outputs[0].path).exists());

        // 2. Checksum (`all` + verification)
        let mut hash_opts = ToolOptions::new();
        hash_opts.insert("algorithm", json!("sha256"));
        let hash_res = FileChecksumTool
            .run(&make_ctx(vec![f3.clone()], hash_opts))
            .unwrap();
        let sha_hex = hash_res.report_json.unwrap()["files"][0]["hashes"]["sha256"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(sha_hex.len(), 64);

        let mut verify_opts = ToolOptions::new();
        verify_opts.insert("algorithm", json!("sha256"));
        verify_opts.insert("expected_hash", json!(sha_hex));
        let verified = FileChecksumTool
            .run(&make_ctx(vec![f3.clone()], verify_opts))
            .unwrap();
        assert!(verified.summary.contains("VERIFIED"));

        // 3. Duplicate Finder (f1 & f2 + their renamed copies = 4 identical copies -> 1 duplicate group!)
        let dup_res = DuplicateFinderTool
            .run(&make_ctx(
                vec![dir.path().to_path_buf()],
                ToolOptions::new(),
            ))
            .unwrap();
        let report = dup_res.report_json.unwrap();
        assert_eq!(report["duplicate_group_count"], 1);
        assert_eq!(report["groups"][0]["files"].as_array().unwrap().len(), 4);
    }
}
