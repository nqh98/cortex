use crate::config::Config;
use crate::indexer::db;
use crate::query::{context, search};
use crate::scanner::walker;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSymbolsRequest {
    #[schemars(description = "Search query (symbol name pattern)")]
    pub query: String,
    #[schemars(description = "Optional filter by kind (function, struct, impl, trait, enum, constant, module, class, method)")]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCodeContextRequest {
    #[schemars(description = "Path to the source file")]
    pub file_path: String,
    #[schemars(description = "Name of the symbol to retrieve")]
    pub symbol_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDirectoryRequest {
    #[schemars(description = "Path to the project directory")]
    pub path: String,
    #[schemars(description = "Maximum depth of the tree (default: 3)")]
    pub max_depth: Option<usize>,
}

#[tool(tool_box)]
impl CortexServer {
    #[tool(description = "Search for code symbols by name. Returns matching functions, structs, classes, etc. with file locations.")]
    async fn search_symbols(
        &self,
        #[tool(aggr)] request: SearchSymbolsRequest,
    ) -> String {
        let pool = match self.get_pool().await {
            Ok(p) => p,
            Err(e) => return format!("Error connecting to database: {e}"),
        };

        match search::search_symbols(&pool, &request.query, request.kind.as_deref()).await {
            Ok(results) => {
                if results.is_empty() {
                    format!("No symbols matching '{}'", request.query)
                } else {
                    let mut lines = vec![format!("Found {} symbols:", results.len())];
                    for row in &results {
                        let sig = row.signature.as_deref().unwrap_or("");
                        lines.push(format!(
                            "  {} {} ({}:{}-{}) {}",
                            row.kind, row.name, row.path, row.start_line, row.end_line, sig
                        ));
                    }
                    lines.join("\n")
                }
            }
            Err(e) => format!("Search error: {e}"),
        }
    }

    #[tool(description = "Get the source code for a specific symbol. Returns the full implementation with line numbers.")]
    async fn get_code_context(
        &self,
        #[tool(aggr)] request: GetCodeContextRequest,
    ) -> String {
        let pool = match self.get_pool().await {
            Ok(p) => p,
            Err(e) => return format!("Error connecting to database: {e}"),
        };

        match context::get_code_context(&pool, &request.file_path, &request.symbol_name).await {
            Ok(ctx) => {
                let mut result = format!("--- {} ({}) ---\n", ctx.symbol_name, ctx.kind);
                result.push_str(&format!(
                    "File: {} lines {}-{}\n",
                    ctx.file_path, ctx.start_line, ctx.end_line
                ));
                if let Some(sig) = &ctx.signature {
                    result.push_str(&format!("Signature: {sig}\n"));
                }
                result.push('\n');
                result.push_str(&ctx.code);
                result
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(description = "List the directory structure of a project. Returns a tree view showing files and folders.")]
    async fn list_directory_structure(
        &self,
        #[tool(aggr)] request: ListDirectoryRequest,
    ) -> String {
        let path = Path::new(&request.path);
        if !path.exists() {
            return format!("Directory not found: {}", request.path);
        }

        match walker::directory_tree(path, request.max_depth.or(Some(3))) {
            Ok(tree) => tree,
            Err(e) => format!("Error listing directory: {e}"),
        }
    }
}

#[tool(tool_box)]
impl ServerHandler for CortexServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "cortex".into(),
                version: "0.1.0".into(),
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Cortex is a local code context engine. Use search_symbols to find code, get_code_context to read implementations, and list_directory_structure to explore projects.".into(),
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
