use crate::models::{Import, ImportType, Language, Symbol, SymbolKind};
use crate::parser::{ParseResult, Parser};
use std::path::Path;

pub struct RustParser;

impl Parser for RustParser {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn parse(&self, content: &str, path: &Path) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Failed to set Rust language");

        let tree = parser.parse(content, None).unwrap_or_else(|| {
            panic!("Failed to parse file: {}", path.display())
        });

        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        extract_symbols(&root, content, &mut symbols);
        extract_imports(&root, content, &mut imports);
        ParseResult { symbols, imports }
    }
}

fn extract_symbols(
    node: &tree_sitter::Node,
    source: &str,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_item" => {
            if let Some(sym) = extract_function(node, source) {
                symbols.push(sym);
            }
            return;
        }
        "struct_item" => {
            if let Some(sym) = extract_type_ident(node, source, SymbolKind::Struct) {
                symbols.push(sym);
            }
            return;
        }
        "enum_item" => {
            if let Some(sym) = extract_type_ident(node, source, SymbolKind::Enum) {
                symbols.push(sym);
            }
            return;
        }
        "trait_item" => {
            if let Some(sym) = extract_type_ident(node, source, SymbolKind::Trait) {
                symbols.push(sym);
            }
            return;
        }
        "impl_item" => {
            if let Some(sym) = extract_impl(node, source) {
                symbols.push(sym);
            }
            // Methods are inside declaration_list children
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "declaration_list" {
                    let mut dl_cursor = child.walk();
                    for dl_child in child.children(&mut dl_cursor) {
                        if dl_child.kind() == "function_item" {
                            if let Some(sym) = extract_function(&dl_child, source) {
                                let mut method_sym = sym;
                                method_sym.kind = SymbolKind::Method;
                                symbols.push(method_sym);
                            }
                        }
                    }
                }
            }
            return;
        }
        "const_item" => {
            if let Some(sym) = extract_const(node, source) {
                symbols.push(sym);
            }
            return;
        }
        "mod_item" => {
            if let Some(sym) = extract_mod(node, source) {
                symbols.push(sym);
            }
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_symbols(&child, source, symbols);
    }
}

fn extract_function(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name_text = name_node.utf8_text(source.as_bytes()).ok()?;

    let params = node.child_by_field_name("parameters");
    let params_text = params
        .and_then(|p| p.utf8_text(source.as_bytes()).ok())
        .unwrap_or("()");

    // Build return type by looking for -> followed by type
    let return_type = find_return_type(node, source);

    let signature = match return_type {
        Some(rt) => format!("fn {name_text}{params_text} -> {rt}"),
        None => format!("fn {name_text}{params_text}"),
    };

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Function,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(signature),
        documentation: extract_doc_comments(node, source),
    })
}

fn find_return_type(node: &tree_sitter::Node, source: &str) -> Option<String> {
    // Look for "-> type" pattern in function children
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    let mut found_arrow = false;
    for child in &children {
        if child.kind() == "->" {
            found_arrow = true;
            continue;
        }
        if found_arrow {
            return child.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
        }
    }
    None
}

fn extract_type_ident(
    node: &tree_sitter::Node,
    source: &str,
    kind: SymbolKind,
) -> Option<Symbol> {
    // struct_item, enum_item, trait_item use "name" field (type_identifier)
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    Some(Symbol {
        name: name_text.to_string(),
        kind,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: None,
        documentation: extract_doc_comments(node, source),
    })
}

fn extract_impl(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    // impl_item: "impl" [trait] "for"? type_identifier declaration_list
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    let mut type_name = None;
    let mut trait_name = None;
    let mut found_for = false;

    for (i, child) in children.iter().enumerate() {
        match child.kind() {
            "type_identifier" | "generic_type" | "scoped_type_identifier" => {
            if !found_for && i > 0 {
                // First type_identifier after "impl" could be the trait
                // Check if "for" keyword follows
                if let Some(next) = children.get(i + 1) {
                    if next.kind() == "for" {
                        trait_name = child.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
                        found_for = true;
                        continue;
                    }
                }
            }
            if found_for {
                type_name = child.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
            } else {
                type_name = child.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
            }
        }
            "for" => {
                found_for = true;
            }
            _ => {}
        }
    }

    let type_text = type_name?;
    let signature = if let Some(trait_t) = trait_name {
        Some(format!("impl {trait_t} for {type_text}"))
    } else {
        Some(format!("impl {type_text}"))
    };

    Some(Symbol {
        name: type_text,
        kind: SymbolKind::Impl,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature,
        documentation: None,
    })
}

fn extract_const(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    // const_item: "const" identifier ":" type "=" value ";"
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            let name = child.utf8_text(source.as_bytes()).ok()?;
            return Some(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Constant,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_col: node.start_position().column,
                end_col: node.end_position().column,
                signature: None,
                documentation: None,
            });
        }
    }
    None
}

fn extract_mod(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    // mod_item: "mod" identifier ";"
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            let name = child.utf8_text(source.as_bytes()).ok()?;
            return Some(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Module,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_col: node.start_position().column,
                end_col: node.end_position().column,
                signature: None,
                documentation: None,
            });
        }
    }
    None
}

fn extract_doc_comments(node: &tree_sitter::Node, source: &str) -> Option<String> {
    let mut comments = Vec::new();
    let mut sibling = node.prev_named_sibling();

    while let Some(sib) = sibling {
        if sib.kind() == "line_comment" {
            let text = sib.utf8_text(source.as_bytes()).ok()?;
            if text.starts_with("///") || text.starts_with("//!") {
                comments.push(text[3..].trim().to_string());
                sibling = sib.prev_named_sibling();
                continue;
            }
        }
        break;
    }

    if comments.is_empty() {
        return None;
    }

    comments.reverse();
    Some(comments.join("\n"))
}

