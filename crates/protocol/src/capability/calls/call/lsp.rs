//! Asking a language server about the code.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Lsp {
    pub asks: LspQuestion,
    pub path: String,
    /// Where in the file, when the question is about a place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Position>,
}

/// A place in a file.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// From 1.
    pub line: u32,
    /// From 1.
    pub column: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LspQuestion {
    Definition,
    References,
    Implementation,
    Hover,
    Symbols,
    WorkspaceSymbols,
    Callers,
    Callees,
    #[serde(other)]
    Unknown,
}
