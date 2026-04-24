use crate::error::Result;
use crate::indexer::db::DbPool;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ContentMatch {
    pub file_path: String,
    pub project_root: String,
    pub line_number: usize,
    pub line_content: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct FilePathRow {
    project_root: String,
    path: String,
}

pub async fn search_content(
    pool: &DbPool,
    project_root: &str,
    pattern: &str,
    file_extension: Option<&str>,
    limit: usize,
) -> Result<Vec<ContentMatch>> {
    // Fetch file paths from DB
    let files = if let Some(ext) = file_extension {
        let pattern = format!("%.{ext}");
        sqlx::query_as::<_, FilePathRow>(
            "SELECT f.project_root, f.path FROM files f WHERE f.project_root = ? AND f.path LIKE ?",
        )
        .bind(project_root)
        .bind(&pattern)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, FilePathRow>(
            "SELECT f.project_root, f.path FROM files f WHERE f.project_root = ?",
        )
        .bind(project_root)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| crate::error::CortexError::Database(e.to_string()))?;

    let pattern_owned = pattern.to_string();
    let limit_owned = limit;

    // Run regex matching in a blocking thread
    Ok(tokio::task::spawn_blocking(move || {
        let re = match regex::Regex::new(&pattern_owned) {
            Ok(re) => re,
            Err(_) => {
                // Fall back to literal substring search
                match regex::Regex::new(&regex::escape(&pattern_owned)) {
                    Ok(re) => re,
                    Err(_) => return Vec::new(),
                }
            }
        };

        let mut matches = Vec::new();

        for file in &files {
            let abs = Path::new(&file.project_root).join(&file.path);
            let content = match std::fs::read_to_string(&abs) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let lines: Vec<&str> = content.lines().collect();

            for (i, line) in lines.iter().enumerate() {
                if re.is_match(line) {
                    let before: Vec<String> = if i > 0 {
                        lines[i.saturating_sub(2)..i]
                            .iter()
                            .map(|l| l.to_string())
                            .collect()
                    } else {
                        Vec::new()
                    };

                    let after: Vec<String> = lines
                        .get(i + 1..std::cmp::min(i + 3, lines.len()))
                        .map(|slice| slice.iter().map(|l| l.to_string()).collect())
                        .unwrap_or_default();

                    matches.push(ContentMatch {
                        file_path: file.path.clone(),
                        project_root: file.project_root.clone(),
                        line_number: i + 1,
                        line_content: line.to_string(),
                        context_before: before,
                        context_after: after,
                    });

                    if matches.len() >= limit_owned {
                        return matches;
                    }
                }
            }
        }

        matches
    })
    .await
    .unwrap_or_default())
}
