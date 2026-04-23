use crate::models::Language;
use std::path::Path;

pub struct WalkResult {
    pub path: std::path::PathBuf,
    pub language: Language,
}

pub fn walk_directory(root: &Path) -> crate::error::Result<Vec<WalkResult>> {
    let mut results = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .build();

    for entry in walker {
        let entry = entry.map_err(|e| crate::error::CortexError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))?;

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        let path = entry.into_path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if let Some(language) = Language::from_extension(ext) {
            results.push(WalkResult { path, language });
        }
    }

    Ok(results)
}

pub fn directory_tree(root: &Path, max_depth: Option<usize>) -> crate::error::Result<String> {
    let mut lines = Vec::new();
    let root_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".");
    lines.push(root_name.to_string());

    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .build();

    let mut entries: Vec<_> = walker
        .filter_map(|e| e.ok())
        .filter(|e| e.path() != root)
        .collect();

    entries.sort_by(|a, b| a.path().cmp(b.path()));

    let total = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let depth = entry.depth();
        if let Some(max) = max_depth {
            if depth > max {
                continue;
            }
        }

        let is_last = i == total - 1
            || entries[i + 1..].iter().all(|e| e.depth() > depth);

        let prefix = if depth > 0 {
            let connector = if is_last { "└── " } else { "├── " };
            let indent = "│   ".repeat(depth.saturating_sub(1));
            format!("{indent}{connector}")
        } else {
            String::new()
        };

        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        let suffix = if entry.file_type().map_or(false, |ft| ft.is_dir()) {
            "/".to_string()
        } else {
            String::new()
        };

        lines.push(format!("{prefix}{name}{suffix}"));
    }

    Ok(lines.join("\n"))
}
