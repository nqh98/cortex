pub mod db;

use crate::config::Config;
use crate::error::Result;
use crate::indexer::db::DbPool;
use crate::models::Language;
use crate::parser;
use crate::scanner::walker;
use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::{debug, info, warn};

pub struct Indexer {
    pool: DbPool,
    config: Config,
}

impl Indexer {
    pub async fn new(config: &Config) -> Result<Self> {
        // Ensure the .cortex directory exists
        if let Some(parent) = Path::new(&config.database.path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let pool = db::init_pool(&format!("sqlite:{}", config.database.path)).await?;
        Ok(Self {
            pool,
            config: config.clone(),
        })
    }

    pub async fn index_project(&self, project_path: &Path) -> Result<IndexStats> {
        info!("Indexing project: {}", project_path.display());

        let files = walker::walk_directory(project_path)?;
        info!("Found {} files to index", files.len());

        let project_root = project_path.to_string_lossy().to_string();

        let mut stats = IndexStats::default();

        for file_entry in &files {
            let relative = file_entry.path.strip_prefix(project_path)
                .unwrap_or(&file_entry.path);

            match self.index_file(&project_root, relative, &file_entry.path, file_entry.language).await {
                Ok(IndexFileResult::New(symbols)) => {
                    stats.files_indexed += 1;
                    stats.symbols_found += symbols;
                }
                Ok(IndexFileResult::Unchanged) => {
                    stats.files_unchanged += 1;
                }
                Ok(IndexFileResult::Skipped) => {
                    stats.files_skipped += 1;
                }
                Err(e) => {
                    warn!("Failed to index {}: {}", file_entry.path.display(), e);
                    stats.files_failed += 1;
                }
            }
        }

        info!(
            "Indexing complete: {} indexed, {} unchanged, {} skipped, {} failed, {} symbols",
            stats.files_indexed,
            stats.files_unchanged,
            stats.files_skipped,
            stats.files_failed,
            stats.symbols_found
        );

        Ok(stats)
    }

    pub async fn index_single_file(&self, project_path: &Path, path: &Path, language: Language) -> Result<usize> {
        let project_root = project_path.to_string_lossy().to_string();
        let relative = path.strip_prefix(project_path).unwrap_or(path);
        match self.index_file(&project_root, relative, path, language).await? {
            IndexFileResult::New(n) => Ok(n),
            IndexFileResult::Unchanged => Ok(0),
            IndexFileResult::Skipped => Ok(0),
        }
    }

    async fn index_file(
        &self,
        project_root: &str,
        relative_path: &Path,
        absolute_path: &Path,
        language: Language,
    ) -> Result<IndexFileResult> {
        let content = tokio::fs::read_to_string(absolute_path).await?;

        // Check file size
        let max_size = (self.config.indexing.max_file_size_kb * 1024) as usize;
        if content.len() > max_size {
            debug!("Skipping large file: {}", absolute_path.display());
            return Ok(IndexFileResult::Skipped);
        }

        let hash = compute_hash(&content);
        let path_str = relative_path.to_string_lossy().to_string();

        // Check if file has changed
        let existing_hash = db::get_file_hash(&self.pool, project_root, &path_str).await?;
        if existing_hash.as_deref() == Some(&hash) {
            debug!("Unchanged: {}", relative_path.display());
            return Ok(IndexFileResult::Unchanged);
        }

        // Parse the file
        let parser = parser::get_parser(language);
        let result = parser.parse(&content, absolute_path);

        let symbol_count = result.symbols.len();

        // Store in database
        let file_id = db::upsert_file(&self.pool, project_root, &path_str, &hash, language.as_str()).await?;
        db::insert_symbols(&self.pool, file_id, &result.symbols).await?;
        db::insert_imports(&self.pool, file_id, &result.imports).await?;

        debug!(
            "Indexed {}: {} symbols",
            relative_path.display(),
            symbol_count
        );

        Ok(IndexFileResult::New(symbol_count))
    }
}

enum IndexFileResult {
    New(usize),
    Unchanged,
    Skipped,
}

#[derive(Debug, Default)]
pub struct IndexStats {
    pub files_indexed: usize,
    pub files_unchanged: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub symbols_found: usize,
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
