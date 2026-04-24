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

    // Migrate old schema: add project_root column if missing
    let has_project_root: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('files') WHERE name = 'project_root'"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if !has_project_root {
        sqlx::raw_sql("DROP TABLE IF EXISTS symbols").execute(pool).await
            .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;
        sqlx::raw_sql("DROP TABLE IF EXISTS files").execute(pool).await
            .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;
        sqlx::raw_sql(schema).execute(pool).await
            .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;
    }

    // Migrate TS interface/type_alias kinds: rename 'trait' -> 'interface' and 'struct' -> 'type_alias'
    // for TypeScript/JavaScript files only (Rust traits and structs keep their original kinds)
    sqlx::raw_sql(
        "UPDATE symbols SET kind = 'interface' WHERE kind = 'trait' AND file_id IN (SELECT id FROM files WHERE language IN ('typescript', 'javascript'))"
    ).execute(pool).await.map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    sqlx::raw_sql(
        "UPDATE symbols SET kind = 'type_alias' WHERE kind = 'struct' AND file_id IN (SELECT id FROM files WHERE language IN ('typescript', 'javascript')) AND name IN (SELECT s.name FROM symbols s JOIN files f ON s.file_id = f.id WHERE f.language IN ('typescript', 'javascript') AND s.kind = 'struct' AND s.signature LIKE 'type %')"
    ).execute(pool).await.map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    // FTS5 full-text search index
    let migration_fts = include_str!("../../migrations/002_add_fts.sql");
    sqlx::raw_sql(migration_fts)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    // Imports table for dependency analysis
    let migration_imports = include_str!("../../migrations/003_add_imports.sql");
    sqlx::raw_sql(migration_imports)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(())
}

pub async fn upsert_file(
    pool: &DbPool,
    project_root: &str,
    path: &str,
    hash: &str,
    language: &str,
) -> crate::error::Result<i64> {
    let result = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO files (project_root, path, hash, language) VALUES (?, ?, ?, ?)
         ON CONFLICT(project_root, path) DO UPDATE SET hash = ?, language = ?, last_indexed = CURRENT_TIMESTAMP
         RETURNING id"
    )
    .bind(project_root)
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
        "SELECT s.id, f.project_root, f.path, s.name, s.kind, s.start_line, s.end_line, s.signature
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
    pub project_root: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: Option<String>,
}

impl SymbolRow {
    pub fn absolute_path(&self) -> String {
        std::path::Path::new(&self.project_root)
            .join(&self.path)
            .to_string_lossy()
            .to_string()
    }
}

pub async fn get_file_hash(pool: &DbPool, project_root: &str, path: &str) -> crate::error::Result<Option<String>> {
    let result = sqlx::query_as::<_, (String,)>(
        "SELECT hash FROM files WHERE project_root = ? AND path = ?"
    )
    .bind(project_root)
    .bind(path)
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(result.map(|r| r.0))
}

pub async fn delete_project(pool: &DbPool, project_root: &str) -> crate::error::Result<u64> {
    let file_ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM files WHERE project_root = ?"
    )
    .bind(project_root)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    let count = file_ids.len();

    sqlx::query("DELETE FROM symbols WHERE file_id IN (SELECT id FROM files WHERE project_root = ?)")
        .bind(project_root)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    sqlx::query("DELETE FROM files WHERE project_root = ?")
        .bind(project_root)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(count as u64)
}

pub async fn delete_all(pool: &DbPool) -> crate::error::Result<u64> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM files")
        .fetch_one(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    sqlx::raw_sql("DELETE FROM symbols")
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;
    sqlx::raw_sql("DELETE FROM files")
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(count.0 as u64)
}

