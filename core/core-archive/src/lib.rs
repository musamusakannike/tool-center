//! Streaming archive creation, Zip-Slip-safe extraction, and listing (`zip`, `tar.gz`, `7z`, `zstd`, `xz`).

pub mod engine;
pub mod tools;

pub use engine::{
    create_archive_streaming, detect_archive_kind, extract_archive_streaming, list_archive_entries,
    sanitize_entry_rel_path, ArchiveEntryItem,
};
pub use tools::{
    register_archive_tools, ArchiveCreateTool, ArchiveExtractTool, ArchiveInspectTool,
    ARCHIVE_EXTENSIONS,
};

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
            "archive-test",
            inputs,
            options,
            None,
            CancelHandle::new(),
            Arc::new(|_, _| {}),
        )
    }

    #[test]
    fn zip_slip_path_traversal_is_blocked() {
        assert!(sanitize_entry_rel_path("../../etc/passwd").is_err());
        assert!(sanitize_entry_rel_path("/absolute/evil.sh").is_err());
        assert!(sanitize_entry_rel_path("folder/clean.txt").is_ok());
    }

    #[test]
    fn create_list_and_extract_zip_targz_7z_zstd_and_xz_roundtrip() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("project_files");
        fs::create_dir_all(src_dir.join("nested")).unwrap();
        fs::write(src_dir.join("hello.txt"), b"Hello Tool Center!").unwrap();
        fs::write(src_dir.join("nested/data.json"), b"{\"ok\":true}").unwrap();

        for fmt in ["zip", "tar.gz", "7z", "tar.zst", "tar.xz"] {
            let mut create_opts = ToolOptions::new();
            create_opts.insert("format", json!(fmt));
            let created = ArchiveCreateTool
                .run(&make_ctx(vec![src_dir.clone()], create_opts))
                .unwrap();
            let archive_path = PathBuf::from(&created.outputs[0].path);
            assert!(archive_path.exists(), "Missing created archive for {fmt}");

            let listed = list_archive_entries(&archive_path, 100).unwrap();
            assert!(listed.len() >= 2, "Expected >=2 entries for {fmt}");

            let extracted = ArchiveExtractTool
                .run(&make_ctx(vec![archive_path], ToolOptions::new()))
                .unwrap();
            let extracted_dir = PathBuf::from(&extracted.outputs[0].path);
            let hello = extracted_dir.join("project_files/hello.txt");
            assert_eq!(fs::read(&hello).unwrap(), b"Hello Tool Center!");
        }
    }
}
