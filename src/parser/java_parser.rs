use crate::models::{Import, ImportType, Language, Symbol, SymbolKind};
use crate::parser::{ParseResult, Parser};
use std::path::Path;

pub struct JavaParser;

impl Parser for JavaParser {
    fn language(&self) -> Language {
        Language::Java
    }

    fn parse(&self, content: &str, path: &Path) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("Failed to set Java language");

        let tree = parser
            .parse(content, None)
            .unwrap_or_else(|| panic!("Failed to parse file: {}", path.display()));

        let root = tree.root_node();
        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        extract_java_symbols(&root, content, &mut symbols);
        extract_java_imports(&root, content, &mut imports);
        ParseResult { symbols, imports }
    }
}

fn extract_java_symbols(node: &tree_sitter::Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "class_declaration" => {
            if let Some(sym) = extract_java_class(node, source) {
                symbols.push(sym);
            }
            extract_body_symbols(node, source, symbols, "class_body");
            return;
        }
        "interface_declaration" => {
            if let Some(sym) = extract_java_interface(node, source) {
                symbols.push(sym);
            }
            extract_body_symbols(node, source, symbols, "interface_body");
            return;
        }
        "enum_declaration" => {
            if let Some(sym) = extract_java_enum(node, source) {
                symbols.push(sym);
            }
            extract_body_symbols(node, source, symbols, "enum_body");
            return;
        }
        "annotation_type_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                if let Some(name_text) = name.utf8_text(source.as_bytes()).ok() {
                    symbols.push(Symbol {
                        name: name_text.to_string(),
                        kind: SymbolKind::Interface,
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        start_col: node.start_position().column,
                        end_col: node.end_position().column,
                        signature: Some(format!("@interface {name_text}")),
                        documentation: extract_javadoc(node, source),
                    });
                }
            }
            extract_body_symbols(node, source, symbols, "annotation_type_body");
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_java_symbols(&child, source, symbols);
    }
}

fn extract_body_symbols(
    parent: &tree_sitter::Node,
    source: &str,
    symbols: &mut Vec<Symbol>,
    body_kind: &str,
) {
    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        if child.kind() == body_kind {
            extract_members(&child, source, symbols);
        }
    }
}

fn extract_members(body: &tree_sitter::Node, source: &str, symbols: &mut Vec<Symbol>) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "method_declaration" => {
                if let Some(method) = extract_java_method(&child, source) {
                    symbols.push(method);
                }
            }
            "constructor_declaration" => {
                if let Some(ctor) = extract_java_constructor(&child, source) {
                    symbols.push(ctor);
                }
            }
            "field_declaration" => {
                if let Some(field) = extract_java_field(&child, source) {
                    symbols.push(field);
                }
            }
            "constant_declaration" => {
                if let Some(field) = extract_java_constant(&child, source) {
                    symbols.push(field);
                }
            }
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "annotation_type_declaration" => {
                extract_java_symbols(&child, source, symbols);
            }
            "enum_body_declarations" => {
                // Java enums wrap methods/fields in enum_body_declarations
                extract_members(&child, source, symbols);
            }
            _ => {}
        }
    }
}

fn extract_java_class(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let mut parts = vec![format!("class {name_text}")];

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "superclass" => {
                if let Some(type_id) = find_child_by_kind(&child, "type_identifier") {
                    if let Some(t) = type_id.utf8_text(source.as_bytes()).ok() {
                        parts.push(format!("extends {t}"));
                    }
                }
            }
            "super_interfaces" => {
                let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
                parts.push(text.to_string());
            }
            _ => {}
        }
    }

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Class,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(parts.join(" ")),
        documentation: extract_javadoc(node, source),
    })
}

fn extract_java_interface(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let mut parts = vec![format!("interface {name_text}")];

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "extends_interfaces" {
            let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
            parts.push(text.to_string());
        }
    }

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Interface,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(parts.join(" ")),
        documentation: extract_javadoc(node, source),
    })
}

fn extract_java_enum(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let mut parts = vec![format!("enum {name_text}")];

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "super_interfaces" {
            let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
            parts.push(text.to_string());
        }
    }

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Enum,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(parts.join(" ")),
        documentation: extract_javadoc(node, source),
    })
}

