//! Streaming archive creation, extraction, and listing (`zip`, `tar.gz`, `7z`, `zstd`, `xz`).

pub fn crate_name() -> &'static str {
    "core-archive"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_crate_name() {
        assert_eq!(crate_name(), "core-archive");
    }
}
