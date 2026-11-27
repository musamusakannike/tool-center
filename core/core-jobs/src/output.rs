use std::path::{Path, PathBuf};

/// Computes a non-destructive output file path that **never** overwrites `source`
/// or any existing file on disk.
///
/// - Writes into `output_dir` if supplied, otherwise next to `source`.
/// - Appends `suffix` (e.g. `"_resized"`) to the file stem. If `suffix` is empty
///   and the target path would collide with `source` or an existing file, appends
///   `"_output"` or `"_2"`, `"_3"`, etc.
pub fn resolve_safe_output_path(
    source: &Path,
    output_dir: Option<&Path>,
    suffix: &str,
    new_extension: Option<&str>,
) -> PathBuf {
    let parent = output_dir
        .or_else(|| source.parent())
        .unwrap_or_else(|| Path::new("."));

    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("output");

    let ext = match new_extension {
        Some(e) => e.trim_start_matches('.').to_string(),
        None => source
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string(),
    };

    let build_candidate = |extra: Option<u32>, effective_suffix: &str| -> PathBuf {
        let numbered = match extra {
            Some(n) => format!("{stem}{effective_suffix}_{n}"),
            None => format!("{stem}{effective_suffix}"),
        };
        if ext.is_empty() {
            parent.join(numbered)
        } else {
            parent.join(format!("{numbered}.{ext}"))
        }
    };

    let initial_candidate = build_candidate(None, suffix);
    let same_as_source = paths_refer_to_same_location(source, &initial_candidate);
    let effective_suffix = if same_as_source && suffix.is_empty() {
        "_out"
    } else {
        suffix
    };

    let first = build_candidate(None, effective_suffix);
    if !first.exists() && !paths_refer_to_same_location(source, &first) {
        return first;
    }

    let mut counter = 2u32;
    loop {
        let candidate = build_candidate(Some(counter), effective_suffix);
        if !candidate.exists() && !paths_refer_to_same_location(source, &candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Resolves a safe directory path for multi-file extraction so existing folders are not overwritten.
pub fn resolve_safe_output_dir(source: &Path, output_dir: Option<&Path>, suffix: &str) -> PathBuf {
    let parent = output_dir
        .or_else(|| source.parent())
        .unwrap_or_else(|| Path::new("."));

    let raw_name = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("extracted");
    // Strip double archive extensions such as `.tar` from `archive.tar.gz`
    let stem = raw_name.strip_suffix(".tar").unwrap_or(raw_name);
    let effective_suffix = if suffix.is_empty() { "" } else { suffix };

    let first = parent.join(format!("{stem}{effective_suffix}"));
    if !first.exists() && !paths_refer_to_same_location(source, &first) {
        return first;
    }

    let mut counter = 2u32;
    loop {
        let candidate = parent.join(format!("{stem}{effective_suffix}_{counter}"));
        if !candidate.exists() && !paths_refer_to_same_location(source, &candidate) {
            return candidate;
        }
        counter += 1;
    }
}

fn paths_refer_to_same_location(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}
