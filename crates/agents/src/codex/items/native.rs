use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum Item {
    UserMessage {},
    AgentMessage {
        id: String,
        text: String,
    },
    Reasoning {
        id: String,
        #[serde(default)]
        summary: Vec<String>,
    },
    Plan {
        id: String,
        text: String,
    },
    CommandExecution(CommandExecution),
    CollabAgentToolCall(super::collaboration::Collaboration),
    SubAgentActivity(crate::codex::subagents::Activity),
    FileChange {
        id: String,
        changes: Vec<Change>,
        status: Status,
    },
    McpToolCall {
        id: String,
        server: String,
        tool: String,
        arguments: Value,
        status: Status,
        result: Option<Value>,
        error: Option<Value>,
    },
    /// A look at an image, which Codex says it started and completed at once, once it has read the image.
    ImageView {
        id: String,
        path: String,
    },
    ImageGeneration(super::images::Made),
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Status {
    InProgress,
    Completed,
    Failed,
    Declined,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
pub(super) struct Change {
    pub(super) path: String,
    pub(super) diff: String,
    pub(super) kind: ChangeKind,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum ChangeKind {
    Add,
    Delete,
    Update {
        move_path: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
pub(super) struct CommandExecution {
    pub(super) id: String,
    pub(super) command: String,
    pub(super) cwd: String,
    #[serde(rename = "processId")]
    pub(super) process_id: Option<String>,
    #[serde(rename = "aggregatedOutput")]
    pub(super) output: Option<String>,
    #[serde(rename = "exitCode")]
    pub(super) exit_code: Option<i32>,
    #[serde(rename = "durationMs")]
    pub(super) duration_ms: Option<u64>,
    pub(super) status: Status,
}
