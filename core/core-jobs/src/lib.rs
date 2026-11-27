//! Core job queue, common `Tool` trait, option schemas, and non-destructive output helpers.

/// Returns the crate identifier for smoke verification.
pub fn crate_name() -> &'static str {
    "core-jobs"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_crate_name() {
        assert_eq!(crate_name(), "core-jobs");
    }
}
