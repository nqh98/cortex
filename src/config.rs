use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

/// Returns the global config directory: ~/.cortex/
fn cortex_dir() -> PathBuf {
    dirs_home().join(".cortex")
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                path: cortex_dir()
                    .join("db.sqlite")
                    .to_string_lossy()
                    .to_string(),
            },
            indexing: IndexingConfig {
                max_file_size_kb: 1024,
                supported_extensions: vec![
                    "rs".to_string(),
                    "py".to_string(),
                    "js".to_string(),
                    "ts".to_string(),
                    "tsx".to_string(),
                    "jsx".to_string(),
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
    pub fn default_config_path() -> PathBuf {
        cortex_dir().join("config.toml")
    }

    pub fn load(path: &Path) -> crate::error::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .map_err(|e| crate::error::CortexError::Config(e.to_string()))?;
            return toml::from_str(&content)
                .map_err(|e| crate::error::CortexError::Config(e.to_string()));
        }

        // No config found, return defaults
        Ok(Self::default())
    }

    pub fn save(&self, path: &Path) -> crate::error::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::error::CortexError::Config(e.to_string()))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::error::CortexError::Config(e.to_string()))?;
        }

        std::fs::write(path, content)
            .map_err(|e| crate::error::CortexError::Config(e.to_string()))?;

        Ok(())
    }
}
