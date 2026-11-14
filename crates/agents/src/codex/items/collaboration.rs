use protocol::capability::calls::{
    Call, CallOutput,
    call::{MessageSubagent, StopSubagent, WaitForSubagents},
};
use protocol::model::ToolUseId;
use serde::Deserialize;
use serde_json::Value;

use super::native::Status;
use crate::codex::{running::Shared, state::ThreadId};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Collaboration {
    pub(super) id: ToolUseId,
    pub(super) status: Status,
    tool: Tool,
    receiver_thread_ids: Vec<ThreadId>,
    prompt: Option<String>,
    agents_states: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Tool {
    Wait,
    SendInput,
    SendMessage,
    FollowupTask,
    CloseAgent,
    InterruptAgent,
    #[serde(other)]
    Other,
}

impl Collaboration {
    pub(super) fn call(&self, shared: &Shared, value: &Value) -> Call {
        let state = shared.state.lock();
        let targets: Option<Vec<_>> = self.receiver_thread_ids.iter().map(|id| state.subagents.call(id)).collect();
        if let Some(targets) = targets {
            match (&self.tool, targets.as_slice()) {
                (Tool::Wait, _) => {
                    return Call::WaitForSubagents(WaitForSubagents { subagents: targets, timeout_ms: None });
                }
                (Tool::SendInput | Tool::SendMessage | Tool::FollowupTask, [target]) => {
                    if let Some(message) = &self.prompt {
                        return Call::MessageSubagent(MessageSubagent {
                            subagent: target.clone(),
                            message: message.clone(),
                        });
                    }
                }
                (Tool::CloseAgent | Tool::InterruptAgent, [target]) => {
                    return Call::StopSubagent(StopSubagent { subagent: target.clone() });
                }
                _ => {}
            }
        }
        // Some collaboration items target the root, or refer to an agent whose association wasn't supplied.
        // Keep the operation visible with its native arguments rather than inventing a subagent call to link to.
        Call::Other { tool: value["tool"].as_str().unwrap_or("collaboration").into(), input: value.clone() }
    }

    pub(super) fn output(&self) -> CallOutput {
        CallOutput::Words { text: self.agents_states.to_string() }
    }
}
