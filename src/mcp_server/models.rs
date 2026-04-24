use crate::models::SymbolKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Semantic search result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticSearchResult {
    /// Query used for the search
    pub query: String,
    /// Total matching symbols found
    pub total_count: u32,
    /// Whether more results are available
    pub has_more: bool,
    /// Matching symbols
    pub symbols: Vec<SymbolMatch>,
}

/// Find references result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FindReferencesResult {
    /// Symbol name searched
    pub symbol_name: String,
    /// Total references found
    pub total_count: u32,
    /// Whether more results are available
    pub has_more: bool,
    /// Reference matches
    pub references: Vec<ReferenceMatchEntry>,
}

/// Individual reference match
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReferenceMatchEntry {
    /// File path (relative to project root)
    pub file_path: String,
    /// Project root path
    pub project_root: String,
    /// Line number (1-indexed)
    pub line_number: usize,
    /// Content of the line containing the reference
    pub line_content: String,
    /// Type of reference (import, call, type_usage, definition, other)
    pub reference_type: String,
}

/// Content search result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContentSearchResult {
    /// Pattern used for the search
    pub pattern: String,
    /// Total matching lines found
    pub total_count: u32,
    /// Whether more results are available
    pub has_more: bool,
    /// Matching lines with context
    pub matches: Vec<ContentMatchEntry>,
}

/// Individual content match
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContentMatchEntry {
    /// File path (relative to project root)
    pub file_path: String,
    /// Project root path
    pub project_root: String,
    /// Line number of the match (1-indexed)
    pub line_number: usize,
    /// Content of the matched line
    pub line_content: String,
    /// 2 lines before the match for context
    pub context_before: Vec<String>,
    /// 2 lines after the match for context
    pub context_after: Vec<String>,
}

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

/// Document symbols result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentSymbolResult {
    /// File path (relative to project root)
    pub file_path: String,
    /// Project root path
    pub project_root: String,
    /// Language of the file
    pub language: Option<String>,
    /// Symbols in the file, sorted by start line
    pub symbols: Vec<DocumentSymbolEntry>,
}

/// Individual symbol entry in a document
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentSymbolEntry {
    /// Database ID
    pub id: i64,
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Start line number (1-indexed)
    pub start_line: i64,
    /// End line number (1-indexed)
    pub end_line: i64,
    /// Start column
    pub start_col: i64,
    /// End column
    pub end_col: i64,
    /// Function/method signature
    pub signature: Option<String>,
    /// Documentation
    pub documentation: Option<String>,
    /// Child symbols (symbols nested within this one)
    pub children: Vec<DocumentSymbolEntry>,
}

/// Import analysis result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportAnalysisResult {
    /// File path analyzed (relative to project root)
    pub file_path: String,
    /// Project root path
    pub project_root: String,
    /// Imports this file makes from other files
    pub outgoing: Vec<ImportEntry>,
    /// Files that import from this file
    pub incoming: Vec<ImportEntry>,
}

/// Individual import entry
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportEntry {
    /// Database ID
    pub id: i64,
    /// Symbol being imported
    pub imported_symbol: String,
    /// Path the symbol is imported from (raw source string)
    pub imported_from_path: Option<String>,
    /// Import type (import, require, use, from, include)
    pub import_type: String,
    /// Line number of the import statement
    pub start_line: Option<i64>,
    /// Raw import statement text
    pub raw_statement: Option<String>,
    /// File containing this import (relative to project root)
    pub file_path: String,
    /// Project root path
    pub project_root: String,
}
