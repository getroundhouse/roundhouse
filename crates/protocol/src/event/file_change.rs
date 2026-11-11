//! What a tool changed in a file, as its agent reported it.

use serde::{Deserialize, Serialize};

/// What a tool changed in a file, as its agent reported it, so a client can show the change with the file's own line
/// numbers rather than work it out from the call. A kind this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileChange {
    /// The file didn't exist before. What it holds is in the call that wrote it.
    Created,
    /// The file was there, and these parts of it changed, in order.
    Changed { hunks: Vec<Hunk> },
    #[serde(other)]
    Unknown,
}

/// One changed part of a file, as a unified diff's hunk has it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// The line the hunk starts on in the file before, from 1.
    pub old_start: u32,
    /// The line the hunk starts on in the file after, from 1.
    pub new_start: u32,
    pub lines: Vec<HunkLine>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HunkLine {
    pub mark: LineMark,
    /// The line, without the mark a unified diff puts before it.
    pub text: String,
}

/// Whether a line of a hunk was kept, removed or added. A mark this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineMark {
    /// In the file both before and after.
    Same,
    Removed,
    Added,
    #[serde(other)]
    Unknown,
}
