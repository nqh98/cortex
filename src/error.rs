use thiserror::Error;

#[derive(Error, Debug)]
pub enum CortexError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Indexer error: {0}")]
    Indexer(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("MCP server error: {0}")]
    Mcp(String),

    #[error("Watcher error: {0}")]
    Watcher(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),
}

pub type Result<T> = std::result::Result<T, CortexError>;
