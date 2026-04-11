use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VibeError {
    #[error("inference error: {0}")]
    Inference(String),

    #[error("backend not available: {0}")]
    BackendUnavailable(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("store error: {0}")]
    Store(String),

    #[error("parse error for {path}: {reason}")]
    Parse { path: PathBuf, reason: String },

    #[error("file not found: {0}")]
    NotFound(PathBuf),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("code graph error: {0}")]
    CodeGraph(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, VibeError>;
