use std::path::PathBuf;

/// Errors carry either a source location, an operation, or a generation limit.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{source_name}:{line}:{column}: {message}")]
    Document {
        source_name: String,
        line: usize,
        column: usize,
        message: String,
    },
    #[error("{0}")]
    Invalid(String),
    #[error("cannot {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported generation:\n{0}")]
    Unsupported(String),
}

impl Error {
    pub fn io(operation: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
