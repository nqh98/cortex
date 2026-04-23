use sqlx::sqlite::SqlitePoolOptions;

pub type DbPool = sqlx::sqlite::SqlitePool;

pub async fn init_pool(db_path: &str) -> crate::error::Result<DbPool> {
    // Ensure parent directory exists
    let file_path = db_path.trim_start_matches("sqlite:");
    if let Some(parent) = std::path::Path::new(file_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let connection_string = if db_path.starts_with("sqlite:") {
        format!("{}?mode=rwc", db_path)
    } else {
        format!("sqlite:{}?mode=rwc", db_path)
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    run_migrations(&pool).await?;
    Ok(pool)
}

async fn run_migrations(pool: &DbPool) -> crate::error::Result<()> {
    let schema = include_str!("../../migrations/001_init.sql");
    sqlx::raw_sql(schema)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;
    Ok(())
}

pub async fn upsert_file(
    pool: &DbPool,
    path: &str,
    hash: &str,
    language: &str,
) -> crate::error::Result<i64> {
    let result = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO files (path, hash, language) VALUES (?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET hash = ?, language = ?, last_indexed = CURRENT_TIMESTAMP
         RETURNING id"
    )
    .bind(path)
    .bind(hash)
    .bind(language)
    .bind(hash)
    .bind(language)
    .fetch_one(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(result.0)
}

pub async fn insert_symbols(
    pool: &DbPool,
    file_id: i64,
    symbols: &[crate::models::Symbol],
) -> crate::error::Result<()> {
    // Delete existing symbols for this file first
    sqlx::query("DELETE FROM symbols WHERE file_id = ?")
        .bind(file_id)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    for symbol in symbols {
        sqlx::query(
            "INSERT INTO symbols (file_id, name, kind, start_line, end_line, start_col, end_col, signature, documentation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(file_id)
        .bind(&symbol.name)
        .bind(symbol.kind.as_str())
        .bind(symbol.start_line as i64)
        .bind(symbol.end_line as i64)
        .bind(symbol.start_col as i64)
        .bind(symbol.end_col as i64)
        .bind(&symbol.signature)
        .bind(&symbol.documentation)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;
    }

    Ok(())
}

pub async fn search_symbols(
    pool: &DbPool,
    query: &str,
) -> crate::error::Result<Vec<SymbolRow>> {
    let pattern = format!("%{query}%");
    let rows = sqlx::query_as::<_, SymbolRow>(
        "SELECT s.id, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
         FROM symbols s JOIN files f ON s.file_id = f.id
         WHERE s.name LIKE ?1
         ORDER BY s.name
         LIMIT 50"
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, sqlx::FromRow)]
pub struct SymbolRow {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: Option<String>,
}

pub async fn get_file_hash(pool: &DbPool, path: &str) -> crate::error::Result<Option<String>> {
    let result = sqlx::query_as::<_, (String,)>(
        "SELECT hash FROM files WHERE path = ?"
    )
    .bind(path)
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(result.map(|r| r.0))
}
