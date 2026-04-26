use crate::error::{CortexError, Result};
use crate::indexer::db::{self, DbPool};
use std::path::Path;

/// Look up a symbol in the database and return its row (async, DB-only).
pub async fn lookup_symbol(
    pool: &DbPool,
    file_path: Option<&str>,
    symbol_name: &str,
    kind_filter: Option<&str>,
) -> Result<db::SymbolRow> {
    // Find the symbol in the database by name
    let mut symbols = db::search_symbols(pool, symbol_name).await?;

    // Filter by kind if specified
    if let Some(kind) = kind_filter {
        symbols.retain(|s| s.kind == kind);
    }

    // If file_path provided, filter and rank matches
    if let Some(fp) = file_path {
        let canonical = Path::new(fp).canonicalize().ok();
        let canonical_str = canonical.as_deref().and_then(|p| p.to_str());
        let filename = Path::new(fp)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(fp);

        // Rank: exact > ends_with > absolute match > filename
        // Secondary: prefer non-barrel definitions over re-export files
        symbols.sort_by(|a, b| {
            let score = |s: &db::SymbolRow| -> (u8, bool) {
                let path_score = if s.path == fp {
                    4
                } else if s.path.ends_with(fp) {
                    3
                } else if canonical_str.is_some_and(|c| s.absolute_path() == c) {
                    2
                } else if Path::new(&s.path).file_name().and_then(|n| n.to_str()) == Some(filename)
                {
                    1
                } else {
                    0
                };
                let is_barrel = is_barrel_file(&s.path);
                (path_score, is_barrel)
            };
            let sa = score(a);
            let sb = score(b);
            // Higher path_score first, then prefer non-barrel (false < true in reverse)
            sa.cmp(&sb).reverse()
        });

        let mut filtered: Vec<_> = symbols
            .into_iter()
            .filter(|s| {
                s.path == fp
                    || s.path.ends_with(fp)
                    || canonical_str.is_some_and(|c| s.absolute_path() == c)
                    || Path::new(&s.path).file_name().and_then(|n| n.to_str()) == Some(filename)
            })
            .collect();

        if filtered.len() > 1 {
            // Prefer non-barrel definitions
            let non_barrel: Vec<_> = filtered
                .iter()
                .filter(|s| !is_barrel_file(&s.path))
                .cloned()
                .collect();
            if !non_barrel.is_empty() {
                filtered = non_barrel;
            }

            // If still ambiguous, try exact name match (db::search_symbols uses LIKE)
            if filtered.len() > 1 {
                let exact_name: Vec<_> = filtered
                    .iter()
                    .filter(|s| s.name == symbol_name)
                    .cloned()
                    .collect();
                if !exact_name.is_empty() {
                    return Ok(exact_name.into_iter().next().unwrap());
                }

                return Err(CortexError::SymbolNotFound(format!(
                    "Multiple symbols named '{}' found in file '{}'. Use 'kind' to disambiguate.",
                    symbol_name, fp
                )));
            }
        }

        filtered
            .into_iter()
            .next()
            .ok_or_else(|| CortexError::SymbolNotFound(format!("{} in {}", symbol_name, fp)))
    } else {
        // No file_path — filter by exact name first to narrow
        let exact: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == symbol_name)
            .cloned()
            .collect();
        let candidates = if exact.len() == 1 {
            return Ok(exact.into_iter().next().unwrap());
        } else if !exact.is_empty() {
            exact
        } else {
            symbols
        };

        if candidates.len() > 1 {
            let symbol_list: String = candidates
                .iter()
                .take(5)
                .map(|s| format!("  {} ({}) [{}]", s.name, s.path, s.kind))
                .collect::<Vec<_>>()
                .join("\n");

            let remaining = if candidates.len() > 5 {
                candidates.len() - 5
            } else {
                0
            };

            return Err(CortexError::SymbolNotFound(format!(
                "Multiple symbols named '{}' found. Please specify a file path or kind:\n{}\n{} more matches...",
                symbol_name,
                symbol_list,
                remaining
            )));
        }

        candidates
            .into_iter()
            .next()
            .ok_or_else(|| CortexError::SymbolNotFound(symbol_name.to_string()))
    }
}

/// Detect barrel/re-export files by naming convention.
fn is_barrel_file(path: &str) -> bool {
    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    matches!(
        filename,
        "index.ts" | "index.tsx" | "index.js" | "index.jsx" | "mod.rs" | "__init__.py"
    )
}

/// Extract code from file content using symbol line numbers (sync, no DB).
pub fn extract_code(symbol: &db::SymbolRow, content: &str) -> CodeContext {
    let lines: Vec<&str> = content.lines().collect();
    let start = (symbol.start_line as usize).saturating_sub(1);
    let end = (symbol.end_line as usize).min(lines.len());

    let code: String = (start..end)
        .map(|i| lines[i].to_string())
        .collect::<Vec<String>>()
        .join("\n");

    CodeContext {
        symbol_name: symbol.name.clone(),
        kind: symbol.kind.clone(),
        file_path: symbol.path.clone(),
        start_line: symbol.start_line,
        end_line: symbol.end_line,
        signature: symbol.signature.clone(),
        code,
    }
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
