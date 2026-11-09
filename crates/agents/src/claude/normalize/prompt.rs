//! The CLI's word on where a prompt the adapter sent has got to.

use serde_json::Value;

use super::{Parsed, other, str_of};

/// What the CLI's `command_lifecycle` says of a prompt. It reports `queued` when it reads one, `started` when it takes
/// it into the conversation, and `completed` or `cancelled` when that turn ends. A prompt that's `cancelled` before it
/// `started` was dropped unread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptProgress {
    Started,
    Cancelled,
    /// `queued`, `completed`, or something newer. Nothing changes for these.
    Other,
}

/// A `command_lifecycle` line, about the prompt sent with its `command_uuid`.
pub(super) fn command_lifecycle(value: &Value) -> Parsed {
    let Some(uuid) = str_of(value, "command_uuid") else { return other("command_lifecycle") };
    let progress = match str_of(value, "state") {
        Some("started") => PromptProgress::Started,
        Some("cancelled") => PromptProgress::Cancelled,
        _ => PromptProgress::Other,
    };
    Parsed::Prompt { uuid: uuid.to_string(), progress }
}