/// Get statistics for a specific project
pub async fn get_project_stats(
    pool: &DbPool,
    project_root: &str,
) -> crate::error::Result<(u32, u32, Option<String>)> {
    let file_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM files WHERE project_root = ?")
        .bind(project_root)
        .fetch_one(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    let symbol_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM symbols s JOIN files f ON s.file_id = f.id WHERE f.project_root = ?",
    )
    .bind(project_root)
    .fetch_one(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    let last_indexed: Option<(String,)> = sqlx::query_as(
        "SELECT MAX(last_indexed) FROM files WHERE project_root = ?",
    )
    .bind(project_root)
    .fetch_optional(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok((
        file_count.0 as u32,
        symbol_count.0 as u32,
        last_indexed.map(|r| r.0),
    ))
}

/// Get languages used in a project
pub async fn get_project_languages(
    pool: &DbPool,
    project_root: &str,
) -> crate::error::Result<Vec<String>> {
    let languages: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT language FROM files WHERE project_root = ? ORDER BY language",
    )
    .bind(project_root)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(languages.into_iter().map(|l| l.0).collect())
}

/// Get total symbol count across all projects
pub async fn get_total_symbol_count(pool: &DbPool) -> crate::error::Result<u32> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM symbols")
        .fetch_one(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    Ok(count.0 as u32)
}

/// Get symbols grouped by kind
pub async fn get_symbols_by_kind(
    pool: &DbPool,
) -> crate::error::Result<std::collections::HashMap<String, u32>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT kind, COUNT(*) as count FROM symbols GROUP BY kind ORDER BY count DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    let mut result = std::collections::HashMap::new();
    for (kind, count) in rows {
        result.insert(kind, count as u32);
    }

    Ok(result)
}

/// Get symbols grouped by language
pub async fn get_symbols_by_language(
    pool: &DbPool,
) -> crate::error::Result<std::collections::HashMap<String, u32>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT f.language, COUNT(*) as count
         FROM symbols s JOIN files f ON s.file_id = f.id
         GROUP BY f.language ORDER BY count DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    let mut result = std::collections::HashMap::new();
    for (language, count) in rows {
        result.insert(language, count as u32);
    }

    Ok(result)
}

/// Insert imports for a file, replacing any existing ones
pub async fn insert_imports(
    pool: &DbPool,
    file_id: i64,
    imports: &[crate::models::Import],
) -> crate::error::Result<()> {
    sqlx::query("DELETE FROM imports WHERE file_id = ?")
        .bind(file_id)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    for imp in imports {
        sqlx::query(
            "INSERT INTO imports (file_id, imported_symbol, imported_from_path, import_type, start_line, raw_statement)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(file_id)
        .bind(&imp.imported_symbol)
        .bind(&imp.imported_from_path)
        .bind(imp.import_type.as_str())
        .bind(imp.start_line.map(|l| l as i64))
        .bind(&imp.raw_statement)
        .execute(pool)
        .await
        .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;
    }

    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct ImportRow {
    pub id: i64,
    pub file_id: i64,
    pub imported_symbol: String,
    pub imported_from_path: Option<String>,
    pub import_type: String,
    pub start_line: Option<i64>,
    pub raw_statement: Option<String>,
    pub file_path: String,
    pub project_root: String,
}

pub async fn get_outgoing_imports(
    pool: &DbPool,
    project_root: &str,
    file_path: &str,
) -> crate::error::Result<Vec<ImportRow>> {
    sqlx::query_as::<_, ImportRow>(
        "SELECT i.id, i.file_id, i.imported_symbol, i.imported_from_path, i.import_type, i.start_line, i.raw_statement, f.path as file_path, f.project_root
         FROM imports i JOIN files f ON i.file_id = f.id
         WHERE f.project_root = ? AND f.path = ?
         ORDER BY i.start_line",
    )
    .bind(project_root)
    .bind(file_path)
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))
}

pub async fn get_incoming_imports(
    pool: &DbPool,
    project_root: &str,
    file_path: &str,
) -> crate::error::Result<Vec<ImportRow>> {
    let module_name = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_path)
        .to_string();

    sqlx::query_as::<_, ImportRow>(
        "SELECT i.id, i.file_id, i.imported_symbol, i.imported_from_path, i.import_type, i.start_line, i.raw_statement, f.path as file_path, f.project_root
         FROM imports i JOIN files f ON i.file_id = f.id
         WHERE f.project_root = ?
         AND (i.imported_from_path LIKE ? OR i.imported_symbol LIKE ?)
         ORDER BY f.path, i.start_line",
    )
    .bind(project_root)
    .bind(format!("%{module_name}%"))
    .bind(format!("%{module_name}%"))
    .fetch_all(pool)
    .await
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))
}
