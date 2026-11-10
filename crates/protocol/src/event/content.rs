//! What a message holds: its blocks, and what a tool changed in a file.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::attachment::Attachment;
use crate::model::ToolUseId;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// What a call sent back. It comes in an `AgentUser` event, except for a tool the API runs itself, such as Claude's
    /// advisor, whose result comes in an `Assistant` event.
    ToolResult {
        tool_use_id: ToolUseId,
        is_error: bool,
        text: String,
        /// Images the tool sent back, such as a screenshot the agent read. Each is an attachment of the session, which
        /// the hub keeps, so a client reads its bytes with `AttachmentRead`, as it does a prompt's. A result recorded
        /// before the hub kept them names none.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<Attachment>,
        /// What the tool changed in a file, when the agent says, as with Claude's edits and writes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        change: Option<FileChange>,
        /// What came back is encrypted, for the model alone, as the advisor's reply can be, so `text` holds none of it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        encrypted: bool,
    },
    Image {
        media_type: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

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
