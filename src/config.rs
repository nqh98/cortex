use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub indexing: IndexingConfig,
    pub embeddings: EmbeddingConfig,
    pub watcher: WatcherConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingConfig {
    pub max_file_size_kb: u64,
    pub supported_extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub model: String,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                path: ".cortex/db.sqlite".to_string(),
            },
            indexing: IndexingConfig {
                max_file_size_kb: 1024,
                supported_extensions: vec![
                    "rs".to_string(),
                    "py".to_string(),
                    "js".to_string(),
                    "ts".to_string(),
                ],
            },
            embeddings: EmbeddingConfig {
                enabled: false,
                model: "AllMiniLML6V2".to_string(),
                batch_size: 32,
            },
            watcher: WatcherConfig {
                debounce_ms: 500,
            },
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> crate::error::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::error::CortexError::Config(e.to_string()))?;
        toml::from_str(&content)
            .map_err(|e| crate::error::CortexError::Config(e.to_string()))
    }
}
