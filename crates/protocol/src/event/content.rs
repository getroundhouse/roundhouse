//! What a message holds: its blocks.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{CallKind, FileChange};
use crate::attachment::Attachment;
use crate::model::ToolUseId;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
        /// The images the text points to in the worktree, as they were when it came, each by the address the text
        /// gives it. The worker reads each, and the hub keeps it as an attachment of the session, so a client reads
        /// its bytes with `AttachmentRead`, as it does a prompt's. One that wasn't there, or wasn't an image inside the
        /// worktree, isn't named, and neither is any in a text recorded before the hub kept them.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<LinkedImage>,
    },
    Thinking {
        text: String,
    },
    ToolUse {
        id: ToolUseId,
        /// The agent's own name for the tool, and what it gave it.
        name: String,
        input: Value,
        /// What kind of call it is, as the agent's adapter read it from the tool. `None` in one recorded before
        /// adapters said, which a client reads from the tool's name as Claude's were then.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<CallKind>,
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
        /// What kind of call it answers, as the agent's adapter read it, or `Unknown` when the adapter no longer knows
        /// the call. With it, `text` is what came back alone, and how a command exited is `exit_code`. `None` only
        /// in one recorded before adapters said, whose `text` is as Claude sent it then, with what it put in it to
        /// say how it failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<CallKind>,
        /// How the command exited, when the call was one and the agent said.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
    Image {
        media_type: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// An image a text points to, and the attachment that keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LinkedImage {
    /// Where the text says the image is, as its Markdown gives it, before any decoding.
    pub address: String,
    pub attachment: Attachment,
}
