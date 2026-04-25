use crate::error::Result;
use crate::indexer::db::{DbPool, SymbolRow};

pub async fn search_by_keyword(
    pool: &DbPool,
    query: &str,
    project_root: &str,
    limit: usize,
) -> Result<Vec<SymbolRow>> {
    // Tokenize: split on whitespace, lowercase
    let tokens: Vec<&str> = query.split_whitespace().filter(|t| !t.is_empty()).collect();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    // Use AND for multi-word queries (all terms must match somewhere),
    // with each token matching as a prefix via * for partial matches.
    // Single-word queries also use prefix matching.
    let fts_query: String = tokens
        .iter()
        .map(|t| format!("{t}*"))
        .collect::<Vec<_>>()
        .join(" AND ");

    let rows = sqlx::query_as::<_, SymbolRow>(
        "SELECT s.id, f.project_root, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature, f.language
         FROM symbol_search ss
         JOIN symbols s ON ss.rowid = s.id
         JOIN files f ON s.file_id = f.id
         WHERE symbol_search MATCH ?1 AND f.project_root = ?2
         ORDER BY rank
         LIMIT ?3",
    )
    .bind(&fts_query)
    .bind(project_root)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(rows)
}

pub async fn count_keyword_results(pool: &DbPool, query: &str, project_root: &str) -> Result<i64> {
    let tokens: Vec<&str> = query.split_whitespace().filter(|t| !t.is_empty()).collect();
    if tokens.is_empty() {
        return Ok(0);
    }

    let fts_query: String = tokens
        .iter()
        .map(|t| format!("{t}*"))
        .collect::<Vec<_>>()
        .join(" AND ");

    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM symbol_search ss
         JOIN symbols s ON ss.rowid = s.id
         JOIN files f ON s.file_id = f.id
         WHERE symbol_search MATCH ?1 AND f.project_root = ?2",
    )
    .bind(&fts_query)
    .bind(project_root)
    .fetch_one(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(count)
}

/// Rebuild the FTS index from existing symbols data.
pub async fn rebuild_fts_index(pool: &DbPool) -> Result<()> {
    // Clear and rebuild
    sqlx::raw_sql("DELETE FROM symbol_search")
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    sqlx::raw_sql(
        "INSERT INTO symbol_search(rowid, name_tokens, signature, documentation, file_path_tokens)
         SELECT s.id, s.name_tokens, s.signature, s.documentation,
                REPLACE(REPLACE(REPLACE(f.path, '/', ' '), '.', ' '), '-', ' ')
         FROM symbols s JOIN files f ON s.file_id = f.id",
    )
    .execute(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(())
}
