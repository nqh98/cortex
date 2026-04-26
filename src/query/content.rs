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
    context_lines: usize,
    multiline: bool,
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
    let ctx_owned = context_lines;
    let multiline_owned = multiline;

    // Run regex matching in a blocking thread
    Ok(tokio::task::spawn_blocking(move || {
        let re = if multiline_owned {
            match regex::RegexBuilder::new(&pattern_owned)
                .multi_line(true)
                .dot_matches_new_line(true)
                .build()
            {
                Ok(re) => re,
                Err(_) => {
                    let escaped = regex::escape(&pattern_owned);
                    match regex::RegexBuilder::new(&escaped).multi_line(true).build() {
                        Ok(re) => re,
                        Err(_) => return Vec::new(),
                    }
                }
            }
        } else {
            match regex::Regex::new(&pattern_owned) {
                Ok(re) => re,
                Err(_) => {
                    let escaped = regex::escape(&pattern_owned);
                    match regex::Regex::new(&format!(r"\b{escaped}\b")) {
                        Ok(re) => re,
                        Err(_) => match regex::Regex::new(&escaped) {
                            Ok(re) => re,
                            Err(_) => return Vec::new(),
                        },
                    }
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

            if multiline_owned {
                for mat in re.find_iter(&content) {
                    let start_line = content[..mat.start()].lines().count() + 1;
                    let end_line = content[..mat.end()].lines().count();

                    let lines: Vec<&str> = content.lines().collect();
                    let matched_text = mat.as_str();

                    let before: Vec<String> = if start_line > 1 {
                        let ctx_start = start_line.saturating_sub(ctx_owned + 1);
                        lines[ctx_start..start_line - 1]
                            .iter()
                            .map(|l| l.to_string())
                            .collect()
                    } else {
                        Vec::new()
                    };

                    let after: Vec<String> = if end_line < lines.len() {
                        lines[end_line..std::cmp::min(end_line + ctx_owned, lines.len())]
                            .iter()
                            .map(|l| l.to_string())
                            .collect()
                    } else {
                        Vec::new()
                    };

                    matches.push(ContentMatch {
                        file_path: file.path.clone(),
                        project_root: file.project_root.clone(),
                        line_number: start_line,
                        line_content: matched_text.to_string(),
                        context_before: before,
                        context_after: after,
                    });

                    if matches.len() >= limit_owned {
                        return matches;
                    }
                }
            } else {
                let lines: Vec<&str> = content.lines().collect();

                for (i, line) in lines.iter().enumerate() {
                    if re.is_match(line) {
                        let before: Vec<String> = if i > 0 {
                            lines[i.saturating_sub(ctx_owned)..i]
                                .iter()
                                .map(|l| l.to_string())
                                .collect()
                        } else {
                            Vec::new()
                        };

                        let after: Vec<String> = lines
                            .get(i + 1..std::cmp::min(i + 1 + ctx_owned, lines.len()))
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
        }

        matches
    })
    .await
    .unwrap_or_default())
}
