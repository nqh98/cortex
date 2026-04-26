use crate::models::{Import, ImportType, Language, Symbol, SymbolKind};
use crate::parser::{ParseResult, Parser};
use std::path::Path;

pub struct JsParser;

impl Parser for JsParser {
    fn language(&self) -> Language {
        Language::JavaScript
    }

    fn parse(&self, content: &str, path: &Path) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .expect("Failed to set JavaScript language");

        let tree = parser
            .parse(content, None)
            .unwrap_or_else(|| panic!("Failed to parse file: {}", path.display()));

        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        extract_js_symbols(&root, content, &mut symbols);
        extract_js_imports(&root, content, &mut imports);
        ParseResult { symbols, imports }
    }
}

fn extract_js_symbols(node: &tree_sitter::Node, source: &str, symbols: &mut Vec<Symbol>) {
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
            let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
            let child_kinds: Vec<&str> = children.iter().map(|c| c.kind()).collect();

            // `export * from './module'`
            if child_kinds.contains(&"*") && child_kinds.contains(&"from") {
                let raw = node.utf8_text(source.as_bytes()).ok().unwrap_or("");
                if let Some(source_path) = extract_js_from_path(raw) {
                    symbols.push(Symbol {
                        name: format!("* from {source_path}"),
                        kind: SymbolKind::Module,
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        start_col: node.start_position().column,
                        end_col: node.end_position().column,
                        signature: Some(raw.trim().to_string()),
                        documentation: None,
                    });
                }
                return;
            }

            // `export { foo, bar } from './module'`
            if child_kinds.contains(&"export_clause") && child_kinds.contains(&"from") {
                if let Some(clause) = children.iter().find(|c| c.kind() == "export_clause") {
                    if let Some(sym) = extract_js_export_clause(clause, source, node) {
                        symbols.push(sym);
                    }
                }
                return;
            }

            for child in &children {
                match child.kind() {
                    "function_declaration" => {
                        if let Some(sym) = extract_js_function(child, source, SymbolKind::Function)
                        {
                            symbols.push(sym);
                        }
                    }
                    "class_declaration" => {
                        extract_js_symbols(child, source, symbols);
                    }
                    "lexical_declaration" | "variable_declaration" => {
                        extract_variable_declarations(child, source, symbols);
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

fn extract_js_export_clause(
    clause_node: &tree_sitter::Node,
    source: &str,
    export_node: &tree_sitter::Node,
) -> Option<Symbol> {
    let raw = export_node.utf8_text(source.as_bytes()).ok()?;
    if !raw.contains(" from ") && !raw.contains(" from'") && !raw.contains(" from\"") {
        return None;
    }
    let source_path = extract_js_from_path(raw)?;
    let mut names = Vec::new();
    let mut cursor = clause_node.walk();
    for child in clause_node.children(&mut cursor) {
        if child.kind() == "export_specifier" {
            if let Some(name_node) = child.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                    names.push(name.to_string());
                }
            }
        }
    }
    let display_name = if names.is_empty() {
        format!("{{}} from {source_path}")
    } else {
        format!("{{ {} }} from {source_path}", names.join(", "))
    };
    Some(Symbol {
        name: display_name,
        kind: SymbolKind::Module,
        start_line: export_node.start_position().row + 1,
        end_line: export_node.end_position().row + 1,
        start_col: export_node.start_position().column,
        end_col: export_node.end_position().column,
        signature: Some(raw.trim().to_string()),
        documentation: None,
    })
}

fn extract_js_from_path(raw: &str) -> Option<String> {
    let from_idx = raw.find(" from ")?;
    let after = &raw[from_idx + 6..].trim();
    Some(
        after
            .trim_matches(|c| c == '\'' || c == '"' || c == '`')
            .trim_end_matches(';')
            .to_string(),
    )
}

fn extract_js_function(node: &tree_sitter::Node, source: &str, kind: SymbolKind) -> Option<Symbol> {
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
        documentation: extract_jsdoc(node, source),
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
        documentation: extract_jsdoc(node, source),
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
        documentation: extract_jsdoc(node, source),
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
                    if let Ok(name) = name_n.utf8_text(source.as_bytes()) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                            start_col: child.start_position().column,
                            end_col: child.end_position().column,
                            signature: Some(format!("const {name} = () => ...")),
                            documentation: extract_jsdoc(&child, source),
                        });
                    }
                }
            }
        }
    }
}

fn find_child_by_kind<'a>(
    node: &'a tree_sitter::Node,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    result
}

/// Extract JSDoc comment preceding a node.
fn extract_jsdoc(node: &tree_sitter::Node, source: &str) -> Option<String> {
    let parent = node.parent()?;
    let my_start = node.start_byte();

    let mut cursor = parent.walk();
    let mut comments: Vec<String> = Vec::new();

    for child in parent.children(&mut cursor) {
        if child.start_byte() >= my_start {
            break;
        }
        if child.kind() == "decorator" {
            continue;
        }
        if child.kind() == "comment" {
            let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
            let cleaned = clean_jsdoc(text);
            if !cleaned.is_empty() {
                comments.push(cleaned);
            }
        } else if !comments.is_empty() {
            comments.clear();
        }
    }

    if comments.is_empty() {
        None
    } else {
        Some(comments.join(" "))
    }
}

