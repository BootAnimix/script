use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("descriptor parse error on line {line}: {message}")]
    DescriptorParse { line: usize, message: String },

    #[error("invalid descriptor model: {0}")]
    InvalidDescriptor(String),

    #[error("frame ordering error: {0}")]
    FrameOrder(String),

    #[error("project manifest error: {0}")]
    Manifest(String),

    #[error("archive error: {0}")]
    Archive(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
