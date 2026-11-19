use serde::ser::SerializeStruct;
use vellumi_core::CoreError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error("no document window named {0}")]
    UnknownWindow(String),
    #[error("invalid request: {0}")]
    Ipc(String),
}

impl AppError {
    fn i18n_key(&self) -> &'static str {
        match self {
            AppError::Core(error) => error.i18n_key(),
            AppError::Tauri(_) | AppError::UnknownWindow(_) | AppError::Ipc(_) => "app.error.internal",
        }
    }
}

/// Serialized to the frontend as `{ kind, message }`; `kind` is the
/// localization key, `message` the technical detail.
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", self.i18n_key())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
