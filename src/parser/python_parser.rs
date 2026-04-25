use crate::models::{Import, ImportType, Language, Symbol, SymbolKind};
use crate::parser::{ParseResult, Parser};
use std::path::Path;

pub struct PythonParser;

impl Parser for PythonParser {
    fn language(&self) -> Language {
        Language::Python
    }

    fn parse(&self, content: &str, path: &Path) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("Failed to set Python language");

        let tree = parser
            .parse(content, None)
            .unwrap_or_else(|| panic!("Failed to parse file: {}", path.display()));

        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        extract_python_symbols(&root, content, &mut symbols);
        extract_python_imports(&root, content, &mut imports);
        ParseResult { symbols, imports }
    }
}

fn extract_python_symbols(node: &tree_sitter::Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "function_definition" => {
            if let Some(sym) = extract_python_function(node, source, SymbolKind::Function) {
                symbols.push(sym);
            }
        }
        "class_definition" => {
            if let Some(sym) = extract_python_class(node, source) {
                symbols.push(sym);
            }
            // Extract methods inside the class
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "block" {
                    let mut block_cursor = child.walk();
                    for block_child in child.children(&mut block_cursor) {
                        if block_child.kind() == "function_definition" {
                            if let Some(method) =
                                extract_python_function(&block_child, source, SymbolKind::Method)
                            {
                                symbols.push(method);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_python_symbols(&child, source, symbols);
    }
}

fn extract_python_function(
    node: &tree_sitter::Node,
    source: &str,
    kind: SymbolKind,
) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let params = node.child_by_field_name("parameters");
    let signature = params
        .and_then(|p| p.utf8_text(source.as_bytes()).ok())
        .map(|p| format!("def {name_text}{p}"))
        .unwrap_or_else(|| format!("def {name_text}()"));

    Some(Symbol {
        name: name_text.to_string(),
        kind,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(signature),
        documentation: extract_python_docstring(node, source),
    })
}

fn extract_python_class(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let superclasses = node.child_by_field_name("superclasses");
    let signature = superclasses
        .and_then(|s| s.utf8_text(source.as_bytes()).ok())
        .map(|s| format!("class {name_text}({s})"))
        .unwrap_or_else(|| format!("class {name_text}"));

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Class,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(signature),
        documentation: extract_python_docstring(node, source),
    })
}

fn extract_python_docstring(node: &tree_sitter::Node, source: &str) -> Option<String> {
    // Look for a string expression as the first statement in the body
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "block" {
            let mut block_cursor = child.walk();
            if let Some(block_child) = child.children(&mut block_cursor).next() {
                if block_child.kind() == "expression_statement" {
                    if let Some(string_node) = block_child.child(0) {
                        if string_node.kind() == "string" {
                            let text = string_node.utf8_text(source.as_bytes()).ok()?;
                            // Strip quotes
                            let trimmed = text
                                .trim_start_matches('"')
                                .trim_start_matches('\'')
                                .trim_start_matches("\"\"\"")
                                .trim_start_matches("'''")
                                .trim_end_matches('"')
                                .trim_end_matches('\'')
                                .trim_end_matches("\"\"\"")
                                .trim_end_matches("'''");
                            return Some(trimmed.trim().to_string());
                        }
                    }
                }
                // Only check first statement
            }
            break;
        }
    }
    None
}

fn extract_python_imports(node: &tree_sitter::Node, source: &str, imports: &mut Vec<Import>) {
    match node.kind() {
        "import_statement" => {
            let line = node.start_position().row + 1;
            let raw = node
                .utf8_text(source.as_bytes())
                .ok()
                .unwrap_or("")
                .to_string();
            // import X, Y, Z
            let mut cursor = node.walk();
            let dotted_names: Vec<String> = node
                .children(&mut cursor)
                .filter(|c| c.kind() == "dotted_name" || c.kind() == "aliased_import")
                .filter_map(|c| c.utf8_text(source.as_bytes()).ok())
                .map(|s| s.to_string())
                .collect();

            for name in &dotted_names {
                let symbol = name.split_whitespace().last().unwrap_or(name).to_string();
                imports.push(Import {
                    imported_symbol: symbol,
                    imported_from_path: Some(name.clone()),
                    import_type: ImportType::Import,
                    start_line: Some(line),
                    raw_statement: Some(raw.clone()),
                });
            }
            if dotted_names.is_empty() {
                imports.push(Import {
                    imported_symbol: raw.clone(),
                    imported_from_path: None,
                    import_type: ImportType::Import,
                    start_line: Some(line),
                    raw_statement: Some(raw),
                });
            }
            return;
        }
        "import_from_statement" => {
            let line = node.start_position().row + 1;
            let raw = node
                .utf8_text(source.as_bytes())
                .ok()
                .unwrap_or("")
                .to_string();
            // from X import Y, Z
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();

            let mut module_path = None;
            let mut symbol_names = Vec::new();

            for child in &children {
                match child.kind() {
                    "dotted_name" | "relative_import"
                        if module_path.is_none() => {
                            module_path = child
                                .utf8_text(source.as_bytes())
                                .ok()
                                .map(|s| s.to_string());
                        }
                    "identifier" => {
                        if let Ok(t) = child.utf8_text(source.as_bytes()) {
                            symbol_names.push(t.to_string());
                        }
                    }
                    "wildcard_import" => {
                        symbol_names.push("*".to_string());
                    }
                    "aliased_import" => {
                        if let Some(name) = child.child(0) {
                            if let Ok(t) = name.utf8_text(source.as_bytes()) {
                                symbol_names.push(t.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }

            let import_symbol = if symbol_names.is_empty() {
                module_path.clone().unwrap_or_default()
            } else {
                symbol_names.join(", ")
            };

            imports.push(Import {
                imported_symbol: import_symbol,
                imported_from_path: module_path,
                import_type: ImportType::From,
                start_line: Some(line),
                raw_statement: Some(raw),
            });
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_python_imports(&child, source, imports);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_function() {
        let code = r#"
def hello(name):
    print(f"Hello {name}")
"#;
        let parser = PythonParser;
        let result = parser.parse(code, &PathBuf::from("test.py"));
        let symbols = &result.symbols;
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "hello");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_parse_class_with_methods() {
        let code = r#"
class Dog:
    def __init__(self, name):
        self.name = name

    def bark(self):
        return f"{self.name} says woof!"
"#;
        let parser = PythonParser;
        let result = parser.parse(code, &PathBuf::from("test.py"));
        let symbols = &result.symbols;
        assert!(symbols
            .iter()
            .any(|s| s.name == "Dog" && s.kind == SymbolKind::Class));
        assert!(symbols
            .iter()
            .any(|s| s.name == "__init__" && s.kind == SymbolKind::Method));
        assert!(symbols
            .iter()
            .any(|s| s.name == "bark" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_parse_class_with_inheritance() {
        let code = r#"
class Animal(Base):
    pass
"#;
        let parser = PythonParser;
        let result = parser.parse(code, &PathBuf::from("test.py"));
        let symbols = &result.symbols;
        let class = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Class)
            .unwrap();
        assert!(class.signature.as_ref().unwrap().contains("Base"));
    }

    #[test]
    fn test_parse_docstring() {
        let code = r#"
def greet(name):
    """Say hello to someone."""
    return f"Hello {name}"
"#;
        let parser = PythonParser;
        let result = parser.parse(code, &PathBuf::from("test.py"));
        let symbols = &result.symbols;
        assert_eq!(
            symbols[0].documentation.as_deref(),
            Some("Say hello to someone.")
        );
    }
}
