pub mod config;
pub mod error;
pub mod indexer;
pub mod models;
pub mod mcp_server;
pub mod parser;
pub mod query;
pub mod scanner;
pub mod watcher;

#[cfg(feature = "embeddings")]
pub mod embeddings;