fn extract_imports(
    node: &tree_sitter::Node,
    source: &str,
    imports: &mut Vec<Import>,
) {
    if node.kind() == "use_declaration" {
        let line = node.start_position().row + 1;
        let raw = node.utf8_text(source.as_bytes()).ok().unwrap_or("");

        // Extract the path from use statement (e.g., "use crate::parser::Parser")
        let path_text = node
            .children(&mut node.walk())
            .skip(1) // skip "use" keyword
            .filter_map(|c| {
                if c.kind() == "scoped_identifier"
                    || c.kind() == "identifier"
                    || c.kind() == "use_list"
                    || c.kind() == "scoped_use_list"
                {
                    c.utf8_text(source.as_bytes()).ok()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        if !path_text.is_empty() {
            // Extract the final identifier as the symbol name
            let symbol_name = path_text
                .split("::")
                .last()
                .unwrap_or("")
                .trim_start_matches('{')
                .trim_end_matches('}')
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            imports.push(Import {
                imported_symbol: if symbol_name == "*" || symbol_name.is_empty() {
                    path_text.clone()
                } else {
                    symbol_name
                },
                imported_from_path: Some(path_text),
                import_type: ImportType::Use,
                start_line: Some(line),
                raw_statement: Some(raw.to_string()),
            });
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_imports(&child, source, imports);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_simple_function() {
        let code = "fn hello_world() {\n    println!(\"Hello\");\n}\n";
        let parser = RustParser;
        let result = parser.parse(code, &PathBuf::from("test.rs"));
        let symbols = &result.symbols;
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "hello_world");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_parse_struct() {
        let code = "struct User {\n    name: String,\n    age: u32,\n}\n";
        let parser = RustParser;
        let result = parser.parse(code, &PathBuf::from("test.rs"));
        let symbols = &result.symbols;
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn test_parse_enum() {
        let code = "enum Color {\n    Red,\n    Green,\n    Blue,\n}\n";
        let parser = RustParser;
        let result = parser.parse(code, &PathBuf::from("test.rs"));
        let symbols = &result.symbols;
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Color");
        assert_eq!(symbols[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn test_parse_impl_with_methods() {
        let code = "impl User {\n    fn new(name: String) -> Self {\n        Self { name, age: 0 }\n    }\n\n    fn greet(&self) -> String {\n        format!(\"Hi, I'm {}\", self.name)\n    }\n}\n";
        let parser = RustParser;
        let result = parser.parse(code, &PathBuf::from("test.rs"));
        let symbols = &result.symbols;
        assert!(symbols.len() >= 3, "Expected >= 3 symbols, got {}", symbols.len());
        let impl_sym = symbols.iter().find(|s| s.kind == SymbolKind::Impl).unwrap();
        assert_eq!(impl_sym.name, "User");
        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Method).collect();
        assert_eq!(methods.len(), 2);
    }

    #[test]
    fn test_parse_trait() {
        let code = "trait Drawable {\n    fn draw(&self);\n}\n";
        let parser = RustParser;
        let result = parser.parse(code, &PathBuf::from("test.rs"));
        let symbols = &result.symbols;
        assert!(symbols.iter().any(|s| s.name == "Drawable" && s.kind == SymbolKind::Trait));
    }

    #[test]
    fn test_parse_constant() {
        let code = "const MAX_SIZE: usize = 1024;\n";
        let parser = RustParser;
        let result = parser.parse(code, &PathBuf::from("test.rs"));
        let symbols = &result.symbols;
        assert!(symbols.iter().any(|s| s.name == "MAX_SIZE" && s.kind == SymbolKind::Constant));
    }

    #[test]
    fn test_function_signature() {
        let code = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let parser = RustParser;
        let result = parser.parse(code, &PathBuf::from("test.rs"));
        let symbols = &result.symbols;
        assert_eq!(symbols.len(), 1);
        let sig = symbols[0].signature.as_ref().unwrap();
        assert!(sig.contains("fn add"), "Signature: {sig}");
        assert!(sig.contains("-> i32"), "Signature: {sig}");
    }

    #[test]
    fn test_parse_mixed() {
        let code = "mod my_module;\n\nconst VERSION: &str = \"1.0\";\n\nstruct Config {\n    debug: bool,\n}\n\nimpl Config {\n    fn new() -> Self {\n        Self { debug: false }\n    }\n}\n\nfn main() {\n    let cfg = Config::new();\n}\n\nenum Status {\n    Active,\n    Inactive,\n}\n";
        let parser = RustParser;
        let result = parser.parse(code, &PathBuf::from("test.rs"));
        let symbols = &result.symbols;
        let kinds: Vec<_> = symbols.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SymbolKind::Module), "Missing Module in {:?}", kinds);
        assert!(kinds.contains(&SymbolKind::Constant), "Missing Constant in {:?}", kinds);
        assert!(kinds.contains(&SymbolKind::Struct), "Missing Struct in {:?}", kinds);
        assert!(kinds.contains(&SymbolKind::Impl), "Missing Impl in {:?}", kinds);
        assert!(kinds.contains(&SymbolKind::Method), "Missing Method in {:?}", kinds);
        assert!(kinds.contains(&SymbolKind::Function), "Missing Function in {:?}", kinds);
        assert!(kinds.contains(&SymbolKind::Enum), "Missing Enum in {:?}", kinds);
    }
}
