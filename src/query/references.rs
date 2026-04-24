use crate::error::Result;
use crate::indexer::db::DbPool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceType {
    Import,
    Call,
    TypeUsage,
    Definition,
    Other,
}

#[derive(Debug, Clone)]
pub struct ReferenceMatch {
    pub file_path: String,
    pub project_root: String,
    pub line_number: usize,
    pub line_content: String,
    pub reference_type: ReferenceType,
}

/// Classify a reference match by analyzing the line content around the symbol name.
fn classify_reference(line: &str, symbol_name: &str, is_definition_file: bool) -> ReferenceType {
    let line_lower = line.to_lowercase();
    let line_trimmed = line.trim();

    // Check if this is the definition itself
    if is_definition_file {
        // Heuristic: definition lines typically start with the keyword + name
        let def_patterns = [
            format!("class {symbol_name}"),
            format!("interface {symbol_name}"),
            format!("type {symbol_name}"),
            format!("enum {symbol_name}"),
            format!("fn {symbol_name}"),
            format!("function {symbol_name}"),
            format!("const {symbol_name}"),
            format!("let {symbol_name}"),
            format!("var {symbol_name}"),
            format!("async fn {symbol_name}"),
            format!("async function {symbol_name}"),
            format!("export class {symbol_name}"),
            format!("export interface {symbol_name}"),
            format!("export type {symbol_name}"),
            format!("export enum {symbol_name}"),
            format!("export fn {symbol_name}"),
            format!("export function {symbol_name}"),
            format!("pub fn {symbol_name}"),
            format!("pub struct {symbol_name}"),
            format!("pub trait {symbol_name}"),
            format!("pub enum {symbol_name}"),
            format!("struct {symbol_name}"),
            format!("trait {symbol_name}"),
            format!("impl {symbol_name}"),
            format!("def {symbol_name}"),
        ];
        for pattern in &def_patterns {
            if line_trimmed.contains(pattern.as_str()) {
                return ReferenceType::Definition;
            }
        }
    }

    // Import patterns
    let import_keywords = ["import ", "from ", "require(", "use ", "use\t", "include "];
    for kw in &import_keywords {
        if line_lower.contains(kw) {
            return ReferenceType::Import;
        }
    }

    // Find position of symbol in line
    if let Some(pos) = line.find(symbol_name) {
        let after = &line[pos + symbol_name.len()..];
        let before = &line[..pos];

        // Call: symbol followed by ( or <
        let after_trimmed = after.trim_start();
        if after_trimmed.starts_with('(') || after_trimmed.starts_with('<') {
            return ReferenceType::Call;
        }

        // Type usage: symbol after :, implements, extends, as
        let before_trimmed = before.trim_end();
        if before_trimmed.ends_with(':')
            || before_trimmed.ends_with("implements")
            || before_trimmed.ends_with("extends")
            || before_trimmed.ends_with("as")
            || before_trimmed.ends_with("->")
        {
            return ReferenceType::TypeUsage;
        }
    }

    ReferenceType::Other
}

pub async fn find_references(
    pool: &DbPool,
    project_root: &str,
    symbol_name: &str,
    file_path: Option<&str>,
    limit: usize,
) -> Result<Vec<ReferenceMatch>> {
    // Get all content matches for the symbol name
    let content_matches = crate::query::content::search_content(
        pool,
        project_root,
        symbol_name,
        None,
        limit * 2, // Fetch extra since we'll filter some out
    )
    .await?;

    // Determine definition file if provided
    let def_file = file_path.map(|f| f.to_string());

    let mut references = Vec::new();
    for m in content_matches {
        // Skip lines that are inside comments (simple heuristic)
        let trimmed = m.line_content.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*") {
            continue;
        }
        if trimmed.starts_with('#') && !trimmed.starts_with("#[") && !trimmed.starts_with("#[") {
            continue;
        }

        let is_def_file = def_file
            .as_ref()
            .map(|df| m.file_path == *df || m.file_path.ends_with(df))
            .unwrap_or(false);

        let ref_type = classify_reference(&m.line_content, symbol_name, is_def_file);

        // Optionally skip definitions
        references.push(ReferenceMatch {
            file_path: m.file_path,
            project_root: m.project_root,
            line_number: m.line_number,
            line_content: m.line_content,
            reference_type: ref_type,
        });

        if references.len() >= limit {
            break;
        }
    }

    Ok(references)
}
