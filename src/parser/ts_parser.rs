use crate::models::{Import, ImportType, Language, Symbol, SymbolKind};
use crate::parser::{ParseResult, Parser};
use std::path::Path;

pub struct TsParser;

impl Parser for TsParser {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn parse(&self, content: &str, path: &Path) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .expect("Failed to set TypeScript/TSX language");

        let tree = parser
            .parse(content, None)
            .unwrap_or_else(|| panic!("Failed to parse file: {}", path.display()));

        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        extract_ts_symbols(&root, content, &mut symbols);
        extract_ts_imports(&root, content, &mut imports);
        ParseResult { symbols, imports }
    }
}

fn extract_ts_symbols(node: &tree_sitter::Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(sym) = extract_named_symbol(node, source, SymbolKind::Function, "function")
            {
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
            if let Some(sym) =
                extract_named_symbol(node, source, SymbolKind::Interface, "interface")
            {
                symbols.push(sym);
            }
            return;
        }
        "type_alias_declaration" => {
            if let Some(sym) = extract_named_symbol(node, source, SymbolKind::TypeAlias, "type") {
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
                    "function_declaration"
                    | "class_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "enum_declaration"
                    | "module" => extract_ts_symbols(&child, source, symbols),
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
        documentation: extract_jsdoc(node, source),
    })
}

fn extract_class(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    // Look for extends/implements clauses
    let mut cursor = node.walk();
    let heritage: Vec<String> = node
        .children(&mut cursor)
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
        documentation: extract_jsdoc(node, source),
    })
}

fn extract_methods(class_node: &tree_sitter::Node, source: &str, symbols: &mut Vec<Symbol>) {
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
    let name = node
        .child_by_field_name("name")
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
    let result = node.children(&mut cursor).find(|child| child.kind() == kind);
    result
}

/// Extract JSDoc comment preceding a node.
/// Walks backward through previous siblings to find a `comment` node,
/// skipping any `decorator` nodes (e.g. @Inject, @ApiProperty).
fn extract_jsdoc(node: &tree_sitter::Node, source: &str) -> Option<String> {
    // tree-sitter stores comments as children of the parent, not as named siblings.
    // We need to look at the parent's children to find comments before our node.
    let parent = node.parent()?;
    let my_start = node.start_byte();

    let mut cursor = parent.walk();
    let mut comments: Vec<String> = Vec::new();

    for child in parent.children(&mut cursor) {
        if child.start_byte() >= my_start {
            break;
        }
        // Collect consecutive comment blocks immediately before this node,
        // skipping over decorator nodes
        if child.kind() == "decorator" {
            // Preserve any comments collected so far — decorators may be between
            // the JSDoc and the declaration
            continue;
        }
        if child.kind() == "comment" {
            let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
            let cleaned = clean_jsdoc(text);
            if !cleaned.is_empty() {
                comments.push(cleaned);
            }
        } else if !comments.is_empty() {
            // Non-comment, non-decorator node between comments and our node — reset
            comments.clear();
        }
    }

    if comments.is_empty() {
        None
    } else {
        Some(comments.join(" "))
    }
}

/// Strip JSDoc markers and leading asterisks from a comment.
/// Handles /** ... */ and // ... styles.
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

fn extract_ts_imports(node: &tree_sitter::Node, source: &str, imports: &mut Vec<Import>) {
    match node.kind() {
        "import_statement" => {
            let line = node.start_position().row + 1;
            let raw = node
                .utf8_text(source.as_bytes())
                .ok()
                .unwrap_or("")
                .to_string();

            // Find the source path (string literal)
            let mut from_path = None;
            let mut symbols_list = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "string" | "template_string" => {
                        let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
                        from_path = Some(
                            text.trim_matches(|c| c == '\'' || c == '"' || c == '`')
                                .to_string(),
                        );
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
                    "namespace_import" => {
                        if let Some(name) = child.child_by_field_name("name") {
                            if let Ok(t) = name.utf8_text(source.as_bytes()) {
                                symbols_list.push(format!("* as {t}"));
                            }
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
        // Handle `const x = require('y')` pattern
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
        extract_ts_imports(&child, source, imports);
    }
}
