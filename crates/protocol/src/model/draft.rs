//! What's typed and attached in a chat's composer and not sent.

use serde::{Deserialize, Serialize};

use crate::attachment::Attachment;
use crate::model::AttachmentId;

/// What's typed and attached in a chat's composer and not sent, kept on the hub so it's there after the app restarts,
/// and in every client. It's the person's rather than the chat's, which a hub with several people would keep for each.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    /// What's typed, which can be empty when files are attached.
    #[serde(default)]
    pub text: String,
    /// The files waiting to go with it, in the order they were attached, as the hub has them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Attachment>,
}

impl Draft {
    /// Nothing to keep. Spaces alone are something, since they're what was typed, though they're no prompt.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.files.is_empty()
    }
}

/// What a client asks the hub to keep as a chat's [`Draft`]: what's typed, and the files waiting with it, by ID. Each
/// file is added to the session first, as a prompt's are, so the hub names each as it has it, and leaves out any it
/// hasn't been given.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct NewDraft {
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<AttachmentId>,
}

impl NewDraft {
    /// Only `text`, with no files.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), files: Vec::new() }
    }
}
