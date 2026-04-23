use crate::models::SymbolKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Structured search result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    /// Matching symbols
    pub symbols: Vec<SymbolMatch>,
    /// Total count of matching symbols (may exceed returned list)
    pub total_count: u32,
    /// Whether more results are available
    pub has_more: bool,
}

/// Individual symbol match in search results
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolMatch {
    /// Database ID
    pub id: i64,
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// File path (relative to project root)
    pub file_path: String,
    /// Project root path
    pub project_root: String,
    /// Start line number (1-indexed)
    pub start_line: i64,
    /// End line number (1-indexed)
    pub end_line: i64,
    /// Function/method signature
    pub signature: Option<String>,
}

/// Structured code context result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CodeContextResult {
    /// Symbol name
    pub symbol_name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// File path (relative to project root)
    pub file_path: String,
    /// Start line number (1-indexed)
    pub start_line: i64,
    /// End line number (1-indexed)
    pub end_line: i64,
    /// Function/method signature
    pub signature: Option<String>,
    /// Code content (without line numbers)
    pub code: String,
    /// Preview with line numbers (for display)
    pub preview: String,
    /// Context lines before the symbol
    pub context_before: Option<Vec<String>>,
    /// Context lines after the symbol
    pub context_after: Option<Vec<String>>,
}

/// File entry in directory listing
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileEntry {
    /// File or directory name
    pub name: String,
    /// Full path (relative to project root)
    pub path: String,
    /// Type of entry
    pub entry_type: FileType,
    /// File extension (if file)
    pub extension: Option<String>,
    /// Programming language (if file)
    pub language: Option<String>,
    /// File size in bytes (if file)
    pub size: Option<u64>,
    /// Depth in directory tree
    pub depth: usize,
}

/// File or directory type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    File,
    Directory,
    Symlink,
}

/// Structured directory listing
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DirectoryListing {
    /// Root directory name
    pub root: String,
    /// Entries in the directory
    pub entries: Vec<FileEntry>,
    /// Total files found
    pub file_count: usize,
    /// Total directories found
    pub directory_count: usize,
}

/// Index status for a project
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IndexStatus {
    /// Whether the project is indexed
    pub is_indexed: bool,
    /// Number of indexed files
    pub file_count: u32,
    /// Number of indexed symbols
    pub symbol_count: u32,
    /// Last indexed timestamp (ISO 8601)
    pub last_indexed_at: Option<String>,
    /// Project root path
    pub project_root: String,
    /// Languages detected
    pub languages: Vec<String>,
}

/// Indexing result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IndexResult {
    /// Number of newly indexed files
    pub files_indexed: u32,
    /// Number of unchanged files
    pub files_unchanged: u32,
    /// Number of failed files
    pub files_failed: u32,
    /// Total symbols found
    pub symbols_found: u32,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Project root path
    pub project_root: String,
}

/// Ambiguous symbol error details
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AmbiguousSymbolDetails {
    /// The symbol name that was requested
    pub symbol_name: String,
    /// All matching symbols found
    pub matches: Vec<SymbolMatch>,
    /// Suggestion for resolving the ambiguity
    pub suggestion: String,
}

/// Symbol statistics
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolStats {
    /// Total symbols in the index
    pub total_symbols: u32,
    /// Count by kind
    pub by_kind: HashMap<String, u32>,
    /// Count by language
    pub by_language: HashMap<String, u32>,
}
