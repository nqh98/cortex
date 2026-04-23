use crate::indexer::db::{DbPool, SymbolRow};

pub async fn search_symbols(
    pool: &DbPool,
    query: &str,
    kind_filter: Option<&str>,
) -> crate::error::Result<Vec<SymbolRow>> {
    let pattern = format!("%{query}%");

    let rows = if let Some(kind) = kind_filter {
        sqlx::query_as::<_, SymbolRow>(
            "SELECT s.id, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
             FROM symbols s JOIN files f ON s.file_id = f.id
             WHERE s.name LIKE ?1 AND s.kind = ?2
             ORDER BY s.name
             LIMIT 50"
        )
        .bind(&pattern)
        .bind(kind)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, SymbolRow>(
            "SELECT s.id, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
             FROM symbols s JOIN files f ON s.file_id = f.id
             WHERE s.name LIKE ?1
             ORDER BY s.name
             LIMIT 50"
        )
        .bind(&pattern)
        .fetch_all(pool)
        .await
    };

    rows.map_err(|e| crate::error::CortexError::Database(e.to_string()))
}

pub async fn search_by_kind(
    pool: &DbPool,
    kind: &str,
) -> crate::error::Result<Vec<SymbolRow>> {
    sqlx::query_as::<_, SymbolRow>(
        "SELECT s.id, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
         FROM symbols s JOIN files f ON s.file_id = f.id
         WHERE s.kind = ?1
         ORDER BY s.name
         LIMIT 100"
    )
    .bind(kind)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))
}
