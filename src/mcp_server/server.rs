use crate::config::Config;
use crate::indexer::{db, Indexer};
use crate::mcp_server::models::{
    CodeContextResult, ContentMatchEntry, ContentSearchResult, DirectoryListing,
    DocumentSymbolEntry, DocumentSymbolResult, ExportReportResult, FileFrequencyResult,
    FindReferencesResult, GetFileContentRequest, GetFileContentResult, ImportAnalysisResult,
    ImportEntry, IndexResult, IndexStatus, IssueFrequencyResult, KeywordSearchResult,
    ReferenceMatchEntry, ReExportEntry, SearchResult, SuggestionFrequencyResult, SymbolMatch,
    SymbolStats, SynthesizeReportsResult, ToolUsageResult,
};
use crate::query::{content, context, document, imports_query, keyword, references, search};
use crate::scanner::walker;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

const REINDEX_COOLDOWN: Duration = Duration::from_secs(30);
const MAX_POOLS: usize = 10;

#[derive(Debug)]
struct FileCacheEntry {
    content: String,
    mtime: SystemTime,
    cached_at: Instant,
}

#[derive(Debug)]
struct FileCache {
    entries: HashMap<String, FileCacheEntry>,
    max_entries: usize,
    ttl: Duration,
}

impl FileCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: 50,
            ttl: Duration::from_secs(60),
        }
    }

    /// Read a file, using cached content if still valid (same mtime, within TTL).
    fn read(&mut self, path: &Path) -> std::io::Result<String> {
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let path_key = path.to_string_lossy().to_string();

        if let Some(entry) = self.entries.get(&path_key) {
            if entry.cached_at.elapsed() < self.ttl && entry.mtime == mtime {
                return Ok(entry.content.clone());
            }
        }

        let content = std::fs::read_to_string(path)?;

        // Evict oldest entry if at capacity
        if self.entries.len() >= self.max_entries {
            if let Some(evict_key) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.cached_at)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&evict_key);
            }
        }

        self.entries.insert(
            path_key,
            FileCacheEntry {
                content: content.clone(),
                mtime,
                cached_at: Instant::now(),
            },
        );

        Ok(content)
    }
}

#[derive(Debug)]
pub struct CortexServer {
    config: Arc<Config>,
    pools: tokio::sync::RwLock<HashMap<String, db::DbPool>>,
    pool_access: tokio::sync::Mutex<HashMap<String, Instant>>,
    last_index_check: tokio::sync::Mutex<HashMap<String, Instant>>,
    file_cache: std::sync::Mutex<FileCache>,
}

impl CortexServer {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            pools: tokio::sync::RwLock::new(HashMap::new()),
            pool_access: tokio::sync::Mutex::new(HashMap::new()),
            last_index_check: tokio::sync::Mutex::new(HashMap::new()),
            file_cache: std::sync::Mutex::new(FileCache::new()),
        }
    }

    async fn get_pool(&self, project_root: &str) -> crate::error::Result<db::DbPool> {
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(project_root) {
                self.pool_access
                    .lock()
                    .await
                    .insert(project_root.to_string(), Instant::now());
                return Ok(pool.clone());
            }
        }

        let mut pools = self.pools.write().await;
        // Double-check after acquiring write lock
        if let Some(pool) = pools.get(project_root) {
            self.pool_access
                .lock()
                .await
                .insert(project_root.to_string(), Instant::now());
            return Ok(pool.clone());
        }

        // Evict least-recently-used pool if at capacity
        if pools.len() >= MAX_POOLS {
            let access = self.pool_access.lock().await;
            if let Some(evict_key) = access
                .iter()
                .filter(|(k, _)| pools.contains_key(*k))
                .min_by_key(|(_, t)| *t)
                .map(|(k, _)| k.clone())
            {
                drop(access);
                pools.remove(&evict_key);
                self.pool_access.lock().await.remove(&evict_key);
            }
        }

        let db_path = crate::config::project_db_path(Path::new(project_root));
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let pool = db::init_pool(&format!("sqlite:{}", db_path.display())).await?;
        pools.insert(project_root.to_string(), pool.clone());
        self.pool_access
            .lock()
            .await
            .insert(project_root.to_string(), Instant::now());
        Ok(pool)
    }

    async fn ensure_indexed(&self, project_root: &str) -> crate::error::Result<()> {
        // Throttle: skip if checked recently
        {
            let checks = self.last_index_check.lock().await;
            if let Some(last) = checks.get(project_root) {
                if last.elapsed() < REINDEX_COOLDOWN {
                    return Ok(());
                }
            }
        }

        let pool = self.get_pool(project_root).await?;
        let path = Path::new(project_root);

        let (_, _, last_indexed) = db::get_project_stats(&pool, project_root).await?;

        let needs_reindex = match last_indexed {
            None => true,
            Some(ref ts) => {
                let last_time = parse_db_timestamp(ts);
                has_stale_files(path, last_time)
            }
        };

        if needs_reindex {
            let indexer = Indexer::new(&self.config, path).await?;
            indexer.index_project(path).await?;
        }

        self.last_index_check
            .lock()
            .await
            .insert(project_root.to_string(), Instant::now());

        Ok(())
    }
}

fn parse_db_timestamp(ts: &str) -> SystemTime {
    // SQLite CURRENT_TIMESTAMP format: "YYYY-MM-DD HH:MM:SS"
    let parts: Vec<u64> = ts
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .collect();

    if parts.len() < 6 {
        return SystemTime::UNIX_EPOCH;
    }

    let [year, month, day, hour, minute, second] =
        [parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]];

    // Simple days-since-epoch calculation
    let days = days_from_ce(year, month as u8, day as u8);
    let secs = days * 86400 + hour * 3600 + minute * 60 + second;
    // Days from CE to Unix epoch (1970-01-01) = 719163
    let unix_secs = secs.saturating_sub(719163 * 86400);

    SystemTime::UNIX_EPOCH + Duration::from_secs(unix_secs)
}

fn days_from_ce(year: u64, month: u8, day: u8) -> u64 {
    // Algorithm fromchrono
    let y = year as i64;
    let m = month as i64;
    let d = day as i64;
    let days = y * 365 + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + d - 306;
    days as u64 + 366 // offset to CE
}

fn has_stale_files(project_path: &Path, since: SystemTime) -> bool {
    // Tier 1: Check the project root directory mtime itself (O(1))
    if let Ok(meta) = std::fs::metadata(project_path) {
        if let Ok(modified) = meta.modified() {
            if modified <= since {
                // Root dir hasn't changed — no files could have been added/removed
                return false;
            }
        }
    }

    // Tier 2: Check immediate subdirectory mtimes (O(depth-1))
    // Most edits happen inside a small number of top-level dirs (src/, lib/, etc.)
    if let Ok(entries) = std::fs::read_dir(project_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(modified) = meta.modified() {
                        if modified > since {
                            // At least one top-level dir changed — need a real check
                            // Fall through to tier 3 instead of returning true,
                            // because dir mtime alone doesn't mean source files changed.
                            break;
                        }
                    }
                }
            }
        }
    }

    // Tier 3: Walk source files (O(N)) — but early-return on first stale file
    let Ok(files) = walker::walk_directory(project_path) else {
        return false;
    };

    for file_entry in &files {
        if let Ok(metadata) = std::fs::metadata(&file_entry.path) {
            if let Ok(modified) = metadata.modified() {
                if modified > since {
                    return true;
                }
            }
        }
    }
    false
}

