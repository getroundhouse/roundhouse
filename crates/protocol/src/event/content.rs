//! What a message holds: its blocks.

use serde::{Deserialize, Serialize};

use crate::attachment::Attachment;
use crate::model::ToolUseId;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
        /// The images the text points to in the worktree, as they were when it came, each by the address the text
        /// gives it, and the videos it points to as Markdown points to an image. The worker reads each, and the hub
        /// keeps it as an attachment of the session, so a client reads an image's bytes with `AttachmentRead`, as it
        /// does a prompt's, and a video's a part at a time with `AttachmentReadRange`. One that wasn't there, or wasn't
        /// an image or a video the transcript shows inside the worktree, isn't named, and neither is any in a text
        /// recorded before the hub kept them.
        ///
        /// This is the only record of which images the worker read, since the agent's raw line doesn't say. So
        /// anything that builds the event again from its raw line has to carry these over from the event it replaces.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<LinkedImage>,
    },
    Thinking {
        text: String,
    },
    /// A current adapter's call. Its native input is kept only in the event's raw line.
    Call {
        id: ToolUseId,
        call: crate::capability::calls::Call,
    },
    CallResult {
        tool_use_id: ToolUseId,
        output: crate::capability::calls::CallOutput,
        /// The call failed. A client shows what went wrong only from an output that's words: what a command printed,
        /// what an MCP tool or a subagent said, or `Words`, which is where an adapter records the agent's own words
        /// for a failure. One that failed with any other output, such as a patch's files, says only that it failed.
        is_error: bool,
        /// The person wouldn't let the call be made. Its adapter says so, since what came back is what the agent was
        /// told, which reads as any failure, and only the adapter can read where its agent says it was a denial. It's
        /// said here, and not on the record that closes the approval, since everything that draws a call reads its
        /// result, and a subagent's work, in its tab or on its record, has no way back to the request. A request a stop
        /// cancelled isn't a denial, so its call's result says none, and its approval's record says what became of it.
        /// *Unconfirmed:* that it's said here.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        denied: Option<Denied>,
        /// Images that came back beside what the call did, such as a screenshot a command made, each an attachment of
        /// the session, as a text's are. An image that's what the call is about, as a read of one is, is its
        /// output instead, as [`CallOutput::Image`](crate::capability::calls::CallOutput::Image) and an MCP tool's
        /// [`McpContent::Image`](crate::capability::calls::output::McpContent::Image) are.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<Attachment>,
    },
    Image {
        media_type: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// A tool's use the person wouldn't allow.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Denied {
    /// What the person said as they denied it, which the agent was told. `None` where they said nothing, and the
    /// agent was told in its adapter's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}

/// An image a text points to, or a video it points to as Markdown points to an image, and the attachment that keeps
/// it, whose kind says which.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LinkedImage {
    /// Where the text says it is, as its Markdown gives it, before any decoding.
    pub address: String,
    pub attachment: Attachment,
}
