use protocol::capability::settings::Effort;
use protocol::model::{ModelName, ToolUseId};
use serde::Deserialize;

use crate::codex::state::ThreadId;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::codex) struct Activity {
    pub(super) id: ToolUseId,
    pub(super) kind: Kind,
    pub(super) agent_thread_id: ThreadId,
    pub(super) agent_path: String,
    pub(super) model: Option<ModelName>,
    pub(super) reasoning_effort: Option<Effort>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Kind {
    Started,
    Interacted,
    Interrupted,
    Completed,
    #[serde(other)]
    Unknown,
}
