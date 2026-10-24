use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("network request to {url} failed: {message}")]
    Http { url: String, message: String },

    #[error("refusing to download from untrusted host: {0}")]
    UntrustedUrl(String),

    #[error("hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("size mismatch for {path}: expected {expected} bytes, got {actual}")]
    SizeMismatch {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },

    #[error("invalid JSON in {context}: {source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("archive error in {path}: {message}")]
    Archive { path: PathBuf, message: String },

    #[error("invalid metadata: {0}")]
    Metadata(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("launch failed: {0}")]
    Launch(String),

    #[error("operation cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    pub fn json(context: impl Into<String>, source: serde_json::Error) -> Self {
        Error::Json {
            context: context.into(),
            source,
        }
    }
}

/// Extension trait to attach a path to `std::io::Result`.
pub trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|e| Error::io(path, e))
    }
}
