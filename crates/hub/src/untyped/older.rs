//! The forms Claude's adapter recorded calls in before it typed them, as far as typing them reads them. Every field but
//! a call's ID may be missing, as in a record a build before it wrote, and reads as empty.

use protocol::attachment::Attachment;
use protocol::event::{Denied, FileChange};
use protocol::model::{Said, ToolUseId};
use serde::Deserialize;
use serde_json::Value;

use super::outputs::Answered;

/// A call, by the tool's name and what the agent gave it, as `{"type":"tool_use",…}` in a message's blocks, and as a
/// subagent's record kept it.
#[derive(Deserialize)]
pub(super) struct Use {
    pub id: ToolUseId,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub input: Value,
}

/// What came back for one, as `{"type":"tool_result",…}`.
#[derive(Deserialize)]
pub(super) struct Returned {
    pub tool_use_id: ToolUseId,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub images: Vec<Attachment>,
    #[serde(default)]
    pub change: Option<FileChange>,
    #[serde(default)]
    pub encrypted: bool,
    /// The kind of call it answered, as the adapter said, such as `{"kind":"command"}`. `None` in one from before it
    /// said, whose `text` is as the CLI sent it.
    #[serde(default)]
    pub call: Option<Value>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub denied: Option<Denied>,
}

impl Returned {
    /// What the adapter said of the call it answered.
    pub(super) fn answered(&self) -> Answered {
        match self.call.as_ref().map(|kind| kind.get("kind").and_then(Value::as_str)) {
            None => Answered::Unsaid,
            Some(Some("command")) => Answered::Command,
            Some(_) => Answered::Other,
        }
    }
}

/// What a subagent's record said it had done lately, as its `recent` kept it.
#[derive(Deserialize)]
pub(super) struct RecentWork {
    #[serde(default)]
    pub calls: Vec<RecentCall>,
    #[serde(default)]
    pub since: Option<Said>,
}

#[derive(Deserialize)]
pub(super) struct RecentCall {
    #[serde(default)]
    pub before: Option<Said>,
    pub call: Use,
    #[serde(default)]
    pub result: Option<RecentResult>,
}

/// What came back for a call, as a subagent's record kept it: a result's, but with its text cut short where it was
/// long, and without what it changed in a file.
#[derive(Deserialize)]
pub(super) struct RecentResult {
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub cut: bool,
    #[serde(default)]
    pub images: Vec<Attachment>,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub denied: Option<Denied>,
}