/// Search symbols request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSymbolsRequest {
    /// Search query (symbol name pattern, minimum 1 character)
    #[schemars(description = "Search query (symbol name pattern, minimum 1 character)")]
    pub query: String,
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub project_root: String,
    /// Filter by symbol kind
    #[schemars(description = "Filter by symbol kind")]
    pub kind: Option<crate::models::SymbolKind>,
    /// Maximum number of results (default: 50)
    #[schemars(description = "Maximum number of results to return (default: 50, max: 100)")]
    pub limit: Option<u32>,
    /// Offset for pagination (default: 0)
    #[schemars(description = "Offset for pagination (default: 0)")]
    pub offset: Option<u32>,
    /// Search mode: "contains" (default), "exact", or "prefix"
    #[schemars(
        description = "Search mode: 'contains' (default, matches anywhere in name), 'exact' (exact name match), 'prefix' (name starts with query)"
    )]
    pub search_mode: Option<String>,
}

/// Get code context request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCodeContextRequest {
    /// Name of the symbol to retrieve
    #[schemars(description = "Name of the symbol to retrieve")]
    pub symbol_name: String,
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub project_root: String,
    /// Relative path to disambiguate when multiple symbols have the same name
    #[schemars(
        description = "Relative path to the source file (e.g. src/parser/mod.rs). Use when multiple symbols have the same name to disambiguate."
    )]
    pub file_path: Option<String>,
    /// Include surrounding context (lines before/after the symbol)
    #[schemars(
        description = "Number of context lines to include before and after the symbol (default: 0)"
    )]
    pub context_lines: Option<u32>,
}

/// List directory structure request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDirectoryRequest {
    /// Path to the project directory
    #[schemars(description = "Path to the project directory")]
    pub path: String,
    /// Maximum depth of the tree (default: 3)
    #[schemars(description = "Maximum depth of the tree (default: 3)")]
    pub max_depth: Option<usize>,
    /// Filter by file extension
    #[schemars(description = "Filter files by extension (e.g., 'rs', 'py'). Omit for all files.")]
    pub extension: Option<String>,
}

/// List document symbols request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDocumentSymbolsRequest {
    /// Relative path to the source file within the project
    #[schemars(
        description = "Relative path to the source file within the project (e.g. src/parser/mod.rs)"
    )]
    pub file_path: String,
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub project_root: String,
}

/// Search content request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchContentRequest {
    /// Regex or plain text pattern to search for
    #[schemars(description = "Regex or plain text pattern to search for in file contents")]
    pub pattern: String,
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub path: String,
    /// Filter by file extension (e.g., 'ts', 'rs')
    #[schemars(description = "Filter by file extension (e.g., 'ts', 'rs'). Omit for all files.")]
    pub file_extension: Option<String>,
    /// Maximum number of matches to return (default: 50)
    #[schemars(description = "Maximum number of matches to return (default: 50, max: 200)")]
    pub limit: Option<u32>,
    /// Number of context lines before and after each match (default: 2, max: 10)
    #[schemars(
        description = "Number of context lines before and after each match (default: 2, max: 10)"
    )]
    pub context_lines: Option<u32>,
}

/// Find references request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindReferencesRequest {
    /// Name of the symbol to find references for
    #[schemars(description = "Name of the symbol to find references for")]
    pub symbol_name: String,
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub path: String,
    /// Relative path to disambiguate when multiple symbols share the same name
    #[schemars(
        description = "Relative path to disambiguate when multiple symbols share the same name"
    )]
    pub file_path: Option<String>,
    /// Maximum number of results (default: 50)
    #[schemars(description = "Maximum number of results to return (default: 50, max: 100)")]
    pub limit: Option<u32>,
    /// Number of context lines before and after each match (default: 2, max: 10)
    #[schemars(
        description = "Number of context lines before and after each reference (default: 2, max: 10)"
    )]
    pub context_lines: Option<u32>,
}

/// Keyword search request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchByKeywordRequest {
    /// Natural language or keyword query to search for
    #[schemars(
        description = "Natural language or keyword query to search for (e.g., 'rate limiting', 'database query'). Searches tokenized symbol names, signatures, and documentation using FTS5 prefix matching."
    )]
    pub query: String,
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub path: String,
    /// Maximum number of results (default: 50)
    #[schemars(description = "Maximum number of results to return (default: 50, max: 100)")]
    pub limit: Option<u32>,
}

/// Get imports request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetImportsRequest {
    /// Relative path to the source file within the project
    #[schemars(
        description = "Relative path to the source file within the project (e.g. src/parser/mod.rs)"
    )]
    pub file_path: String,
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub project_root: String,
    /// Direction: 'outgoing' (what this file imports), 'incoming' (what imports this file), or 'both'
    #[schemars(
        description = "Direction of import analysis: 'outgoing' (what this file imports), 'incoming' (what imports this file), or 'both' (default: both)"
    )]
    pub direction: Option<String>,
}

/// Export report request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportReportRequest {
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub project_root: String,
    /// Type of task completed: bug_fix, feature, refactoring, exploration, review, or other
    #[schemars(
        description = "Type of task completed: bug_fix, feature, refactoring, exploration, review, or other"
    )]
    pub task_type: String,
    /// Summary of what was accomplished
    #[schemars(description = "Summary of what was accomplished")]
    pub summary: String,
    /// AI model that generated this report (e.g., 'claude-sonnet-4-6', 'gpt-4o')
    #[schemars(
        description = "AI model that generated this report (e.g., 'claude-sonnet-4-6', 'gpt-4o'). Include your model identifier so reports can be tracked per model."
    )]
    pub model: Option<String>,
    /// List of Cortex tool names used during the task
    #[schemars(
        description = "List of Cortex tool names used during the task (e.g. ['search_symbols', 'get_code_context'])"
    )]
    pub tools_used: Option<Vec<String>>,
    /// List of file paths that were modified
    #[schemars(description = "List of file paths that were modified during the task")]
    pub files_modified: Option<Vec<String>>,
    /// List of issues discovered during the task
    #[schemars(description = "List of issues discovered during the task")]
    pub issues_found: Option<Vec<String>>,
    /// List of suggestions for improving the codebase
    #[schemars(description = "List of suggestions for improving the codebase")]
    pub improvement_suggestions: Option<Vec<String>>,
    /// Arbitrary key-value metadata for additional context
    #[schemars(description = "Arbitrary key-value metadata for additional context")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// Synthesize reports request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SynthesizeReportsRequest {
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub project_root: String,
    /// Maximum number of reports to analyze (default: 50, newest first)
    #[schemars(description = "Maximum number of reports to analyze (default: 50, newest first)")]
    pub limit: Option<usize>,
}

