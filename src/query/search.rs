use crate::indexer::db::{DbPool, SymbolRow};

pub async fn search_symbols(
    pool: &DbPool,
    query: &str,
    kind_filter: Option<&str>,
) -> crate::error::Result<Vec<SymbolRow>> {
    let pattern = format!("%{query}%");

    let rows = if let Some(kind) = kind_filter {
        sqlx::query_as::<_, SymbolRow>(
            "SELECT s.id, f.project_root, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
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
            "SELECT s.id, f.project_root, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
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

/// Paginated search for symbols
pub async fn search_symbols_paginated(
    pool: &DbPool,
    query: &str,
    kind_filter: Option<&str>,
    limit: usize,
    offset: usize,
) -> crate::error::Result<Vec<SymbolRow>> {
    let pattern = format!("%{query}%");

    let rows = if let Some(kind) = kind_filter {
        sqlx::query_as::<_, SymbolRow>(
            "SELECT s.id, f.project_root, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
             FROM symbols s JOIN files f ON s.file_id = f.id
             WHERE s.name LIKE ?1 AND s.kind = ?2
             ORDER BY s.name
             LIMIT ?3 OFFSET ?4"
        )
        .bind(&pattern)
        .bind(kind)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, SymbolRow>(
            "SELECT s.id, f.project_root, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
             FROM symbols s JOIN files f ON s.file_id = f.id
             WHERE s.name LIKE ?1
             ORDER BY s.name
             LIMIT ?2 OFFSET ?3"
        )
        .bind(&pattern)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await
    };

    rows.map_err(|e| crate::error::CortexError::Database(e.to_string()))
}

/// Count total matching symbols
pub async fn count_symbols(
    pool: &DbPool,
    query: &str,
    kind_filter: Option<&str>,
) -> crate::error::Result<i64> {
    let pattern = format!("%{query}%");

    let count = if let Some(kind) = kind_filter {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)
             FROM symbols s JOIN files f ON s.file_id = f.id
             WHERE s.name LIKE ?1 AND s.kind = ?2",
        )
        .bind(&pattern)
        .bind(kind)
        .fetch_one(pool)
        .await
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)
             FROM symbols s JOIN files f ON s.file_id = f.id
             WHERE s.name LIKE ?1",
        )
        .bind(&pattern)
        .fetch_one(pool)
        .await
    };

    count.map_err(|e| crate::error::CortexError::Database(e.to_string()))
}

pub async fn search_by_kind(pool: &DbPool, kind: &str) -> crate::error::Result<Vec<SymbolRow>> {
    sqlx::query_as::<_, SymbolRow>(
        "SELECT s.id, f.project_root, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
         FROM symbols s JOIN files f ON s.file_id = f.id
         WHERE s.kind = ?1
         ORDER BY s.name
         LIMIT 100",
    )
    .bind(kind)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))
}
