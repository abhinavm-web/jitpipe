use std::io;
use thiserror::Error;

/// Core error type for all jitpipe operations and utilities.
#[derive(Error, Debug)]
pub enum JitPipeError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Filter compilation failed: {0}")]
    FilterCompilation(String),

    #[error("Filter synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Invalid field or column reference: {0}")]
    InvalidField(String),

    #[error("LLM communication error: {0}")]
    Http(String),

    #[error("Command-line argument error: {0}")]
    Usage(String),
}
