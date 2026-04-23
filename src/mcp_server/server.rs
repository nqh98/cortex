use crate::config::Config;
use crate::indexer::{db, Indexer};
use crate::mcp_server::models::{
    CodeContextResult, DirectoryListing, IndexResult, IndexStatus, SearchResult,
    SymbolMatch, SymbolStats,
};
use crate::query::{context, search};
use crate::scanner::walker;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct CortexServer {
    config: Arc<Config>,
}

impl CortexServer {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    async fn get_pool(&self) -> crate::error::Result<db::DbPool> {
        db::init_pool(&format!("sqlite:{}", self.config.database.path)).await
    }
}

/// Search symbols request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSymbolsRequest {
    /// Search query (symbol name pattern, minimum 1 character)
    #[schemars(description = "Search query (symbol name pattern, minimum 1 character)")]
    pub query: String,
    /// Filter by symbol kind
    #[schemars(description = "Filter by symbol kind")]
    pub kind: Option<crate::models::SymbolKind>,
    /// Maximum number of results (default: 50)
    #[schemars(description = "Maximum number of results to return (default: 50, max: 100)")]
    pub limit: Option<u32>,
    /// Offset for pagination (default: 0)
    #[schemars(description = "Offset for pagination (default: 0)")]
    pub offset: Option<u32>,
}