fn clean_jsdoc(text: &str) -> String {
    let text = text
        .trim_start_matches("/**")
        .trim_start_matches("/*")
        .trim_start_matches("//")
        .trim_end_matches("*/")
        .trim();

    text.lines()
        .map(|line| line.trim().trim_start_matches('*').trim())
        .filter(|line| !line.is_empty() && !line.starts_with('@'))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn extract_js_imports(node: &tree_sitter::Node, source: &str, imports: &mut Vec<Import>) {
    match node.kind() {
        "import_statement" => {
            let line = node.start_position().row + 1;
            let raw = node
                .utf8_text(source.as_bytes())
                .ok()
                .unwrap_or("")
                .to_string();
            let mut from_path = None;
            let mut symbols_list = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "string" => {
                        let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
                        from_path = Some(text.trim_matches(|c| c == '\'' || c == '"').to_string());
                    }
                    "named_imports" => {
                        let mut ic = child.walk();
                        for cc in child.children(&mut ic) {
                            if cc.kind() == "import_specifier" {
                                if let Some(name) = cc.child_by_field_name("name") {
                                    if let Ok(t) = name.utf8_text(source.as_bytes()) {
                                        symbols_list.push(t.to_string());
                                    }
                                }
                            }
                        }
                    }
                    "identifier" => {
                        if let Ok(t) = child.utf8_text(source.as_bytes()) {
                            symbols_list.push(t.to_string());
                        }
                    }
                    _ => {}
                }
            }
            let import_symbol = if symbols_list.is_empty() {
                from_path.clone().unwrap_or_default()
            } else {
                symbols_list.join(", ")
            };
            imports.push(Import {
                imported_symbol: import_symbol,
                imported_from_path: from_path,
                import_type: ImportType::Import,
                start_line: Some(line),
                raw_statement: Some(raw),
            });
            return;
        }
        "call_expression" => {
            let func = node.child_by_field_name("function");
            if let Some(f) = func {
                if let Ok(name) = f.utf8_text(source.as_bytes()) {
                    if name == "require" {
                        let line = node.start_position().row + 1;
                        let raw = node
                            .utf8_text(source.as_bytes())
                            .ok()
                            .unwrap_or("")
                            .to_string();
                        let args = node.child_by_field_name("arguments");
                        let from_path = args.and_then(|a| {
                            a.children(&mut a.walk())
                                .filter_map(|c| {
                                    if c.kind() == "string" {
                                        c.utf8_text(source.as_bytes()).ok().map(|s| {
                                            s.trim_matches(|ch| ch == '\'' || ch == '"').to_string()
                                        })
                                    } else {
                                        None
                                    }
                                })
                                .next()
                        });
                        imports.push(Import {
                            imported_symbol: from_path.clone().unwrap_or_default(),
                            imported_from_path: from_path,
                            import_type: ImportType::Require,
                            start_line: Some(line),
                            raw_statement: Some(raw),
                        });
                    }
                }
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_js_imports(&child, source, imports);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_function() {
        let code = "function greet(name) {\n    return `Hello ${name}`;\n}\n";
        let parser = JsParser;
        let result = parser.parse(code, &PathBuf::from("test.js"));
        let symbols = &result.symbols;
        assert!(symbols
            .iter()
            .any(|s| s.name == "greet" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_parse_class() {
        let code = "class Animal {\n    constructor(name) {\n        this.name = name;\n    }\n\n    speak() {\n        return `${this.name} makes a noise`;\n    }\n}\n";
        let parser = JsParser;
        let result = parser.parse(code, &PathBuf::from("test.js"));
        let symbols = &result.symbols;
        assert!(symbols
            .iter()
            .any(|s| s.name == "Animal" && s.kind == SymbolKind::Class));
        assert!(symbols
            .iter()
            .any(|s| s.name == "constructor" && s.kind == SymbolKind::Method));
        assert!(symbols
            .iter()
            .any(|s| s.name == "speak" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_parse_arrow_function() {
        let code = "const add = (a, b) => a + b;\n";
        let parser = JsParser;
        let result = parser.parse(code, &PathBuf::from("test.js"));
        let symbols = &result.symbols;
        assert!(symbols
            .iter()
            .any(|s| s.name == "add" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_parse_export() {
        let code = "export function handler(req, res) {\n    res.send(\"ok\");\n}\n\nexport class Controller {\n    index() {}\n}\n";
        let parser = JsParser;
        let result = parser.parse(code, &PathBuf::from("test.js"));
        let symbols = &result.symbols;
        assert!(symbols
            .iter()
            .any(|s| s.name == "handler" && s.kind == SymbolKind::Function));
        assert!(symbols
            .iter()
            .any(|s| s.name == "Controller" && s.kind == SymbolKind::Class));
    }

    #[test]
    fn test_parse_extends() {
        let code = "class Dog extends Animal {\n    bark() {}\n}\n";
        let parser = JsParser;
        let result = parser.parse(code, &PathBuf::from("test.js"));
        let symbols = &result.symbols;
        let class = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Class)
            .unwrap();
        assert!(class.signature.as_ref().unwrap().contains("extends"));
    }

    #[test]
    fn test_parse_export_all() {
        let code = "export * from './helpers';\n";
        let parser = JsParser;
        let result = parser.parse(code, &PathBuf::from("index.js"));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Module && s.name.contains("* from ./helpers")));
    }

    #[test]
    fn test_parse_named_re_export() {
        let code = "export { processRequest } from './handler';\n";
        let parser = JsParser;
        let result = parser.parse(code, &PathBuf::from("index.js"));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Module && s.name.contains("processRequest")));
    }
}
