use thiserror::Error;

pub type Result<T> = std::result::Result<T, LynoraError>;

#[derive(Debug, Error)]
pub enum LynoraError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("SQLite error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("missing variable: {0}")]
    MissingVariable(String),
    #[error("invalid collection: {0}")]
    InvalidCollection(String),
    #[error("import error: {0}")]
    Import(String),
    #[error("{0}")]
    Message(String),
}
