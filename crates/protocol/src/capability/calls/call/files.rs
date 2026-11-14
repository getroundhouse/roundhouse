//! Calls that read, write, change or look through files.

use serde::{Deserialize, Serialize};

use crate::event::{HunkLine, LineMark};
use crate::model::ids;

ids!(
    /// One of a notebook's cells, by its ID in the notebook.
    CellId,
);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Read {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<LineRange>,
    /// Which pages of a document, as the agent wrote them, such as `1-5`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineRange {
    /// From 1.
    pub first: u32,
    /// `None` reads to the end, or as far as the agent reads at once.
    pub count: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Write {
    pub path: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub path: String,
    /// In order, each applied to what the one before left.
    pub edits: Vec<Replace>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Replace {
    pub old: String,
    pub new: String,
    #[serde(default)]
    pub replaces: Replaces,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Replaces {
    /// The one place `old` is, which has to be the only one.
    #[default]
    One,
    Every,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub files: Vec<FilePatch>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FilePatch {
    pub path: String,
    pub change: PatchChange,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum PatchChange {
    Add {
        content: String,
    },
    Delete,
    Update {
        /// Where the file goes, when it moves too.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        move_to: Option<String>,
        /// Found by their context, since a patch needn't say line numbers. The result's file change has them.
        hunks: Vec<PatchHunk>,
    },
    #[serde(other)]
    Unknown,
}

impl PatchChange {
    /// Whether it holds nothing this build doesn't know.
    pub fn is_known(&self) -> bool {
        match self {
            PatchChange::Add { .. } | PatchChange::Delete => true,
            PatchChange::Update { move_to: _, hunks } => {
                hunks.iter().flat_map(|hunk| &hunk.lines).all(|line| line.mark != LineMark::Unknown)
            }
            PatchChange::Unknown => false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PatchHunk {
    /// The line the hunk's place is found after, such as a function's first line, when the patch gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    pub lines: Vec<HunkLine>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NotebookEdit {
    pub path: String,
    /// The cell, or `None` for the notebook's first.
    pub cell: Option<CellId>,
    pub source: String,
    pub does: CellEdit,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CellEdit {
    Replace,
    InsertAfter,
    Delete,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Search {
    /// A regular expression, as ripgrep reads one.
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Which files, by a glob, such as `*.rs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<String>,
    #[serde(default)]
    pub shows: Shows,
}

/// What a search sends back.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Shows {
    #[default]
    Lines,
    Files,
    Count,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FindFiles {
    /// A glob, such as `src/**/*.rs`.
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
