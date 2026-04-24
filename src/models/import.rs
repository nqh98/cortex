use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportType {
    Import,
    Require,
    Use,
    From,
    Include,
}

impl ImportType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Require => "require",
            Self::Use => "use",
            Self::From => "from",
            Self::Include => "include",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "import" => Self::Import,
            "require" => Self::Require,
            "use" => Self::Use,
            "from" => Self::From,
            "include" => Self::Include,
            _ => Self::Include,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Import {
    pub imported_symbol: String,
    pub imported_from_path: Option<String>,
    pub import_type: ImportType,
    pub start_line: Option<usize>,
    pub raw_statement: Option<String>,
}
