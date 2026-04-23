use crate::error::{CortexError, Result};
use crate::indexer::db::{self, DbPool};
use std::path::Path;

pub async fn get_code_context(
    pool: &DbPool,
    file_path: Option<&str>,
    symbol_name: &str,
) -> Result<CodeContext> {
    // Find the symbol in the database by name
    let mut symbols = db::search_symbols(pool, symbol_name).await?;

    // If file_path provided, filter and rank matches
    let symbol = if let Some(fp) = file_path {
        let canonical = Path::new(fp).canonicalize().ok();
        let canonical_str = canonical.as_deref().and_then(|p| p.to_str());
        let filename = Path::new(fp).file_name().and_then(|n| n.to_str()).unwrap_or(fp);

        // Rank: exact > ends_with > absolute match > filename
        symbols.sort_by(|a, b| {
            let score = |s: &db::SymbolRow| -> u8 {
                if s.path == fp {
                    4
                } else if s.path.ends_with(fp) {
                    3
                } else if canonical_str.map_or(false, |c| s.absolute_path() == c) {
                    2
                } else if Path::new(&s.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    == Some(filename)
                {
                    1
                } else {
                    0
                }
            };
            score(b).cmp(&score(a))
        });

        // If multiple matches with same score, return error for disambiguation
        let filtered: Vec<_> = symbols
            .into_iter()
            .filter(|s| {
                s.path == fp
                    || s.path.ends_with(fp)
                    || canonical_str.map_or(false, |c| s.absolute_path() == c)
                    || Path::new(&s.path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        == Some(filename)
            })
            .collect();

        if filtered.len() > 1 {
            return Err(CortexError::SymbolNotFound(format!(
                "Multiple symbols named '{}' found in file '{}'. Use a more specific file path.",
                symbol_name, fp
            )));
        }

        filtered
            .into_iter()
            .next()
            .ok_or_else(|| CortexError::SymbolNotFound(format!("{} in {}", symbol_name, fp)))?
    } else {
        // No file_path — check for ambiguity
        if symbols.len() > 1 {
            let symbol_list: String = symbols
                .iter()
                .take(5)
                .map(|s| format!("  {} ({})", s.name, s.path))
                .collect::<Vec<_>>()
                .join("\n");

            return Err(CortexError::SymbolNotFound(format!(
                "Multiple symbols named '{}' found. Please specify a file path:\n{}\n{} more matches...",
                symbol_name,
                symbol_list,
                if symbols.len() > 5 {
                    symbols.len() - 5
                } else {
                    0
                }
            )));
        }

        symbols
            .into_iter()
            .next()
            .ok_or_else(|| CortexError::SymbolNotFound(symbol_name.to_string()))?
    };

    // Read the file using the absolute path
    let abs_path = symbol.absolute_path();
    let content = std::fs::read_to_string(Path::new(&abs_path))
        .map_err(|_| CortexError::FileNotFound(abs_path.clone()))?;

    let lines: Vec<&str> = content.lines().collect();
    let start = (symbol.start_line as usize).saturating_sub(1);
    let end = (symbol.end_line as usize).min(lines.len());

    let code: String = (start..end)
        .map(|i| lines[i].to_string())
        .collect::<Vec<String>>()
        .join("\n");

    Ok(CodeContext {
        symbol_name: symbol.name,
        kind: symbol.kind,
        file_path: symbol.path,
        start_line: symbol.start_line,
        end_line: symbol.end_line,
        signature: symbol.signature,
        code,
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
