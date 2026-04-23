use crate::models::{Language, Symbol, SymbolKind};
use crate::parser::Parser;
use std::path::Path;

pub struct PythonParser;

impl Parser for PythonParser {
    fn language(&self) -> Language {
        Language::Python
    }

    fn parse(&self, content: &str, path: &Path) -> Vec<Symbol> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("Failed to set Python language");

        let tree = parser.parse(content, None).unwrap_or_else(|| {
            panic!("Failed to parse file: {}", path.display())
        });

        let root = tree.root_node();
        let mut symbols = Vec::new();
        extract_python_symbols(&root, content, &mut symbols);
        symbols
    }
}

fn extract_python_symbols(
    node: &tree_sitter::Node,
    source: &str,
    symbols: &mut Vec<Symbol>,
) {
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
            for block_child in child.children(&mut block_cursor) {
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
                break; // Only check first statement
            }
            break;
        }
    }
    None
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
        let symbols = parser.parse(code, &PathBuf::from("test.py"));
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
        let symbols = parser.parse(code, &PathBuf::from("test.py"));
        assert!(symbols.iter().any(|s| s.name == "Dog" && s.kind == SymbolKind::Class));
        assert!(symbols.iter().any(|s| s.name == "__init__" && s.kind == SymbolKind::Method));
        assert!(symbols.iter().any(|s| s.name == "bark" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_parse_class_with_inheritance() {
        let code = r#"
class Animal(Base):
    pass
"#;
        let parser = PythonParser;
        let symbols = parser.parse(code, &PathBuf::from("test.py"));
        let class = symbols.iter().find(|s| s.kind == SymbolKind::Class).unwrap();
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
        let symbols = parser.parse(code, &PathBuf::from("test.py"));
        assert_eq!(
            symbols[0].documentation.as_deref(),
            Some("Say hello to someone.")
        );
    }
}
