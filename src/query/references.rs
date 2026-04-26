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
    context_lines: usize,
) -> Result<Vec<ReferenceMatch>> {
    // Get all content matches for the symbol name
    let content_matches = crate::query::content::search_content(
        pool,
        project_root,
        symbol_name,
        None,
        limit * 3, // Fetch extra since we'll filter many out
        context_lines,
        false,
    )
    .await?;

    // Determine definition file if provided
    let def_file = file_path.map(|f| f.to_string());

    // Group matches by file and sort by line number for stateful comment tracking
    let mut by_file: std::collections::HashMap<String, Vec<&crate::query::content::ContentMatch>> =
        std::collections::HashMap::new();
    for m in &content_matches {
        by_file.entry(m.file_path.clone()).or_default().push(m);
    }
    for matches in by_file.values_mut() {
        matches.sort_by_key(|m| m.line_number);
    }

    let mut references = Vec::new();
    for (file, file_matches) in by_file {
        let is_def_file = def_file
            .as_ref()
            .map(|df| file == *df || file.ends_with(df.as_str()))
            .unwrap_or(false);

        // Determine comment style from file extension
        let ext = std::path::Path::new(&file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let (uses_block, uses_triple) = match ext {
            "py" => (false, true),
            "rs" | "java" | "js" | "ts" | "go" | "c" | "cpp" | "h" | "hpp" => (true, false),
            "html" | "htm" | "xml" | "svg" => (true, false),
            _ => (true, false),
        };

        let mut in_block_comment = false;
        let mut in_triple_quote = false;
        for m in file_matches {
            // Scan lines between last checked and current to update comment state.
            // We need the actual file content for lines we haven't seen.
            // Since we only have matched lines, do per-line state tracking on the matches.
            // This is imperfect but handles the common case where matched lines are sequential.
            let line = &m.line_content;
            let trimmed = line.trim();

            // Track block comment state (/* ... */)
            if uses_block {
                // Count open/close markers in this line
                let opens = trimmed.matches("/*").count();
                let closes = trimmed.matches("*/").count();
                // JSX/HTML style
                if trimmed.contains("{/*") {
                    in_block_comment = true;
                }
                if opens > closes && !trimmed.contains("*/") {
                    in_block_comment = true;
                }
                if closes > 0 && trimmed.ends_with("*/") {
                    in_block_comment = false;
                    continue;
                }
                if in_block_comment {
                    continue;
                }
            }

            // Track Python triple-quote state
            if uses_triple {
                let triple_count =
                    trimmed.matches("\"\"\"").count() + trimmed.matches("'''").count();
                if triple_count % 2 == 1 {
                    in_triple_quote = !in_triple_quote;
                }
                if in_triple_quote {
                    continue;
                }
            }

            // Single-line comment styles
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed.starts_with('#') && !trimmed.starts_with("#[") {
                continue;
            }
            // Lines starting with * inside a block comment (already handled by in_block_comment,
            // but catch standalone cases)
            if trimmed.starts_with('*') && trimmed.len() > 1 && trimmed.chars().nth(1) == Some('/')
            {
                continue;
            }

            let ref_type = classify_reference(line, symbol_name, is_def_file);

            references.push(ReferenceMatch {
                file_path: file.clone(),
                project_root: m.project_root.clone(),
                line_number: m.line_number,
                line_content: m.line_content.clone(),
                reference_type: ref_type,
            });

            if references.len() >= limit {
                return Ok(references);
            }
        }
    }

    Ok(references)
}
