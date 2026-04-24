pub mod java_parser;
pub mod js_parser;
pub mod python_parser;
pub mod rust_parser;
pub mod ts_parser;

use crate::models::{Import, Language, Symbol};
use std::path::Path;

pub struct ParseResult {
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
}

pub trait Parser: Send + Sync {
    fn language(&self) -> Language;
    fn parse(&self, content: &str, path: &Path) -> ParseResult;
}

pub fn get_parser(language: Language) -> Box<dyn Parser> {
    match language {
        Language::Rust => Box::new(rust_parser::RustParser),
        Language::Python => Box::new(python_parser::PythonParser),
        Language::JavaScript => Box::new(js_parser::JsParser),
        Language::TypeScript => Box::new(ts_parser::TsParser),
        Language::Java => Box::new(java_parser::JavaParser),
    }
}
