use crate::error::Result;
use crate::indexer::db::{DbPool, SymbolRow};

pub async fn search_by_semantic(
    pool: &DbPool,
    query: &str,
    project_root: &str,
    limit: usize,
) -> Result<Vec<SymbolRow>> {
    // Tokenize: split on whitespace, join with OR for FTS5
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let fts_query = tokens.join(" OR ");

    let rows = sqlx::query_as::<_, SymbolRow>(
        "SELECT s.id, f.project_root, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
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

pub async fn count_semantic_results(
    pool: &DbPool,
    query: &str,
    project_root: &str,
) -> Result<i64> {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return Ok(0);
    }

    let fts_query = tokens.join(" OR ");

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
        "INSERT INTO symbol_search(rowid, name, signature, documentation)
         SELECT id, name, signature, documentation FROM symbols",
    )
    .execute(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(())
}
