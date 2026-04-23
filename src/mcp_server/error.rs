use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP error codes for structured error handling
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Database connection or query error
    DatabaseError,
    /// File not found on disk
    FileNotFound,
    /// Symbol not found in index
    SymbolNotFound,
    /// Invalid or inaccessible path
    InvalidPath,
    /// Indexing operation failed
    IndexingFailed,
    /// Code parsing error
    ParseError,
    /// Multiple symbols match the query (ambiguity)
    AmbiguousSymbol,
    /// Invalid tool parameters
    InvalidParameters,
    /// IO error
    IoError,
    /// Configuration error
    ConfigError,
    /// Internal server error
    InternalError,
}

/// Structured MCP error response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpError {
    /// Error code
    pub code: ErrorCode,
    /// Human-readable error message
    pub message: String,
    /// Additional error details
    pub details: Option<Value>,
}

impl McpError {
    /// Create a new MCP error
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Add details to the error
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Convert Cortex errors to MCP errors
impl From<crate::error::CortexError> for McpError {
    fn from(err: crate::error::CortexError) -> Self {
        use crate::error::CortexError;

        let (code, message) = match err {
            CortexError::Database(msg) => (ErrorCode::DatabaseError, msg),
            CortexError::FileNotFound(msg) => (ErrorCode::FileNotFound, msg),
            CortexError::SymbolNotFound(msg) => (ErrorCode::SymbolNotFound, msg),
            CortexError::Parse(msg) => (ErrorCode::ParseError, msg),
            CortexError::Indexer(msg) => (ErrorCode::IndexingFailed, msg),
            CortexError::Config(msg) => (ErrorCode::ConfigError, msg),
            CortexError::Io(msg) => (ErrorCode::IoError, msg.to_string()),
            CortexError::Query(msg) => (ErrorCode::DatabaseError, msg),
            CortexError::Mcp(msg) => (ErrorCode::InternalError, msg),
            CortexError::Watcher(msg) => (ErrorCode::InternalError, msg),
            CortexError::Embedding(msg) => (ErrorCode::InternalError, msg),
        };

        Self::new(code, message)
    }
}

/// Convert IO errors to MCP errors
impl From<std::io::Error> for McpError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorCode::IoError, err.to_string())
    }
}

/// Convert SQLx errors to MCP errors
impl From<sqlx::Error> for McpError {
    fn from(err: sqlx::Error) -> Self {
        Self::new(ErrorCode::DatabaseError, err.to_string())
    }
}

/// Result type alias for MCP operations
pub type McpResult<T> = Result<T, McpError>;