fn extract_java_method(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = find_child_by_kind(node, "identifier")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let mut sig_parts = Vec::new();

    // Modifiers (public, static, etc.)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            if let Some(mod_text) = child.utf8_text(source.as_bytes()).ok() {
                sig_parts.push(mod_text.to_string());
            }
        }
    }

    // Return type
    let return_type = find_child_by_kind(node, "type_identifier")
        .or_else(|| find_child_by_kind(node, "generic_type"))
        .or_else(|| find_child_by_kind(node, "array_type"))
        .or_else(|| find_child_by_kind(node, "void_type"));
    if let Some(rt) = return_type {
        if let Some(rt_text) = rt.utf8_text(source.as_bytes()).ok() {
            sig_parts.push(rt_text.to_string());
        }
    }

    sig_parts.push(name_text.to_string());

    // Parameters
    let params = find_child_by_kind(node, "formal_parameters");
    let params_text = params
        .and_then(|p| p.utf8_text(source.as_bytes()).ok())
        .unwrap_or("()");
    sig_parts.push(params_text.to_string());

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Method,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(sig_parts.join(" ")),
        documentation: extract_javadoc(node, source),
    })
}

fn extract_java_constructor(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    let name = find_child_by_kind(node, "identifier")?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    let mut sig_parts = Vec::new();

    // Modifiers
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            if let Some(mod_text) = child.utf8_text(source.as_bytes()).ok() {
                sig_parts.push(mod_text.to_string());
            }
        }
    }

    sig_parts.push(name_text.to_string());

    let params = find_child_by_kind(node, "formal_parameters");
    let params_text = params
        .and_then(|p| p.utf8_text(source.as_bytes()).ok())
        .unwrap_or("()");
    sig_parts.push(params_text.to_string());

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Method,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(sig_parts.join(" ")),
        documentation: extract_javadoc(node, source),
    })
}

fn extract_java_field(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    // Only extract static final fields as constants
    let mut cursor = node.walk();
    let mut is_static = false;
    let mut is_final = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
            is_static = text.contains("static");
            is_final = text.contains("final");
        }
    }

    if !(is_static && is_final) {
        return None;
    }

    let var_decl = find_child_by_kind(node, "variable_declarator")?;
    let name = var_decl.child(0)?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Constant,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(node.utf8_text(source.as_bytes()).ok()?.to_string()),
        documentation: extract_javadoc(node, source),
    })
}

fn extract_java_constant(node: &tree_sitter::Node, source: &str) -> Option<Symbol> {
    // constant_declaration is used inside interfaces
    let var_decl = find_child_by_kind(node, "variable_declarator")?;
    let name = var_decl.child(0)?;
    let name_text = name.utf8_text(source.as_bytes()).ok()?;

    Some(Symbol {
        name: name_text.to_string(),
        kind: SymbolKind::Constant,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_col: node.start_position().column,
        end_col: node.end_position().column,
        signature: Some(node.utf8_text(source.as_bytes()).ok()?.to_string()),
        documentation: extract_javadoc(node, source),
    })
}

fn extract_java_imports(node: &tree_sitter::Node, source: &str, imports: &mut Vec<Import>) {
    if node.kind() == "import_declaration" {
        let line = node.start_position().row + 1;
        let raw = node
            .utf8_text(source.as_bytes())
            .ok()
            .unwrap_or("")
            .to_string();

        // Extract the scoped_identifier or scoped_type_identifier which is the import path
        let mut cursor = node.walk();
        let mut from_path = None;
        let mut is_wildcard = false;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "scoped_identifier" | "scoped_type_identifier" => {
                    from_path = child
                        .utf8_text(source.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                "asterisk" => {
                    is_wildcard = true;
                }
                "identifier" | "type_identifier" => {
                    if from_path.is_none() {
                        from_path = child
                            .utf8_text(source.as_bytes())
                            .ok()
                            .map(|s| s.to_string());
                    }
                }
                _ => {}
            }
        }

        let symbol = if is_wildcard {
            format!("{}.*", from_path.as_deref().unwrap_or(""))
        } else {
            // The last part of scoped identifier is the specific class/package
            from_path
                .as_deref()
                .map(|p| p.rsplit('.').next().unwrap_or(p).to_string())
                .unwrap_or_default()
        };

        imports.push(Import {
            imported_symbol: symbol,
            imported_from_path: from_path,
            import_type: ImportType::Import,
            start_line: Some(line),
            raw_statement: Some(raw),
        });
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_java_imports(&child, source, imports);
    }
}

fn find_child_by_kind<'a>(
    node: &'a tree_sitter::Node,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

