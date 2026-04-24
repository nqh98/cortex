use crate::error::Result;
use crate::indexer::db::DbPool;

#[derive(Debug, sqlx::FromRow)]
pub struct DocumentSymbolRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub start_line: i64,
    pub end_line: i64,
    pub start_col: i64,
    pub end_col: i64,
    pub signature: Option<String>,
    pub documentation: Option<String>,
}

pub async fn list_document_symbols(
    pool: &DbPool,
    project_root: &str,
    file_path: &str,
) -> Result<Vec<DocumentSymbolRow>> {
    let rows = sqlx::query_as::<_, DocumentSymbolRow>(
        "SELECT s.id, s.name, s.kind, s.start_line, s.end_line, s.start_col, s.end_col, s.signature, s.documentation
         FROM symbols s JOIN files f ON s.file_id = f.id
         WHERE f.project_root = ? AND f.path = ?
         ORDER BY s.start_line",
    )
    .bind(project_root)
    .bind(file_path)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(rows)
}
