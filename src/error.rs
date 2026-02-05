use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum IndexerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Watcher error: {0}")]
    Watcher(String),

    #[error("MCP error: {0}")]
    Mcp(String),
}

pub type Result<T> = std::result::Result<T, IndexerError>;