/// Index project request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct IndexProjectRequest {
    /// Absolute path to the project directory to index
    #[schemars(description = "Absolute path to the project directory to index")]
    pub path: String,
}

/// Get index status request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetIndexStatusRequest {
    /// Path to the project directory to check
    #[schemars(description = "Path to the project directory to check")]
    pub path: String,
}

/// Get symbol stats request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSymbolStatsRequest {
    /// Absolute path to the project root directory
    #[schemars(description = "Absolute path to the project root directory")]
    pub path: String,
}

/// List files request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListFilesRequest {
    /// Path to the project directory
    #[schemars(description = "Path to the project directory")]
    pub path: String,
    /// Filter by file extension
    #[schemars(description = "Filter files by extension (e.g., 'rs', 'py'). Omit for all files.")]
    pub extension: Option<String>,
    /// Include directories in results
    #[schemars(description = "Include directories in the results (default: false)")]
    pub include_directories: Option<bool>,
    /// Maximum number of results
    #[schemars(description = "Maximum number of files to return (default: 100)")]
    pub limit: Option<usize>,
}

#[tool_router]
impl CortexServer {
    /// Search for code symbols by name pattern matching.
    ///
    /// Usage examples:
    /// - Search for "parser" → finds all symbols containing "parser"
    /// - Filter by kind: {"kind": "struct"} to only find structs
    /// - Use pagination: {"limit": 10, "offset": 20} to browse results
    ///
    /// Returns structured symbol metadata with file locations.
    #[tool(
        description = "Search for code symbols by name. Supports three search modes via search_mode: 'contains' (default, substring match), 'exact' (exact name), 'prefix' (name starts with query). Supports filtering by symbol kind and language, and pagination. Results include file path, line numbers, signature, and language."
    )]
    async fn search_symbols(
        &self,
        Parameters(request): Parameters<SearchSymbolsRequest>,
    ) -> String {
        if request.query.trim().is_empty() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Query cannot be empty"
                }
            })
            .to_string();
        }

        let project_root = match std::path::Path::new(&request.project_root).canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.project_root)
                    }
                })
                .to_string();
            }
        };

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let limit = request.limit.unwrap_or(50).min(100) as usize;
        let offset = request.offset.unwrap_or(0) as usize;
        let kind_filter = request.kind.map(|k| k.as_str());
        let search_mode = request.search_mode.as_deref().unwrap_or("contains");

        let results = match search::search_symbols_paginated(
            &pool,
            &request.query,
            kind_filter,
            limit,
            offset,
            search_mode,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Search failed: {e}")
                    }
                })
                .to_string()
            }
        };

        let total_count =
            match search::count_symbols(&pool, &request.query, kind_filter, search_mode).await {
                Ok(c) => c as u32,
                Err(_) => results.len() as u32, // Fallback to returned count
            };

        let has_more = (offset + limit) < total_count as usize;

        let symbols: Vec<SymbolMatch> = results
            .into_iter()
            .map(|row| SymbolMatch {
                id: row.id,
                name: row.name,
                kind: parse_symbol_kind(&row.kind),
                file_path: row.path,
                project_root: row.project_root,
                start_line: row.start_line,
                end_line: row.end_line,
                signature: row.signature,
                language: row.language,
            })
            .collect();

        serde_json::to_string(&SearchResult {
            symbols,
            total_count,
            has_more,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize results: {e}")
                }
            })
            .to_string()
        })
    }

    /// Get the source code for a symbol by name.
    ///
    /// Use file_path to disambiguate when multiple symbols have the same name.
    /// Returns the full implementation with optional context lines.
    #[tool(
        description = "Get the full source code for a symbol by name. Use file_path to disambiguate when multiple symbols have the same name. Returns structured JSON with code, line numbers, signature, and optional surrounding context. On ambiguity, suggests similar symbol names."
    )]
    async fn get_code_context(
        &self,
        Parameters(request): Parameters<GetCodeContextRequest>,
    ) -> String {
        if request.symbol_name.trim().is_empty() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Symbol name cannot be empty"
                }
            })
            .to_string();
        }

        let project_root = match std::path::Path::new(&request.project_root).canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.project_root)
                    }
                })
                .to_string();
            }
        };

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let symbol =
            match context::lookup_symbol(&pool, request.file_path.as_deref(), &request.symbol_name)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    let error_code = match &e {
                        crate::error::CortexError::SymbolNotFound(_) => "symbol_not_found",
                        crate::error::CortexError::FileNotFound(_) => "file_not_found",
                        _ => "query_error",
                    };
                    let message = e.to_string();

                    // On symbol_not_found, suggest similar symbols
                    if matches!(&e, crate::error::CortexError::SymbolNotFound(_)) {
                        let suggestions = search::search_symbols_paginated(
                            &pool,
                            &request.symbol_name,
                            None,
                            5,
                            0,
                            "contains",
                        )
                        .await
                        .unwrap_or_default();

                        if !suggestions.is_empty() {
                            let suggested: Vec<serde_json::Value> = suggestions
                                .iter()
                                .map(|s| {
                                    serde_json::json!({
                                        "name": s.name,
                                        "kind": s.kind,
                                        "file_path": s.path,
                                        "signature": s.signature
                                    })
                                })
                                .collect();

                            return serde_json::json!({
                                "error": {
                                    "code": error_code,
                                    "message": message,
                                    "suggestions": suggested
                                }
                            })
                            .to_string();
                        }
                    }

                    return serde_json::json!({
                        "error": {
                            "code": error_code,
                            "message": message
                        }
                    })
                    .to_string();
                }
            };

        let abs_path = symbol.absolute_path();
        let file_content = {
            let mut cache = self.file_cache.lock().unwrap();
            match cache.read(Path::new(&abs_path)) {
                Ok(c) => c,
                Err(_) => {
                    return serde_json::json!({
                        "error": {
                            "code": "file_not_found",
                            "message": format!("File not found: {}", abs_path)
                        }
                    })
                    .to_string();
                }
            }
        };

        let ctx = context::extract_code(&symbol, &file_content);

        let context_lines = request.context_lines.unwrap_or(0) as usize;
        let (code, preview, context_before, context_after) =
            self.format_code_with_context(&ctx, context_lines);

        serde_json::to_string(&CodeContextResult {
            symbol_name: ctx.symbol_name,
            kind: parse_symbol_kind(&ctx.kind),
            file_path: ctx.file_path,
            start_line: ctx.start_line,
            end_line: ctx.end_line,
            signature: ctx.signature,
            code,
            preview,
            context_before: if context_before.is_empty() {
                None
            } else {
                Some(context_before)
            },
            context_after: if context_after.is_empty() {
                None
            } else {
                Some(context_after)
            },
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize context: {e}")
                }
            })
            .to_string()
        })
    }

    /// Read the full source code contents of a file.
    #[tool(
        description = "Read the full source code contents of a file within the project. Returns the file content, detected language, and line count. Useful for reviewing entire files, barrel/index files, or files with no declared symbols."
    )]
    async fn get_file_content(
        &self,
        Parameters(request): Parameters<GetFileContentRequest>,
    ) -> String {
        let project_root = match std::path::Path::new(&request.project_root).canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.project_root)
                    }
                })
                .to_string()
            }
        };

        let abs = std::path::Path::new(&project_root).join(&request.file_path);

        // Ensure resolved path stays within the project root
        let canonical_abs = match abs.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "file_not_found",
                        "message": format!("File '{}' not found in project.", request.file_path)
                    }
                })
                .to_string()
            }
        };

        let canonical_root = std::path::Path::new(&project_root);
        match canonical_abs.strip_prefix(canonical_root) {
            Ok(_) => {}
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": "File path escapes project root."
                    }
                })
                .to_string();
            }
        }

        let content = match self.file_cache.lock().unwrap().read(&canonical_abs) {
            Ok(c) => c,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "read_error",
                        "message": format!("Failed to read file: {e}")
                    }
                })
                .to_string();
            }
        };

        let line_count = content.lines().count();
        let language = std::path::Path::new(&request.file_path)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| match ext {
                "rs" => Some("rust"),
                "py" => Some("python"),
                "js" | "jsx" => Some("javascript"),
                "ts" | "tsx" => Some("typescript"),
                "java" => Some("java"),
                _ => None,
            })
            .map(|s| s.to_string());

        serde_json::to_string(&GetFileContentResult {
            file_path: request.file_path,
            project_root,
            language,
            content,
            line_count,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize result: {e}")
                }
            })
            .to_string()
        })
    }

    /// List all symbols defined in a specific file (like LSP documentSymbol).
    ///
    /// Returns symbols sorted by line with parent-child hierarchy.
    #[tool(
        description = "List all symbols defined in a specific file, sorted by line with parent-child hierarchy. Returns structured JSON with symbol metadata."
    )]
    async fn list_document_symbols(
        &self,
        Parameters(request): Parameters<ListDocumentSymbolsRequest>,
    ) -> String {
        let project_root = match std::path::Path::new(&request.project_root).canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.project_root)
                    }
                })
                .to_string()
            }
        };

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let rows =
            match document::list_document_symbols(&pool, &project_root, &request.file_path).await {
                Ok(r) => r,
                Err(e) => {
                    return serde_json::json!({
                        "error": {
                            "code": "database_error",
                            "message": format!("Query failed: {e}")
                        }
                    })
                    .to_string()
                }
            };

        if rows.is_empty() {
            // Check if the file actually exists on disk
            let abs = std::path::Path::new(&project_root).join(&request.file_path);
            if !abs.exists() {
                return serde_json::json!({
                    "error": {
                        "code": "file_not_found",
                        "message": format!("File '{}' not found in project '{}'.", request.file_path, project_root)
                    }
                }).to_string();
            }
            // File exists but has no indexable symbols — check for re-exports (barrel files)
            let (re_exports, note) = match self.file_cache.lock().unwrap().read(&abs) {
                Ok(content) => {
                    let detected = detect_re_exports(&content, &request.file_path);
                    if !detected.is_empty() {
                        (Some(detected), Some("File contains only re-exports (barrel file)".to_string()))
                    } else {
                        (None, Some("File has no indexable symbols or re-exports".to_string()))
                    }
                }
                Err(_) => (None, None),
            };
            return serde_json::to_string(&DocumentSymbolResult {
                file_path: request.file_path,
                project_root,
                language: None,
                symbols: Vec::new(),
                re_exports,
                note,
            })
            .unwrap_or_else(|e| {
                serde_json::json!({
                    "error": {
                        "code": "serialization_error",
                        "message": format!("Failed to serialize result: {e}")
                    }
                })
                .to_string()
            });
        }

        // Build hierarchy: convert rows to entries, then nest children
        let entries: Vec<DocumentSymbolEntry> = rows
            .into_iter()
            .map(|row| DocumentSymbolEntry {
                id: row.id,
                name: row.name,
                kind: parse_symbol_kind(&row.kind),
                start_line: row.start_line,
                end_line: row.end_line,
                start_col: row.start_col,
                end_col: row.end_col,
                signature: row.signature,
                documentation: row.documentation,
                children: Vec::new(),
            })
            .collect();

        let hierarchical = build_symbol_hierarchy(entries);

        serde_json::to_string(&DocumentSymbolResult {
            file_path: request.file_path,
            project_root,
            language: None,
            symbols: hierarchical,
            re_exports: None,
            note: None,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize result: {e}")
                }
            })
            .to_string()
        })
    }

    /// Search for text patterns within indexed source files (like grep).
    ///
    /// Searches file contents using regex or plain text. Finds TODO comments, raw SQL, security patterns, etc.
    #[tool(
        description = "Search file contents by regex or plain text pattern. Returns matched lines with configurable surrounding context (default 2 lines). Supports file extension filtering. Falls back to literal search if regex is invalid. Useful for finding TODOs, raw SQL, security patterns, etc."
    )]
    async fn search_content(
        &self,
        Parameters(request): Parameters<SearchContentRequest>,
    ) -> String {
        if request.pattern.trim().is_empty() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Pattern cannot be empty"
                }
            })
            .to_string();
        }

        let project_root = match std::path::Path::new(&request.path).canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.path)
                    }
                })
                .to_string()
            }
        };

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let limit = request.limit.unwrap_or(50).min(200) as usize;
        let context_lines = request.context_lines.unwrap_or(2).min(10) as usize;
        let ext = request.file_extension.as_deref();

        let matches = match content::search_content(
            &pool,
            &project_root,
            &request.pattern,
            ext,
            limit,
            context_lines,
        )
        .await
        {
            Ok(m) => m,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "search_error",
                        "message": format!("Content search failed: {e}")
                    }
                })
                .to_string()
            }
        };

        let total_count = matches.len() as u32;
        let has_more = total_count >= limit as u32;

        let entries: Vec<ContentMatchEntry> = matches
            .into_iter()
            .map(|m| ContentMatchEntry {
                file_path: m.file_path,
                project_root: m.project_root,
                line_number: m.line_number,
                line_content: m.line_content,
                context_before: m.context_before,
                context_after: m.context_after,
            })
            .collect();

        serde_json::to_string(&ContentSearchResult {
            pattern: request.pattern,
            total_count,
            has_more,
            matches: entries,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize result: {e}")
                }
            })
            .to_string()
        })
    }

    /// Find all references to a symbol across the project.
    ///
    /// Classifies each reference as import, call, type usage, definition, or other.
    #[tool(
        description = "Find references to a symbol by name across the project. Uses text search with heuristic classification (import, call, type_usage, definition, other). Note: does not track aliased or renamed imports. Returns file locations with line content and configurable context."
    )]
    async fn find_references(
        &self,
        Parameters(request): Parameters<FindReferencesRequest>,
    ) -> String {
        if request.symbol_name.trim().is_empty() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Symbol name cannot be empty"
                }
            })
            .to_string();
        }

        let project_root = match std::path::Path::new(&request.path).canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.path)
                    }
                })
                .to_string()
            }
        };

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let limit = request.limit.unwrap_or(50).min(100) as usize;
        let context_lines = request.context_lines.unwrap_or(2).min(10) as usize;

        let refs = match references::find_references(
            &pool,
            &project_root,
            &request.symbol_name,
            request.file_path.as_deref(),
            limit,
            context_lines,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "query_error",
                        "message": format!("Find references failed: {e}")
                    }
                })
                .to_string()
            }
        };

        let total_count = refs.len() as u32;
        let has_more = total_count >= limit as u32;

        let entries: Vec<ReferenceMatchEntry> = refs
            .into_iter()
            .map(|r| ReferenceMatchEntry {
                file_path: r.file_path,
                project_root: r.project_root,
                line_number: r.line_number,
                line_content: r.line_content,
                reference_type: match r.reference_type {
                    references::ReferenceType::Import => "import".to_string(),
                    references::ReferenceType::Call => "call".to_string(),
                    references::ReferenceType::TypeUsage => "type_usage".to_string(),
                    references::ReferenceType::Definition => "definition".to_string(),
                    references::ReferenceType::Other => "other".to_string(),
                },
            })
            .collect();

        serde_json::to_string(&FindReferencesResult {
            symbol_name: request.symbol_name,
            total_count,
            has_more,
            references: entries,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize result: {e}")
                }
            })
            .to_string()
        })
    }

    /// Search for symbols by keyword using full-text search.
    ///
    /// Uses FTS5 prefix matching across tokenized symbol names, signatures, and documentation.
    #[tool(
        description = "Search for symbols by keyword using full-text search (FTS5). Matches tokenized symbol names, signatures, and documentation with prefix matching. Best for keyword-based lookups like 'rate limiting', 'database connection'. Not embedding-based semantic search."
    )]
    async fn search_by_keyword(
        &self,
        Parameters(request): Parameters<SearchByKeywordRequest>,
    ) -> String {
        if request.query.trim().is_empty() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Query cannot be empty"
                }
            })
            .to_string();
        }

        let project_root = match std::path::Path::new(&request.path).canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.path)
                    }
                })
                .to_string();
            }
        };

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let limit = request.limit.unwrap_or(50).min(100) as usize;

        let results =
            match keyword::search_by_keyword(&pool, &request.query, &project_root, limit).await {
                Ok(r) => r,
                Err(e) => {
                    return serde_json::json!({
                        "error": {
                            "code": "query_error",
                            "message": format!("Keyword search failed: {e}")
                        }
                    })
                    .to_string()
                }
            };

        let total_count =
            match keyword::count_keyword_results(&pool, &request.query, &project_root).await {
                Ok(c) => c as u32,
                Err(_) => results.len() as u32,
            };

        let has_more = (results.len()) < total_count as usize;

        let symbols: Vec<SymbolMatch> = results
            .into_iter()
            .map(|row| SymbolMatch {
                id: row.id,
                name: row.name,
                kind: parse_symbol_kind(&row.kind),
                file_path: row.path,
                project_root: row.project_root,
                start_line: row.start_line,
                end_line: row.end_line,
                signature: row.signature,
                language: row.language,
            })
            .collect();

        serde_json::to_string(&KeywordSearchResult {
            query: request.query,
            total_count,
            has_more,
            symbols,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize result: {e}")
                }
            })
            .to_string()
        })
    }

    /// Analyze import dependencies for a file.
    ///
    /// Shows outgoing imports (what this file imports) and/or incoming imports (what imports this file).
    #[tool(
        description = "Analyze import dependencies for a file. Shows outgoing (what this file imports) and incoming (what imports this file) dependencies. Returns structured JSON with import details."
    )]
    async fn get_imports(&self, Parameters(request): Parameters<GetImportsRequest>) -> String {
        let project_root = match std::path::Path::new(&request.project_root).canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.project_root)
                    }
                })
                .to_string();
            }
        };

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let direction = request.direction.as_deref().unwrap_or("both");

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let analysis =
            match imports_query::get_imports(&pool, &project_root, &request.file_path, direction)
                .await
            {
                Ok(a) => a,
                Err(e) => {
                    return serde_json::json!({
                        "error": {
                            "code": "query_error",
                            "message": format!("Import analysis failed: {e}")
                        }
                    })
                    .to_string()
                }
            };

        let outgoing: Vec<ImportEntry> = analysis
            .outgoing
            .into_iter()
            .map(|r| ImportEntry {
                id: r.id,
                imported_symbol: r.imported_symbol,
                imported_from_path: r.imported_from_path,
                import_type: r.import_type,
                start_line: r.start_line,
                raw_statement: r.raw_statement,
                file_path: r.file_path,
                project_root: r.project_root,
            })
            .collect();

        let incoming: Vec<ImportEntry> = analysis
            .incoming
            .into_iter()
            .map(|r| ImportEntry {
                id: r.id,
                imported_symbol: r.imported_symbol,
                imported_from_path: r.imported_from_path,
                import_type: r.import_type,
                start_line: r.start_line,
                raw_statement: r.raw_statement,
                file_path: r.file_path,
                project_root: r.project_root,
            })
            .collect();

        serde_json::to_string(&ImportAnalysisResult {
            file_path: request.file_path,
            project_root,
            outgoing,
            incoming,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize result: {e}")
                }
            })
            .to_string()
        })
    }

    /// List the directory structure of a project.
    ///
    /// Returns a structured list of files and directories with metadata.
    #[tool(
        description = "List the directory structure of a project. Returns structured JSON list of files and directories with metadata including file types and languages."
    )]
    async fn list_directory_structure(
        &self,
        Parameters(request): Parameters<ListDirectoryRequest>,
    ) -> String {
        let path = Path::new(&request.path);
        if !path.exists() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_path",
                    "message": format!("Directory not found: {}", request.path)
                }
            })
            .to_string();
        }

        let max_depth = Some(request.max_depth.unwrap_or(3));
        let extension_filter = request.extension.as_deref();

        let (entries, root_name) =
            match walker::directory_tree_structured(path, max_depth, extension_filter) {
                Ok(r) => r,
                Err(e) => {
                    return serde_json::json!({
                        "error": {
                            "code": "io_error",
                            "message": format!("Failed to list directory: {e}")
                        }
                    })
                    .to_string()
                }
            };

        let file_count = entries
            .iter()
            .filter(|e| e.entry_type == crate::mcp_server::models::FileType::File)
            .count();
        let directory_count = entries
            .iter()
            .filter(|e| e.entry_type == crate::mcp_server::models::FileType::Directory)
            .count();

        serde_json::to_string(&DirectoryListing {
            root: root_name,
            entries,
            file_count,
            directory_count,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize listing: {e}")
                }
            })
            .to_string()
        })
    }

    /// Index or re-index a project directory.
    ///
    /// Call this after code changes to refresh the symbol index before searching.
    #[tool(
        description = "Index or re-index a project directory. Call this after code changes to refresh the symbol index before searching. Returns structured JSON with indexing statistics."
    )]
    async fn index_project(&self, Parameters(request): Parameters<IndexProjectRequest>) -> String {
        let path = Path::new(&request.path);
        if !path.exists() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_path",
                    "message": format!("Directory not found: {}", request.path)
                }
            })
            .to_string();
        }

        let indexer = match Indexer::new(&self.config, path).await {
            Ok(i) => i,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "indexing_failed",
                        "message": format!("Failed to create indexer: {e}")
                    }
                })
                .to_string()
            }
        };

        let start = Instant::now();
        let stats = match indexer.index_project(path).await {
            Ok(s) => s,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "indexing_failed",
                        "message": format!("Indexing failed: {e}")
                    }
                })
                .to_string()
            }
        };
        let duration_ms = start.elapsed().as_millis() as u64;

        let project_root = path
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| request.path.clone());

        serde_json::to_string(&IndexResult {
            files_indexed: stats.files_indexed as u32,
            files_unchanged: stats.files_unchanged as u32,
            files_failed: stats.files_failed as u32,
            symbols_found: stats.symbols_found as u32,
            duration_ms,
            project_root,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize index result: {e}")
                }
            })
            .to_string()
        })
    }

    /// Check if a project is indexed and get its status.
    #[tool(
        description = "Check if a project is indexed and get its status including file count, symbol count, and last indexed time. Returns structured JSON with project metadata."
    )]
    async fn get_index_status(
        &self,
        Parameters(request): Parameters<GetIndexStatusRequest>,
    ) -> String {
        let path = Path::new(&request.path);
        let project_root = path
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| request.path.clone());

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let (file_count, symbol_count, last_indexed) =
            match db::get_project_stats(&pool, &project_root).await {
                Ok(s) => s,
                Err(e) => {
                    return serde_json::json!({
                        "error": {
                            "code": "database_error",
                            "message": format!("Failed to get project stats: {e}")
                        }
                    })
                    .to_string()
                }
            };

        let languages = match db::get_project_languages(&pool, &project_root).await {
            Ok(l) => l,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to get project languages: {e}")
                    }
                })
                .to_string()
            }
        };

        let is_indexed = file_count > 0;

        serde_json::to_string(&IndexStatus {
            is_indexed,
            file_count,
            symbol_count,
            last_indexed_at: last_indexed,
            project_root,
            languages,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize status: {e}")
                }
            })
            .to_string()
        })
    }

    /// Export a task report to the .cortex/reports/ directory.
    ///
    /// Call this after completing a task to record what was done, issues found, and
    /// improvement suggestions. Reports can later be synthesized with synthesize_reports.
    #[tool(
        description = "Export a task report after completing work. Saves a structured JSON report to the project's .cortex/reports/ directory. Reports can be synthesized later with synthesize_reports to identify patterns and improvement opportunities."
    )]
    async fn export_report(&self, Parameters(request): Parameters<ExportReportRequest>) -> String {
        let path = Path::new(&request.project_root);
        let project_root = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.project_root)
                    }
                })
                .to_string()
            }
        };

        let valid_task_types = [
            "bug_fix",
            "feature",
            "refactoring",
            "exploration",
            "review",
            "other",
        ];
        let task_type = request.task_type.to_lowercase();
        if !valid_task_types.contains(&task_type.as_str()) {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": format!("Invalid task_type '{}'. Must be one of: {}", request.task_type, valid_task_types.join(", "))
                }
            }).to_string();
        }

        if request.summary.trim().is_empty() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Summary cannot be empty"
                }
            })
            .to_string();
        }

        if request.model.as_ref().is_none_or(|m| m.trim().is_empty()) {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Model name is required. Include your AI model identifier (e.g., 'claude-sonnet-4-6', 'gpt-4o') so reports can be tracked per model."
                }
            })
            .to_string();
        }

        let report = crate::report::TaskReport {
            id: String::new(),           // generated by save_report
            timestamp: String::new(),    // generated by save_report
            project_root: String::new(), // generated by save_report
            task_type,
            summary: request.summary,
            model: request.model.unwrap_or_default(),
            tools_used: request.tools_used.unwrap_or_default(),
            files_modified: request.files_modified.unwrap_or_default(),
            issues_found: request.issues_found.unwrap_or_default(),
            improvement_suggestions: request.improvement_suggestions.unwrap_or_default(),
            metadata: request.metadata.unwrap_or_default(),
        };

        let file_path = match crate::report::save_report(&project_root, report) {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "io_error",
                        "message": format!("Failed to save report: {e}")
                    }
                })
                .to_string()
            }
        };

        // Read back to get the generated id and timestamp
        let saved: crate::report::TaskReport = match std::fs::read_to_string(&file_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
        {
            Some(r) => r,
            None => {
                return serde_json::json!({
                    "error": {
                        "code": "io_error",
                        "message": "Failed to read back saved report"
                    }
                })
                .to_string()
            }
        };

        serde_json::to_string(&ExportReportResult {
            report_id: saved.id,
            file_path: file_path.to_string_lossy().to_string(),
            project_root: saved.project_root,
            timestamp: saved.timestamp,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize result: {e}")
                }
            })
            .to_string()
        })
    }

    /// Synthesize past task reports to identify patterns and improvements.
    ///
    /// Reads all reports in .cortex/reports/ and returns aggregated insights including
    /// recurring issues, improvement suggestions, frequently modified files, and tool usage.
    #[tool(
        description = "Synthesize past task reports to identify patterns, recurring issues, and improvement opportunities. Reads all reports from the project's .cortex/reports/ directory and returns aggregated insights."
    )]
    async fn synthesize_reports(
        &self,
        Parameters(request): Parameters<SynthesizeReportsRequest>,
    ) -> String {
        let path = Path::new(&request.project_root);
        let project_root = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return serde_json::json!({
                    "error": {
                        "code": "invalid_path",
                        "message": format!("Project root not found: {}", request.project_root)
                    }
                })
                .to_string()
            }
        };

        let limit = request.limit.unwrap_or(50);

        let total_count = match crate::report::count_reports(&project_root) {
            Ok(c) => c,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "io_error",
                        "message": format!("Failed to count reports: {e}")
                    }
                })
                .to_string()
            }
        };

        if total_count == 0 {
            return serde_json::to_string(&SynthesizeReportsResult {
                total_reports: 0,
                reports_analyzed: 0,
                date_range: None,
                task_type_breakdown: HashMap::new(),
                model_breakdown: HashMap::new(),
                frequently_modified_files: Vec::new(),
                recurring_issues: Vec::new(),
                improvement_suggestions: Vec::new(),
                tools_usage: Vec::new(),
                summary: "No reports found for this project.".to_string(),
            })
            .unwrap_or_else(|_| "{}".to_string());
        }

        let reports = match crate::report::load_reports(&project_root, limit) {
            Ok(r) => r,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "io_error",
                        "message": format!("Failed to load reports: {e}")
                    }
                })
                .to_string()
            }
        };

        let result = crate::report::synthesize(&reports, total_count);

        // Convert internal types to MCP response types
        let date_range = result
            .date_range
            .map(|dr| crate::mcp_server::models::DateRangeResult {
                from: dr.from,
                to: dr.to,
            });

        let frequently_modified_files: Vec<FileFrequencyResult> = result
            .frequently_modified_files
            .into_iter()
            .map(|f| FileFrequencyResult {
                file_path: f.file_path,
                count: f.count,
            })
            .collect();

        let recurring_issues: Vec<IssueFrequencyResult> = result
            .recurring_issues
            .into_iter()
            .map(|i| IssueFrequencyResult {
                issue: i.issue,
                count: i.count,
            })
            .collect();

        let improvement_suggestions: Vec<SuggestionFrequencyResult> = result
            .improvement_suggestions
            .into_iter()
            .map(|s| SuggestionFrequencyResult {
                suggestion: s.suggestion,
                count: s.count,
            })
            .collect();

        let tools_usage: Vec<ToolUsageResult> = result
            .tools_usage
            .into_iter()
            .map(|t| ToolUsageResult {
                tool: t.tool,
                count: t.count,
            })
            .collect();

        serde_json::to_string(&SynthesizeReportsResult {
            total_reports: result.total_reports,
            reports_analyzed: result.reports_analyzed,
            date_range,
            task_type_breakdown: result.task_type_breakdown,
            model_breakdown: result.model_breakdown,
            frequently_modified_files,
            recurring_issues,
            improvement_suggestions,
            tools_usage,
            summary: result.summary,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize result: {e}")
                }
            })
            .to_string()
        })
    }

    /// List files in a project with optional filtering.
    #[tool(
        description = "List files in a project with optional filtering by extension. Returns structured JSON list of file entries with metadata."
    )]
    async fn list_files(&self, Parameters(request): Parameters<ListFilesRequest>) -> String {
        let path = Path::new(&request.path);
        if !path.exists() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_path",
                    "message": format!("Directory not found: {}", request.path)
                }
            })
            .to_string();
        }

        let include_directories = request.include_directories.unwrap_or(false);
        let extension_filter = request.extension.as_deref();
        let limit = request.limit.unwrap_or(100);

        let entries =
            match walker::list_files_structured(path, extension_filter, include_directories, limit)
            {
                Ok(e) => e,
                Err(e) => {
                    return serde_json::json!({
                        "error": {
                            "code": "io_error",
                            "message": format!("Failed to list files: {e}")
                        }
                    })
                    .to_string()
                }
            };

        let total_count = entries.len();
        let has_more = total_count >= limit;

        serde_json::to_string(&crate::mcp_server::models::FileListResult {
            path: request.path,
            files: entries,
            total_count,
            has_more,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize files: {e}")
                }
            })
            .to_string()
        })
    }

    /// Get all available symbol kinds that can be used as filters.
    #[tool(
        description = "Get all available symbol kinds (function, struct, class, etc.) that can be used as filters in search. Returns JSON array of symbol kinds."
    )]
    async fn list_symbol_kinds(&self) -> String {
        serde_json::to_string(&vec![
            crate::models::SymbolKind::Function,
            crate::models::SymbolKind::Struct,
            crate::models::SymbolKind::Impl,
            crate::models::SymbolKind::Trait,
            crate::models::SymbolKind::Interface,
            crate::models::SymbolKind::Enum,
            crate::models::SymbolKind::TypeAlias,
            crate::models::SymbolKind::Constant,
            crate::models::SymbolKind::Module,
            crate::models::SymbolKind::Class,
            crate::models::SymbolKind::Method,
        ])
        .unwrap_or_else(|_| "[]".to_string())
    }

    /// Get statistics about symbols in the index.
    #[tool(
        description = "Get statistics about symbols in the index including total count and breakdown by kind and language. Returns structured JSON with symbol statistics."
    )]
    async fn get_symbol_stats(
        &self,
        Parameters(request): Parameters<GetSymbolStatsRequest>,
    ) -> String {
        let path = Path::new(&request.path);
        let project_root = path
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| request.path.clone());

        if let Err(e) = self.ensure_indexed(&project_root).await {
            return serde_json::json!({
                "error": {
                    "code": "auto_index_failed",
                    "message": format!("Auto-indexing failed: {e}")
                }
            })
            .to_string();
        }

        let pool = match self.get_pool(&project_root).await {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to connect to database: {e}")
                    }
                })
                .to_string()
            }
        };

        let total_symbols = match db::get_total_symbol_count(&pool).await {
            Ok(c) => c,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to get total symbol count: {e}")
                    }
                })
                .to_string()
            }
        };

        let by_kind = match db::get_symbols_by_kind(&pool).await {
            Ok(b) => b,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to get symbols by kind: {e}")
                    }
                })
                .to_string()
            }
        };

        let by_language = match db::get_symbols_by_language(&pool).await {
            Ok(b) => b,
            Err(e) => {
                return serde_json::json!({
                    "error": {
                        "code": "database_error",
                        "message": format!("Failed to get symbols by language: {e}")
                    }
                })
                .to_string()
            }
        };

        serde_json::to_string(&SymbolStats {
            total_symbols,
            by_kind,
            by_language,
        })
        .unwrap_or_else(|e| {
            serde_json::json!({
                "error": {
                    "code": "serialization_error",
                    "message": format!("Failed to serialize stats: {e}")
                }
            })
            .to_string()
        })
    }
}

