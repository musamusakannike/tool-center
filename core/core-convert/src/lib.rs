//! Cross-domain format conversion tools and streaming file utilities (bulk rename, checksums, duplicate finder).

pub fn crate_name() -> &'static str {
    "core-convert"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_crate_name() {
        assert_eq!(crate_name(), "core-convert");
    }
}
