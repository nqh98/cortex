use crate::mcp_server::models::{FileEntry, FileType};
use crate::models::Language;
use std::path::Path;

const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".cortex",
    "dist",
    "build",
    "out",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "coverage",
    ".next",
    ".nuxt",
    ".cache",
];

pub struct WalkResult {
    pub path: std::path::PathBuf,
    pub language: Language,
}

fn build_walker(root: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true);
    for dir in IGNORED_DIRS {
        builder.add_ignore(dir);
    }
    builder
}

pub fn walk_directory(root: &Path) -> crate::error::Result<Vec<WalkResult>> {
    let mut results = Vec::new();
    let walker = build_walker(root).build();

    for entry in walker {
        let entry = entry.map_err(|e| {
            crate::error::CortexError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

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

/// Structured directory tree listing
pub fn directory_tree_structured(
    root: &Path,
    max_depth: Option<usize>,
    extension_filter: Option<&str>,
) -> crate::error::Result<(Vec<FileEntry>, String)> {
    let root_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".");

    let walker = build_walker(root).build();
    let max_depth = max_depth.unwrap_or(3);

    let mut entries: Vec<FileEntry> = Vec::new();

    for entry in walker {
        let entry = entry.map_err(|e| {
            crate::error::CortexError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        let depth = entry.depth();
        if depth > max_depth {
            continue;
        }

        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Filter by extension if specified (only for files)
        if let Some(ext) = extension_filter {
            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                let file_ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if file_ext != ext {
                    continue;
                }
            }
        }

        let relative_path = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| name.clone());

        let entry_type = if entry.file_type().map_or(false, |ft| ft.is_dir()) {
            FileType::Directory
        } else if entry.file_type().map_or(false, |ft| ft.is_symlink()) {
            FileType::Symlink
        } else {
            FileType::File
        };

        let extension = if entry_type == FileType::File {
            path.extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        let language = if entry_type == FileType::File {
            extension
                .as_deref()
                .and_then(Language::from_extension)
                .map(|l| l.as_str().to_string())
        } else {
            None
        };

        let size = if entry_type == FileType::File {
            path.metadata()
                .ok()
                .map(|m| m.len())
        } else {
            None
        };

        entries.push(FileEntry {
            name,
            path: relative_path,
            entry_type,
            extension,
            language,
            size,
            depth,
        });
    }

    // Sort by path for consistent output
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok((entries, root_name.to_string()))
}

/// List files in a structured format
pub fn list_files_structured(
    root: &Path,
    extension_filter: Option<&str>,
    include_directories: bool,
    limit: usize,
) -> crate::error::Result<Vec<FileEntry>> {
    let walker = build_walker(root).build();

    let mut entries: Vec<FileEntry> = Vec::new();

    for entry in walker {
        if entries.len() >= limit {
            break;
        }

        let entry = entry.map_err(|e| {
            crate::error::CortexError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        // Skip the root directory itself
        if entry.path() == root {
            continue;
        }

        let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());
        let is_file = entry.file_type().map_or(false, |ft| ft.is_file());

        if is_dir && !include_directories {
            continue;
        }

        // Filter by extension for files
        if is_file {
            if let Some(ext) = extension_filter {
                let file_ext = entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if file_ext != ext {
                    continue;
                }
            }
        }

        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let relative_path = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| name.clone());

        let entry_type = if is_dir {
            FileType::Directory
        } else if entry.file_type().map_or(false, |ft| ft.is_symlink()) {
            FileType::Symlink
        } else {
            FileType::File
        };

        let extension = if entry_type == FileType::File {
            path.extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        let language = if entry_type == FileType::File {
            extension
                .as_deref()
                .and_then(Language::from_extension)
                .map(|l| l.as_str().to_string())
        } else {
            None
        };

        let size = if entry_type == FileType::File {
            path.metadata()
                .ok()
                .map(|m| m.len())
        } else {
            None
        };

        entries.push(FileEntry {
            name,
            path: relative_path,
            entry_type,
            extension,
            language,
            size,
            depth: entry.depth(),
        });
    }

    // Sort by path for consistent output
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(entries)
}

/// ASCII tree representation (for CLI use, not MCP)
pub fn directory_tree(root: &Path, max_depth: Option<usize>) -> crate::error::Result<String> {
    let mut lines = Vec::new();
    let root_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".");
    lines.push(root_name.to_string());

    let walker = build_walker(root).build();

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