/// Extract Javadoc comment preceding a node.
fn extract_javadoc(node: &tree_sitter::Node, source: &str) -> Option<String> {
    let parent = node.parent()?;
    let my_start = node.start_byte();

    let mut cursor = parent.walk();
    let mut comments: Vec<String> = Vec::new();

    for child in parent.children(&mut cursor) {
        if child.start_byte() >= my_start {
            break;
        }
        if child.kind() == "marker_annotation" || child.kind() == "annotation" {
            continue;
        }
        if child.kind() == "block_comment" {
            let text = child.utf8_text(source.as_bytes()).ok().unwrap_or("");
            if text.starts_with("/**") {
                let cleaned = clean_javadoc(text);
                if !cleaned.is_empty() {
                    comments.push(cleaned);
                }
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

fn clean_javadoc(text: &str) -> String {
    let text = text
        .trim_start_matches("/**")
        .trim_start_matches("/*")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_class_with_methods() {
        let code = r#"
public class UserService {
    private String name;

    public String getName() {
        return name;
    }

    public void setName(String name) {
        this.name = name;
    }
}
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("UserService.java"));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "UserService" && s.kind == SymbolKind::Class));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "getName" && s.kind == SymbolKind::Method));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "setName" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_parse_interface() {
        let code = r#"
public interface Repository {
    void save(Object entity);
    Object findById(long id);
}
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("Repository.java"));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "Repository" && s.kind == SymbolKind::Interface));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "save" && s.kind == SymbolKind::Method));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "findById" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_parse_enum() {
        let code = r#"
public enum Status {
    ACTIVE,
    INACTIVE,
    PENDING;

    public boolean isActive() {
        return this == ACTIVE;
    }
}
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("Status.java"));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "Status" && s.kind == SymbolKind::Enum));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "isActive" && s.kind == SymbolKind::Method));
    }

    #[test]
    fn test_parse_imports() {
        let code = r#"
import java.util.List;
import java.util.*;
import static java.util.Collections.emptyList;
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("Test.java"));
        assert!(result
            .imports
            .iter()
            .any(|i| i.imported_symbol == "List" && i.import_type == ImportType::Import));
        assert!(result
            .imports
            .iter()
            .any(|i| i.imported_symbol == "java.util.*"));
        assert!(result.imports.len() == 3);
    }

    #[test]
    fn test_parse_extends_implements() {
        let code = r#"
public class Dog extends Animal implements Comparable<Dog> {
    @Override
    public int compareTo(Dog other) {
        return 0;
    }
}
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("Dog.java"));
        let class = result
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Class)
            .unwrap();
        assert!(class.signature.as_ref().unwrap().contains("extends Animal"));
        assert!(class
            .signature
            .as_ref()
            .unwrap()
            .contains("implements Comparable"));
    }

    #[test]
    fn test_parse_constructor() {
        let code = r#"
public class User {
    private String name;

    public User(String name) {
        this.name = name;
    }
}
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("User.java"));
        assert!(result.symbols.iter().any(|s| s.name == "User"
            && s.kind == SymbolKind::Method
            && s.signature.as_ref().unwrap().contains("(String name)")));
    }

    #[test]
    fn test_parse_constant() {
        let code = r#"
public class Config {
    public static final int MAX_RETRIES = 3;
    private String name;
}
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("Config.java"));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "MAX_RETRIES" && s.kind == SymbolKind::Constant));
        // Regular field should not appear
        assert!(!result
            .symbols
            .iter()
            .any(|s| s.name == "name" && s.kind == SymbolKind::Constant));
    }

    #[test]
    fn test_parse_javadoc() {
        let code = r#"
/**
 * A service for managing users.
 * Handles CRUD operations.
 */
public class UserService {
}
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("UserService.java"));
        let class = result
            .symbols
            .iter()
            .find(|s| s.name == "UserService")
            .unwrap();
        let doc = class.documentation.as_ref().unwrap();
        assert!(doc.contains("service for managing users"));
        assert!(doc.contains("Handles CRUD operations"));
    }

    #[test]
    fn test_parse_inner_class() {
        let code = r#"
public class Outer {
    public class Inner {
        void doStuff() {}
    }
}
"#;
        let parser = JavaParser;
        let result = parser.parse(code, &PathBuf::from("Outer.java"));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "Outer" && s.kind == SymbolKind::Class));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "Inner" && s.kind == SymbolKind::Class));
        assert!(result
            .symbols
            .iter()
            .any(|s| s.name == "doStuff" && s.kind == SymbolKind::Method));
    }
}
