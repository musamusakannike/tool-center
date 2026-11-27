//! PDF rendering (`pdfium-render`) and manipulation (`lopdf`) tools.

pub fn crate_name() -> &'static str {
    "core-pdf"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_crate_name() {
        assert_eq!(crate_name(), "core-pdf");
    }
}