impl CortexServer {
    fn format_code_with_context(
        &self,
        ctx: &context::CodeContext,
        context_lines: usize,
    ) -> (String, String, Vec<String>, Vec<String>) {
        let lines: Vec<&str> = ctx.code.lines().collect();

        // Code without line numbers
        let code = lines.join("\n");

        // Preview with line numbers (first 10 lines)
        let preview: Vec<String> = lines
            .iter()
            .take(10)
            .enumerate()
            .map(|(i, line)| format!("{:>4} | {}", ctx.start_line as usize + i, line))
            .collect();
        let preview = preview.join("\n");

        // Context lines
        let (before, after) = if context_lines > 0 {
            let start_idx = (ctx.start_line as usize).saturating_sub(1);
            let end_idx = (ctx.end_line as usize).min(lines.len());

            let before_lines: Vec<String> = lines
                .iter()
                .take(start_idx)
                .skip(start_idx.saturating_sub(context_lines))
                .enumerate()
                .map(|(i, line)| format!("{:>4} | {}", start_idx - context_lines + i + 1, line))
                .collect();

            let after_lines: Vec<String> = lines
                .iter()
                .skip(end_idx)
                .take(context_lines)
                .enumerate()
                .map(|(i, line)| format!("{:>4} | {}", end_idx + i + 1, line))
                .collect();

            (before_lines, after_lines)
        } else {
            (vec![], vec![])
        };

        (code, preview, before, after)
    }
}

