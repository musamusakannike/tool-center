//! v2 AI engine interface (ONNX Runtime boundary + permissive-license on-demand model manager).

pub fn crate_name() -> &'static str {
    "core-ai"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_crate_name() {
        assert_eq!(crate_name(), "core-ai");
    }
}
