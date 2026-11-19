//! Vellumi core: the parts of the editor that must behave identically on
//! every platform and in a future sync server. Pure logic only — callers do
//! the IO (see `vellumi-fs`).
//!
//! Each module is a port of a Swift file from the 0.5.x app; the doc comment on
//! the module names it, and its tests are ported from the matching XCTest case.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod assets;
pub mod document;
pub mod encoding;
pub mod error;
pub mod export;
pub mod front_matter;
pub mod settings;
pub mod tree;
pub mod fuzzy;
pub mod stats;

pub use error::{CoreError, EncodingError};

/// Files larger than this are refused (docs/PERF.md: serialization in the
/// editor is ~linear, 4 MB ≈ 1 s per flush).
pub const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;

/// Pasted images larger than this are refused (matches the JS-side guard).
pub const MAX_IMAGE_SIZE: u64 = 32 * 1024 * 1024;

/// Extensions listed in projects / quick open / file tree, and openable (D10:
/// `.txt` is now genuinely supported instead of listed-but-unopenable).
pub const SUPPORTED_EXTENSIONS: &[&str] = &["md", "markdown", "txt"];

/// True when `name`'s extension is one Vellumi lists and opens.
pub fn is_supported_file_name(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => SUPPORTED_EXTENSIONS
            .iter()
            .any(|supported| supported.eq_ignore_ascii_case(ext)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_extensions_are_case_insensitive() {
        assert!(is_supported_file_name("a.md"));
        assert!(is_supported_file_name("A.MARKDOWN"));
        assert!(is_supported_file_name("notes.txt"));
        assert!(!is_supported_file_name("image.png"));
        assert!(!is_supported_file_name(".md"));
        assert!(!is_supported_file_name("md"));
    }
}
