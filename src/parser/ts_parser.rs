use crate::models::{Language, Symbol, SymbolKind};
use crate::parser::Parser;
use std::path::Path;

pub struct TsParser;

impl Parser for TsParser {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn parse(&self, content: &str, path: &Path) -> Vec<Symbol> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .expect("Failed to set TypeScript/TSX language");

        let tree = parser.parse(content, None).unwrap_or_else(|| {
            panic!("Failed to parse file: {}", path.display())
        });

        let root = tree.root_node();
        let mut symbols = Vec::new();
        extract_ts_symbols(&root, content, &mut symbols);
        symbols
    }
}

fn extract_ts_symbols(
    node: &tree_sitter::Node,
    source: &str,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(sym) = extract_named_symbol(node, source, SymbolKind::Function, "function") {
                symbols.push(sym);
            }
            return;
        }
        "class_declaration" => {
            if let Some(sym) = extract_class(node, source) {
                symbols.push(sym);
            }
            extract_methods(node, source, symbols);
            return;
        }
        "interface_declaration" => {
            if let Some(sym) = extract_named_symbol(node, source, SymbolKind::Trait, "interface") {
                symbols.push(sym);
            }
            return;
        }
        "type_alias_declaration" => {
            if let Some(sym) = extract_named_symbol(node, source, SymbolKind::Struct, "type") {
                symbols.push(sym);
            }
            return;
        }
        "enum_declaration" => {
            if let Some(sym) = extract_named_symbol(node, source, SymbolKind::Enum, "enum") {
                symbols.push(sym);
            }
            return;
        }
        "lexical_declaration" | "variable_declaration" => {
            extract_variable_declarations(node, source, symbols);
            return;
        }
        "export_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "function_declaration"
                    | "generator_function_declaration"
                    | "class_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "enum_declaration"
                    | "lexical_declaration"
                    | "variable_declaration" => extract_ts_symbols(&child, source, symbols),
                    _ => {}
                }
            }
            return;
        }
        "ambient_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "function_declaration" | "class_declaration" | "interface_declaration"
                    | "type_alias_declaration" | "enum_declaration" | "module" => {
                        extract_ts_symbols(&child, source, symbols)
                    }
                    _ => {}
                }
            }
            return;
        }
        "abstract_class_declaration" => {
            if let Some(sym) = extract_class(node, source) {
                symbols.push(sym);
            }
            extract_methods(node, source, symbols);
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_ts_symbols(&child, source, symbols);
    }
}

fn extract_named_symbol(
    node: &tree_sitter::Node,
    source: &str,
    kind: SymbolKind,
    keyword: &str,
) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    Some(Symbol {
        name: name_text.to_string(),
        kind,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(format!("{keyword} {name_text}")),
        documentation: None,
    })
}

fn extract_class(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    // Look for extends/implements clauses
    let mut cursor = node.walk();
    let heritage: Vec<String> = node.children(&mut cursor)
        .filter(|c| c.kind() == "class_heritage")
        .filter_map(|h| h.utf8_text(source.as_bytes()).ok())
        .map(|s| s.to_string())
        .collect();

    let signature = if heritage.is_empty() {
        format!("class {name_text}")
    } else {
        format!("class {name_text} {}", heritage.join(" "))
    };

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

fn extract_methods(
    class_node: &tree_sitter::Node,
    source: &str,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "class_body" || child.kind() == "object_type" {
            let mut body_cursor = child.walk();
            for body_child in child.children(&mut body_cursor) {
                match body_child.kind() {
                    "method_definition" | "public_field_definition" | "property_signature" => {
                        if let Some(method) = extract_method_symbol(&body_child, source) {
                            symbols.push(method);
                        }
                    }
                    "abstract_method_signature" | "method_signature" => {
                        if let Some(method) = extract_method_symbol(&body_child, source) {
                            symbols.push(method);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn extract_method_symbol(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = node.child_by_field_name("name")
        .or_else(|| find_child_by_kind(node, "property_identifier"))
        .or_else(|| find_child_by_kind(node, "identifier"))?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Method,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(name_text.to_string()),
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
