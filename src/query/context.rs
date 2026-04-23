use crate::error::{CortexError, Result};
use crate::indexer::db::{self, DbPool};
use std::path::Path;

pub async fn get_code_context(
    pool: &DbPool,
    file_path: &str,
    symbol_name: &str,
) -> Result<CodeContext> {
    // Canonicalize the input path for matching
    let canonical = Path::new(file_path).canonicalize().ok();
    let canonical_str = canonical.as_deref().and_then(|p| p.to_str());

    // Find the symbol in the database
    let symbols = db::search_symbols(pool, symbol_name)
        .await?
        .into_iter()
        .filter(|s| {
            s.path == file_path
                || canonical_str.map_or(false, |c| s.path == c)
                || s.path.ends_with(file_path)
        })
        .collect::<Vec<_>>();

    let symbol = symbols
        .into_iter()
        .next()
        .ok_or_else(|| CortexError::SymbolNotFound(format!("{} in {}", symbol_name, file_path)))?;

    // Read the file
    let content = std::fs::read_to_string(Path::new(&symbol.path))
        .map_err(|_| CortexError::FileNotFound(symbol.path.clone()))?;

    let lines: Vec<&str> = content.lines().collect();
    let start = (symbol.start_line as usize).saturating_sub(1);
    let end = (symbol.end_line as usize).min(lines.len());

    let code_block: Vec<String> = (start..end)
        .map(|i| format!("{:>4} | {}", i + 1, lines[i]))
        .collect();

    Ok(CodeContext {
        symbol_name: symbol.name,
        kind: symbol.kind,
        file_path: symbol.path,
        start_line: symbol.start_line,
        end_line: symbol.end_line,
        signature: symbol.signature,
        code: code_block.join("\n"),
    })
}

#[derive(Debug)]
pub struct CodeContext {
    pub symbol_name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: Option<String>,
    pub code: String,
}
