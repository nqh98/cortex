use crate::models::{Language, Symbol, SymbolKind};
use crate::parser::Parser;
use std::path::Path;

pub struct JsParser;

impl Parser for JsParser {
    fn language(&self) -> Language {
        Language::JavaScript
    }

    fn parse(&self, content: &str, path: &Path) -> Vec<Symbol> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .expect("Failed to set JavaScript language");

        let tree = parser.parse(content, None).unwrap_or_else(|| {
            panic!("Failed to parse file: {}", path.display())
        });

        let root = tree.root_node();
        let mut symbols = Vec::new();
        extract_js_symbols(&root, content, &mut symbols);
        symbols
    }
}

fn extract_js_symbols(
    node: &tree_sitter::Node,
    source: &str,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            if let Some(sym) = extract_js_function(node, source, SymbolKind::Function) {
                symbols.push(sym);
            }
            return;
        }
        "class_declaration" => {
            if let Some(sym) = extract_js_class(node, source) {
                symbols.push(sym);
            }
            // Extract methods inside the class body
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "class_body" {
                    let mut body_cursor = child.walk();
                    for body_child in child.children(&mut body_cursor) {
                        if body_child.kind() == "method_definition" {
                            if let Some(method) = extract_js_method(&body_child, source) {
                                symbols.push(method);
                            }
                        }
                    }
                }
            }
            return;
        }
        "variable_declaration" | "lexical_declaration" => {
            extract_variable_declarations(node, source, symbols);
            return;
        }
        "export_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "function_declaration" => {
                        if let Some(sym) =
                            extract_js_function(&child, source, SymbolKind::Function)
                        {
                            symbols.push(sym);
                        }
                    }
                    "class_declaration" => {
                        extract_js_symbols(&child, source, symbols);
                    }
                    "lexical_declaration" | "variable_declaration" => {
                        extract_variable_declarations(&child, source, symbols);
                    }
                    _ => {}
                }
            }
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_js_symbols(&child, source, symbols);
    }
}

fn extract_js_function(
    node: &tree_sitter::Node,
    source: &str,
    kind: SymbolKind,
) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let params = find_child_by_kind(node, "formal_parameters");
    let signature = params
        .and_then(|p| p.utf8_text(source.as_bytes()).ok())
        .map(|p| format!("function {name_text}{p}"))
        .unwrap_or_else(|| format!("function {name_text}()"));

    Some(Symbol {
        name: name_text.to_string(),
        kind,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(signature),
        documentation: None,
    })
}

fn extract_js_class(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let heritage = find_child_by_kind(node, "class_heritage");
    let signature = heritage
        .and_then(|h| h.utf8_text(source.as_bytes()).ok())
        .map(|h| format!("class {name_text} {h}"))
        .unwrap_or_else(|| format!("class {name_text}"));

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Class,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(signature),
        documentation: None,
    })
}

fn extract_js_method(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    // Method name is a "property_identifier"
    let name = find_child_by_kind(node, "property_identifier")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let params = find_child_by_kind(node, "formal_parameters");
    let signature = params
        .and_then(|p| p.utf8_text(source.as_bytes()).ok())
        .map(|p| format!("{name_text}{p}"))
        .unwrap_or_else(|| format!("{name_text}()"));

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Method,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(signature),
        documentation: None,
    })
}

fn extract_variable_declarations(
    node: &tree_sitter::Node,
    source: &str,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            let name_node = child.child(0);
            let value_node = child.child(2);

            if let (Some(name_n), Some(value_n)) = (name_node, value_node) {
                if value_n.kind() == "arrow_function" {
                    if let Some(name) = name_n.utf8_text(source.as_bytes()).ok() {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                            start_col: child.start_position().column,
                            end_col: child.end_position().column,
                            signature: Some(format!("const {name} = () => ...")),
                            documentation: None,
                        });
                    }
                }
            }
        }
    }
}

fn find_child_by_kind<'a>(node: &'a tree_sitter::Node, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
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
        let code = "function greet(name) {\n    return `Hello ${name}`;\n}\n";
        let parser = JsParser;
        let symbols = parser.parse(code, &PathBuf::from("test.js"));
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_parse_class() {
        let code = "class Animal {\n    constructor(name) {\n        this.name = name;\n    }\n\n    speak() {\n        return `${this.name} makes a noise`;\n    }\n}\n";
        let parser = JsParser;
        let symbols = parser.parse(code, &PathBuf::from("test.js"));
        assert!(symbols.iter().any(|s| s.name == "Animal" && s.kind == SymbolKind::Class));
        assert!(symbols.iter().any(|s| s.name == "constructor" && s.kind == SymbolKind::Method));
        assert!(symbols.iter().any(|s| s.name == "speak" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_parse_arrow_function() {
        let code = "const add = (a, b) => a + b;\n";
        let parser = JsParser;
        let symbols = parser.parse(code, &PathBuf::from("test.js"));
        assert!(symbols.iter().any(|s| s.name == "add" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_parse_export() {
        let code = "export function handler(req, res) {\n    res.send(\"ok\");\n}\n\nexport class Controller {\n    index() {}\n}\n";
        let parser = JsParser;
        let symbols = parser.parse(code, &PathBuf::from("test.js"));
        assert!(symbols.iter().any(|s| s.name == "handler" && s.kind == SymbolKind::Function));
        assert!(symbols.iter().any(|s| s.name == "Controller" && s.kind == SymbolKind::Class));
    }

    #[test]
    fn test_parse_extends() {
        let code = "class Dog extends Animal {\n    bark() {}\n}\n";
        let parser = JsParser;
        let symbols = parser.parse(code, &PathBuf::from("test.js"));
        let class = symbols.iter().find(|s| s.kind == SymbolKind::Class).unwrap();
        assert!(class.signature.as_ref().unwrap().contains("extends"));
    }
}
