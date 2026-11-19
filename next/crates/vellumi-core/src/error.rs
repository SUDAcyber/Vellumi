//! Error types. Every function that touches user data returns these instead
//! of panicking; the desktop shell maps them to localized messages via
//! [`CoreError::i18n_key`] (keys match the 0.5.x Localizable.strings).

use std::path::PathBuf;

/// Why bytes could not be turned into text. Vellumi never guesses an
/// encoding: guessing is how editors corrupt files.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodingError {
    /// Real Markdown never contains NUL; it is the signature of BOM-less
    /// UTF-16 that would otherwise decode as mojibake.
    #[error("file contains NUL bytes (likely UTF-16 without a byte-order mark)")]
    ContainsNul,
    #[error("file is not valid UTF-8 (first invalid byte at offset {valid_up_to})")]
    InvalidUtf8 { valid_up_to: usize },
    #[error("file has a UTF-16 byte-order mark but is not valid UTF-16")]
    InvalidUtf16,
}

/// Why a front-matter block typed by the user was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrontMatterError {
    #[error("front matter must start with a `---` line and be closed by `---` or `...`")]
    NotFenced,
    #[error("nothing may follow the closing fence of the front matter")]
    TrailingContent,
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{op} failed for {path}: {source}")]
    Io {
        op: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is {size} bytes, over the {limit}-byte limit")]
    TooLarge { path: PathBuf, size: u64, limit: u64 },
    #[error("cannot read {path}: {kind}")]
    Encoding {
        path: PathBuf,
        #[source]
        kind: EncodingError,
    },
    #[error(transparent)]
    FrontMatter(#[from] FrontMatterError),
    #[error("{requested} is outside {root}")]
    PathEscapesRoot { requested: String, root: PathBuf },
    #[error("{path} has no parent folder")]
    NoParent { path: PathBuf },
    #[error("the document has not been saved to disk yet")]
    Untitled,
    #[error("image is {size} bytes, over the {limit}-byte limit")]
    ImageTooLarge { size: u64, limit: u64 },
    #[error("{path} changed on disk since it was opened")]
    Conflict { path: PathBuf },
}

impl CoreError {
    /// Stable key for the UI's localized message.
    pub fn i18n_key(&self) -> &'static str {
        match self {
            CoreError::Io { .. } => "document.error.io",
            CoreError::TooLarge { .. } => "document.error.tooLarge",
            CoreError::Encoding { .. } => "document.error.encoding",
            CoreError::FrontMatter(_) => "frontmatter.invalid",
            CoreError::PathEscapesRoot { .. } => "document.error.pathEscapes",
            CoreError::NoParent { .. } => "document.error.io",
            CoreError::Untitled => "image.needsSave.message",
            CoreError::ImageTooLarge { .. } => "image.error.tooLarge",
            CoreError::Conflict { .. } => "document.error.conflict",
        }
    }
}