fn parse_symbol_kind(kind_str: &str) -> crate::models::SymbolKind {
    match kind_str {
        "function" => crate::models::SymbolKind::Function,
        "struct" => crate::models::SymbolKind::Struct,
        "impl" => crate::models::SymbolKind::Impl,
        "trait" => crate::models::SymbolKind::Trait,
        "interface" => crate::models::SymbolKind::Interface,
        "enum" => crate::models::SymbolKind::Enum,
        "type_alias" => crate::models::SymbolKind::TypeAlias,
        "constant" => crate::models::SymbolKind::Constant,
        "module" => crate::models::SymbolKind::Module,
        "class" => crate::models::SymbolKind::Class,
        "method" => crate::models::SymbolKind::Method,
        _ => crate::models::SymbolKind::Function, // Default fallback
    }
}

/// Build parent-child hierarchy from a flat list of symbols sorted by start_line.
/// A symbol B is a child of A if A.start_line <= B.start_line && B.end_line <= A.end_line.
fn build_symbol_hierarchy(mut entries: Vec<DocumentSymbolEntry>) -> Vec<DocumentSymbolEntry> {
    if entries.is_empty() {
        return entries;
    }

    // Process from end to start so we can move children into parents without
    // disturbing indices we haven't visited yet.
    let mut i = entries.len();
    while i > 0 {
        i -= 1;
        let (start, end) = (entries[i].start_line, entries[i].end_line);

        // Collect indices of entries[j] (j > i) that fit within [start, end]
        let mut child_indices: Vec<usize> = Vec::new();
        let mut j = i + 1;
        while j < entries.len() {
            let cj = entries[j].start_line;
            let ce = entries[j].end_line;
            if cj >= start && ce <= end {
                // Check if already claimed by a closer parent
                let claimed = child_indices
                    .iter()
                    .any(|&ci| entries[ci].start_line <= cj && ce <= entries[ci].end_line);
                if !claimed {
                    child_indices.push(j);
                }
            }
            j += 1;
        }

        if !child_indices.is_empty() {
            // Remove children in reverse index order to preserve positions
            let mut children: Vec<DocumentSymbolEntry> = Vec::new();
            for &ci in child_indices.iter().rev() {
                children.push(entries.remove(ci));
            }
            children.reverse();
            // Recursively build hierarchy for children
            children = build_symbol_hierarchy(children);
            entries[i].children = children;
        }
    }

    entries
}

