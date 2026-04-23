pub mod js_parser;
pub mod python_parser;
pub mod rust_parser;

use crate::models::{Language, Symbol};
use std::path::Path;

pub trait Parser: Send + Sync {
    fn language(&self) -> Language;
    fn parse(&self, content: &str, path: &Path) -> Vec<Symbol>;
}

pub fn get_parser(language: Language) -> Box<dyn Parser> {
    match language {
        Language::Rust => Box::new(rust_parser::RustParser),
        Language::Python => Box::new(python_parser::PythonParser),
        Language::JavaScript => Box::new(js_parser::JsParser),
    }
}