/// Get code context request parameters
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCodeContextRequest {
    /// Name of the symbol to retrieve
    #[schemars(description = "Name of the symbol to retrieve")]
    pub symbol_name: String,
    /// Relative path to disambiguate when multiple symbols have the same name
    #[schemars(description = "Relative path to the source file (e.g. src/parser/mod.rs). Use when multiple symbols have the same name to disambiguate.")]
    pub file_path: Option<String>,
    /// Include surrounding context (lines before/after the symbol)
    #[schemars(description = "Number of context lines to include before and after the symbol (default: 0)")]
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

#[tool(tool_box)]
impl CortexServer {
    /// Search for code symbols by name pattern matching.
    ///
    /// Usage examples:
    /// - Search for "parser" → finds all symbols containing "parser"
    /// - Filter by kind: {"kind": "struct"} to only find structs
    /// - Use pagination: {"limit": 10, "offset": 20} to browse results
    ///
    /// Returns structured symbol metadata with file locations.
    #[tool(description = "Search for code symbols by name pattern matching. Supports filtering by kind and pagination. Returns structured JSON with symbol metadata and file locations.")]
    async fn search_symbols(
        &self,
        #[tool(aggr)] request: SearchSymbolsRequest,
    ) -> String {
        if request.query.trim().is_empty() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Query cannot be empty"
                }
            }).to_string();
        }

        let pool = match self.get_pool().await {
            Ok(p) => p,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to connect to database: {e}")
                }
            }).to_string(),
        };

        let limit = request.limit.unwrap_or(50).min(100) as usize;
        let offset = request.offset.unwrap_or(0) as usize;
        let kind_filter = request.kind.map(|k| k.as_str());

        let results = match search::search_symbols_paginated(
            &pool,
            &request.query,
            kind_filter.as_deref(),
            limit,
            offset,
        ).await {
            Ok(r) => r,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Search failed: {e}")
                }
            }).to_string(),
        };

        let total_count = match search::count_symbols(&pool, &request.query, kind_filter.as_deref()).await {
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
            })
            .collect();

        serde_json::to_string(&SearchResult {
            symbols,
            total_count,
            has_more,
        }).unwrap_or_else(|e| serde_json::json!({
            "error": {
                "code": "serialization_error",
                "message": format!("Failed to serialize results: {e}")
            }
        }).to_string())
    }

    /// Get the source code for a symbol by name.
    ///
    /// Use file_path to disambiguate when multiple symbols have the same name.
    /// Returns the full implementation with optional context lines.
    #[tool(description = "Get the source code for a symbol by name. Use file_path to disambiguate when multiple symbols have the same name. Returns structured JSON with full implementation and optional context lines.")]
    async fn get_code_context(
        &self,
        #[tool(aggr)] request: GetCodeContextRequest,
    ) -> String {
        if request.symbol_name.trim().is_empty() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_parameters",
                    "message": "Symbol name cannot be empty"
                }
            }).to_string();
        }

        let pool = match self.get_pool().await {
            Ok(p) => p,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to connect to database: {e}")
                }
            }).to_string(),
        };

        let ctx = match context::get_code_context(
            &pool,
            request.file_path.as_deref(),
            &request.symbol_name,
        ).await {
            Ok(c) => c,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": match &e {
                        crate::error::CortexError::SymbolNotFound(_) => "symbol_not_found",
                        crate::error::CortexError::FileNotFound(_) => "file_not_found",
                        _ => "query_error"
                    },
                    "message": e.to_string()
                }
            }).to_string(),
        };

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
            context_before: if context_before.is_empty() { None } else { Some(context_before) },
            context_after: if context_after.is_empty() { None } else { Some(context_after) },
        }).unwrap_or_else(|e| serde_json::json!({
            "error": {
                "code": "serialization_error",
                "message": format!("Failed to serialize context: {e}")
            }
        }).to_string())
    }

    /// List the directory structure of a project.
    ///
    /// Returns a structured list of files and directories with metadata.
    #[tool(description = "List the directory structure of a project. Returns structured JSON list of files and directories with metadata including file types and languages.")]
    async fn list_directory_structure(
        &self,
        #[tool(aggr)] request: ListDirectoryRequest,
    ) -> String {
        let path = Path::new(&request.path);
        if !path.exists() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_path",
                    "message": format!("Directory not found: {}", request.path)
                }
            }).to_string();
        }

        let max_depth = Some(request.max_depth.unwrap_or(3));
        let extension_filter = request.extension.as_deref();

        let (entries, root_name) = match walker::directory_tree_structured(path, max_depth, extension_filter) {
            Ok(r) => r,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "io_error",
                    "message": format!("Failed to list directory: {e}")
                }
            }).to_string(),
        };

        let file_count = entries.iter().filter(|e| e.entry_type == crate::mcp_server::models::FileType::File).count();
        let directory_count = entries.iter().filter(|e| e.entry_type == crate::mcp_server::models::FileType::Directory).count();

        serde_json::to_string(&DirectoryListing {
            root: root_name,
            entries,
            file_count,
            directory_count,
        }).unwrap_or_else(|e| serde_json::json!({
            "error": {
                "code": "serialization_error",
                "message": format!("Failed to serialize listing: {e}")
            }
        }).to_string())
    }

    /// Index or re-index a project directory.
    ///
    /// Call this after code changes to refresh the symbol index before searching.
    #[tool(description = "Index or re-index a project directory. Call this after code changes to refresh the symbol index before searching. Returns structured JSON with indexing statistics.")]
    async fn index_project(
        &self,
        #[tool(aggr)] request: IndexProjectRequest,
    ) -> String {
        let path = Path::new(&request.path);
        if !path.exists() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_path",
                    "message": format!("Directory not found: {}", request.path)
                }
            }).to_string();
        }

        let indexer = match Indexer::new(&self.config).await {
            Ok(i) => i,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "indexing_failed",
                    "message": format!("Failed to create indexer: {e}")
                }
            }).to_string(),
        };

        let start = Instant::now();
        let stats = match indexer.index_project(path).await {
            Ok(s) => s,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "indexing_failed",
                    "message": format!("Indexing failed: {e}")
                }
            }).to_string(),
        };
        let duration_ms = start.elapsed().as_millis() as u64;

        let project_root = path.canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| request.path.clone());

        serde_json::to_string(&IndexResult {
            files_indexed: stats.files_indexed as u32,
            files_unchanged: stats.files_unchanged as u32,
            files_failed: stats.files_failed as u32,
            symbols_found: stats.symbols_found as u32,
            duration_ms,
            project_root,
        }).unwrap_or_else(|e| serde_json::json!({
            "error": {
                "code": "serialization_error",
                "message": format!("Failed to serialize index result: {e}")
            }
        }).to_string())
    }

    /// Check if a project is indexed and get its status.
    #[tool(description = "Check if a project is indexed and get its status including file count, symbol count, and last indexed time. Returns structured JSON with project metadata.")]
    async fn get_index_status(
        &self,
        #[tool(aggr)] request: GetIndexStatusRequest,
    ) -> String {
        let path = Path::new(&request.path);
        let project_root = path.canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| request.path.clone());

        let pool = match self.get_pool().await {
            Ok(p) => p,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to connect to database: {e}")
                }
            }).to_string(),
        };

        let (file_count, symbol_count, last_indexed) = match db::get_project_stats(&pool, &project_root).await {
            Ok(s) => s,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to get project stats: {e}")
                }
            }).to_string(),
        };

        let languages = match db::get_project_languages(&pool, &project_root).await {
            Ok(l) => l,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to get project languages: {e}")
                }
            }).to_string(),
        };

        let is_indexed = file_count > 0;

        serde_json::to_string(&IndexStatus {
            is_indexed,
            file_count,
            symbol_count,
            last_indexed_at: last_indexed,
            project_root,
            languages,
        }).unwrap_or_else(|e| serde_json::json!({
            "error": {
                "code": "serialization_error",
                "message": format!("Failed to serialize status: {e}")
            }
        }).to_string())
    }

    /// List files in a project with optional filtering.
    #[tool(description = "List files in a project with optional filtering by extension. Returns structured JSON list of file entries with metadata.")]
    async fn list_files(
        &self,
        #[tool(aggr)] request: ListFilesRequest,
    ) -> String {
        let path = Path::new(&request.path);
        if !path.exists() {
            return serde_json::json!({
                "error": {
                    "code": "invalid_path",
                    "message": format!("Directory not found: {}", request.path)
                }
            }).to_string();
        }

        let include_directories = request.include_directories.unwrap_or(false);
        let extension_filter = request.extension.as_deref();
        let limit = request.limit.unwrap_or(100);

        let entries = match walker::list_files_structured(
            path,
            extension_filter,
            include_directories,
            limit,
        ) {
            Ok(e) => e,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "io_error",
                    "message": format!("Failed to list files: {e}")
                }
            }).to_string(),
        };

        serde_json::to_string(&entries).unwrap_or_else(|e| serde_json::json!({
            "error": {
                "code": "serialization_error",
                "message": format!("Failed to serialize files: {e}")
            }
        }).to_string())
    }

    /// Get all available symbol kinds that can be used as filters.
    #[tool(description = "Get all available symbol kinds (function, struct, class, etc.) that can be used as filters in search. Returns JSON array of symbol kinds.")]
    async fn list_symbol_kinds(&self) -> String {
        serde_json::to_string(&vec![
            crate::models::SymbolKind::Function,
            crate::models::SymbolKind::Struct,
            crate::models::SymbolKind::Impl,
            crate::models::SymbolKind::Trait,
            crate::models::SymbolKind::Enum,
            crate::models::SymbolKind::Constant,
            crate::models::SymbolKind::Module,
            crate::models::SymbolKind::Class,
            crate::models::SymbolKind::Method,
        ]).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get statistics about symbols in the index.
    #[tool(description = "Get statistics about symbols in the index including total count and breakdown by kind and language. Returns structured JSON with symbol statistics.")]
    async fn get_symbol_stats(&self) -> String {
        let pool = match self.get_pool().await {
            Ok(p) => p,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to connect to database: {e}")
                }
            }).to_string(),
        };

        let total_symbols = match db::get_total_symbol_count(&pool).await {
            Ok(c) => c,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to get total symbol count: {e}")
                }
            }).to_string(),
        };

        let by_kind = match db::get_symbols_by_kind(&pool).await {
            Ok(b) => b,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to get symbols by kind: {e}")
                }
            }).to_string(),
        };

        let by_language = match db::get_symbols_by_language(&pool).await {
            Ok(b) => b,
            Err(e) => return serde_json::json!({
                "error": {
                    "code": "database_error",
                    "message": format!("Failed to get symbols by language: {e}")
                }
            }).to_string(),
        };

        serde_json::to_string(&SymbolStats {
            total_symbols,
            by_kind,
            by_language,
        }).unwrap_or_else(|e| serde_json::json!({
            "error": {
                "code": "serialization_error",
                "message": format!("Failed to serialize stats: {e}")
            }
        }).to_string())
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
        let preview: Vec<String> = lines.iter().take(10).enumerate()
            .map(|(i, line)| format!("{:>4} | {}", ctx.start_line as usize + i, line))
            .collect();
        let preview = preview.join("\n");

        // Context lines
        let (before, after) = if context_lines > 0 {
            let start_idx = (ctx.start_line as usize).saturating_sub(1);
            let end_idx = (ctx.end_line as usize).min(lines.len());

            let before_lines: Vec<String> = lines.iter().take(start_idx)
                .skip(start_idx.saturating_sub(context_lines))
                .enumerate()
                .map(|(i, line)| format!("{:>4} | {}", start_idx - context_lines + i + 1, line))
                .collect();

            let after_lines: Vec<String> = lines.iter().skip(end_idx).take(context_lines)
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
        "enum" => crate::models::SymbolKind::Enum,
        "constant" => crate::models::SymbolKind::Constant,
        "module" => crate::models::SymbolKind::Module,
        "class" => crate::models::SymbolKind::Class,
        "method" => crate::models::SymbolKind::Method,
        _ => crate::models::SymbolKind::Function, // Default fallback
    }
}

#[tool(tool_box)]
impl ServerHandler for CortexServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "cortex".into(),
                version: "0.2.0".into(),
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
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
- get_symbol_stats: Get overall statistics about the index (returns JSON)

Usage pattern:
1. Use index_project to index your codebase (first time or after changes)
2. Use search_symbols to find relevant symbols
3. Use get_code_context to read full implementations
4. Use list_directory_structure or list_files to explore project structure

Note: All tools return structured JSON that can be parsed programmatically.
"#
                .into(),
            ),
            ..Default::default()
        }
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