/// Detect re-export patterns in file content.
/// Covers JS/TS (`export * from`, `export { ... } from`) and Python (`from . import *`).
fn detect_re_exports(content: &str, file_path: &str) -> Vec<ReExportEntry> {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "ts" | "tsx" | "js" | "jsx" => detect_js_ts_re_exports(content),
        "py" => detect_python_re_exports(content),
        _ => Vec::new(),
    }
}

fn detect_js_ts_re_exports(content: &str) -> Vec<ReExportEntry> {
    let mut re_exports = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = (i + 1) as i64;

        // `export * from './module'` or `export * as name from './module'`
        if trimmed.starts_with("export *") {
            if let Some(source) = extract_from_path(trimmed) {
                re_exports.push(ReExportEntry {
                    exported_symbols: None,
                    source_path: source,
                    start_line: line_num,
                });
            }
        }
        // `export { foo, bar } from './module'`
        else if trimmed.starts_with("export {") {
            if let Some(source) = extract_from_path(trimmed) {
                let symbols = extract_exported_names(trimmed);
                re_exports.push(ReExportEntry {
                    exported_symbols: Some(symbols),
                    source_path: source,
                    start_line: line_num,
                });
            }
        }
    }
    re_exports
}

fn extract_from_path(line: &str) -> Option<String> {
    let from_idx = line.find(" from ")?;
    let after_from = &line[from_idx + 6..].trim();
    Some(
        after_from
            .trim_matches(|c| c == '\'' || c == '"' || c == '`')
            .trim_end_matches(';')
            .to_string(),
    )
}

