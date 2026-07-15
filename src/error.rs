use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Python error: {0}")]
    Python(String),

    #[error("Lua error: {0}")]
    Lua(String),

    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("failed to resolve entry path {path:?}")]
    EntryPathNotFound { path: PathBuf },

    #[error("failed to resolve module {module:?} from {from:?}")]
    ModuleNotFound { module: String, from: PathBuf },

    #[error("worker channel closed")]
    WorkerClosed,

    #[error("worker failed to initialize: {0}")]
    WorkerInit(String),

    #[error("invalid script input: {0}")]
    InvalidInput(String),
}

pub type KernelResult<T> = Result<T, KernelError>;