fn extract_exported_names(line: &str) -> Vec<String> {
    let open = match line.find('{') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let close = match line.find('}') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let inner = &line[open + 1..close];
    inner
        .split(',')
        .filter_map(|s| {
            let name = s.trim();
            let base = name.split(" as ").next().unwrap_or(name).trim();
            if !base.is_empty() {
                Some(base.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn detect_python_re_exports(content: &str) -> Vec<ReExportEntry> {
    let mut re_exports = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = (i + 1) as i64;

        // `from .module import *`
        // `from .module import foo, bar`
        if trimmed.starts_with("from ") && trimmed.contains(" import ") {
            let import_idx = trimmed.find(" import ").unwrap();
            let source = trimmed[5..import_idx].trim().to_string();
            let imported = &trimmed[import_idx + 8..].trim_end_matches(';');
            let symbols = if imported.trim() == "*" {
                None
            } else {
                Some(
                    imported
                        .split(',')
                        .filter_map(|s| {
                            let name = s.trim();
                            if !name.is_empty() {
                                Some(name.to_string())
                            } else {
                                None
                            }
                        })
                        .collect(),
                )
            };
            re_exports.push(ReExportEntry {
                exported_symbols: symbols,
                source_path: source,
                start_line: line_num,
            });
        }
    }
    re_exports
}

#[tool_handler]
impl ServerHandler for CortexServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder().enable_tools().build(),
        )
        .with_server_info(rmcp::model::Implementation::new("cortex", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            r#"
Cortex is a local code context engine that indexes source code and exposes it via MCP.

Available tools:
- search_symbols: Find code symbols by name with filtering and pagination (returns JSON)
- get_code_context: Retrieve full source code implementation for a symbol (returns JSON)
- list_directory_structure: Explore project directory structure (returns JSON)
- index_project: Index or refresh a project's symbol database (returns JSON)
- get_index_status: Check if a project is indexed and get statistics (returns JSON)
- list_files: List files with optional filtering by extension (returns JSON)
- list_symbol_kinds: Get available symbol types for filtering (returns JSON)
- get_symbol_stats: Get statistics about symbols in a project's index (returns JSON)
- list_document_symbols: List all symbols in a file with hierarchy (returns JSON)
- search_content: Search file contents by regex or text pattern (returns JSON)
- find_references: Find all references to a symbol across the project (returns JSON)
- search_by_keyword: Search symbols by keyword using FTS5 full-text search (returns JSON)
- get_imports: Analyze import dependencies for a file (returns JSON)
- export_report: Export a task report after completing work (saves to .cortex/reports/)
- synthesize_reports: Synthesize past reports to identify patterns and improvements (returns JSON)

Usage pattern:
1. Use index_project to index your codebase (first time or after changes)
2. Use get_index_status to check if a project is indexed
3. Use search_symbols to find symbols by name, or search_by_keyword to find by concept/keyword using FTS5
4. Use get_code_context to read full source code for a symbol
5. Use list_document_symbols to see all symbols in a file with hierarchy
6. Use find_references to find all usages of a symbol across the project
7. Use get_imports to analyze import dependencies (outgoing/incoming) for a file
8. Use search_content to grep for text patterns (TODOs, raw SQL, security patterns)
9. Use list_directory_structure or list_files to explore project structure
10. Use list_symbol_kinds to get available symbol type filters, get_symbol_stats for index statistics
11. After completing a task, use export_report to save a report with findings and suggestions
12. Use synthesize_reports to review past task reports and identify recurring patterns

Note: All tools return structured JSON that can be parsed programmatically.
"#,
        )
    }
}

pub async fn serve(config: Config) -> crate::error::Result<()> {
    let server = CortexServer::new(config);

    let transport = rmcp::transport::stdio();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| crate::error::CortexError::Mcp(e.to_string()))?;

    service
        .waiting()
        .await
        .map_err(|e| crate::error::CortexError::Mcp(e.to_string()))?;

    Ok(())
}
