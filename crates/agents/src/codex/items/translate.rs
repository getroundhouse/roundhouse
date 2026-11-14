use protocol::capability::calls::{
    Call, CallOutput,
    call::{Command, CommandPart, Runs},
    output::{CommandOutput, Ended, McpContent},
};
use protocol::event::{ContentBlock, Denied, EventBody};
use protocol::model::ToolUseId;
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use serde_json::Value;

use super::{
    native::{Action, Item, Status},
    patches,
};
use crate::{AgentError, AgentOutput, codex::running::Shared};

pub(in crate::codex) struct Reader<'a> {
    pub(in crate::codex) shared: &'a Shared,
    pub(in crate::codex) scope: &'a crate::codex::subagents::Scope,
}

impl Reader<'_> {
    pub(in crate::codex) async fn item(&self, value: &Value, completed: bool, raw: String) -> Result<(), AgentError> {
        let item: Item = serde_json::from_value(value.clone())?;
        match item {
            Item::UserMessage {} => {
                if completed && self.scope.parent.is_none() {
                    let prompt = self.shared.state.lock().untaken.pop_front();
                    if let Some(prompt) = prompt {
                        self.shared.event(EventBody::PromptTaken { prompt }, Some(raw)).await;
                    }
                }
            }
            Item::AgentMessage { id, text } | Item::Plan { id, text } => {
                if completed {
                    self.note_reply(&text, value["type"] == "plan");
                }
                self.message(id, text, false, completed, raw).await;
            }
            Item::Reasoning { id, summary } => {
                self.message(id, summary.join("\n\n"), true, completed, raw).await;
            }
            Item::CommandExecution(command) => self.command_item(command, completed, raw).await,
            Item::SubAgentActivity(activity) => {
                crate::codex::subagents::activity(self.shared, self.scope, activity, raw).await;
            }
            Item::CollabAgentToolCall(collaboration) => {
                let call = collaboration.call(self.shared, value);
                let output = collaboration.output();
                self.tool(collaboration.id.to_string(), call, output, collaboration.status, completed, raw).await;
            }
            Item::FileChange { id, changes, status } => {
                let (call, output) = patches::patch(changes);
                self.tool(id, call, output, status, completed, raw).await;
            }
            Item::McpToolCall { id, server, tool: name, arguments, status, result, error } => {
                let call = Call::Mcp { server: server.into(), tool: name, input: arguments };
                let output = if let Some(error) = error {
                    CallOutput::Words { text: error.to_string() }
                } else {
                    mcp_result(&result.unwrap_or_default())
                };
                self.tool(id, call, output, status, completed, raw).await;
            }
            Item::Unknown => {
                self.shared
                    .event(
                        EventBody::AgentOther {
                            kind: value["type"].as_str().unwrap_or("item").into(),
                            parent_tool_use_id: self.scope.parent.clone(),
                        },
                        Some(raw),
                    )
                    .await;
            }
        }
        Ok(())
    }

    fn note_reply(&self, text: &str, plan: bool) {
        let mut state = self.shared.state.lock();
        if let Some(parent) = &self.scope.parent {
            if !plan
                && let Some(run) = state.subagents.runs.get_mut(parent)
                && !run.ended
            {
                run.reply = Some(text.to_owned());
            }
        } else if plan {
            state.plan = Some(text.to_owned());
        } else {
            state.reply = Some(text.to_owned());
        }
    }

    async fn command_item(&self, command: super::native::CommandExecution, completed: bool, raw: String) {
        let super::native::CommandExecution {
            id,
            command,
            cwd,
            process_id,
            actions,
            output,
            exit_code,
            duration_ms,
            status,
        } = command;
        let call = Call::Command(Command {
            command,
            cwd: Some(cwd),
            description: None,
            timeout_ms: None,
            runs: Runs::Waits,
            parts: actions.into_iter().map(part).collect(),
            unsandboxed: None,
        });
        if let Some(process) = process_id
            && !completed
        {
            self.shared.state.lock().work(self.scope.parent.as_ref()).terminals.insert(id.clone().into(), process);
        }
        let output = CallOutput::Command(CommandOutput {
            output: output.unwrap_or_default(),
            exit_code,
            ended: if self.scope.parent.is_none() && self.shared.state.lock().cancelling {
                Ended::Stopped
            } else {
                Ended::Exited
            },
            took_ms: duration_ms,
            truncated: None,
        });
        self.tool(id, call, output, status, completed, raw).await;
    }

    async fn message(&self, id: String, text: String, thinking: bool, completed: bool, raw: String) {
        if completed {
            let block = if thinking {
                ContentBlock::Thinking { text }
            } else {
                ContentBlock::Text { text, images: Vec::new() }
            };
            self.shared
                .event(
                    EventBody::Assistant {
                        message_id: Some(id.into()),
                        parent_tool_use_id: self.scope.parent.clone(),
                        model: self.scope.model.clone(),
                        blocks: vec![block],
                        first_index: Some(0),
                    },
                    Some(raw),
                )
                .await;
        } else {
            self.shared
                .emit(AgentOutput::Delta(StreamDelta {
                    run: None,
                    message_id: id.into(),
                    parent_tool_use_id: self.scope.parent.clone(),
                    change: DeltaChange::BlockStart {
                        index: 0,
                        kind: if thinking { BlockKind::Thinking } else { BlockKind::Text },
                        tool_name: None,
                    },
                }))
                .await;
        }
    }

    async fn tool(&self, id: String, call: Call, output: CallOutput, status: Status, completed: bool, raw: String) {
        let id = ToolUseId::from(id);
        if completed {
            let existed = {
                let mut state = self.shared.state.lock();
                let work = state.work(self.scope.parent.as_ref());
                work.terminals.remove(&id);
                work.calls.remove(&id).is_some()
            };
            if !existed {
                return;
            } // A cancelled call was already settled; its delayed native completion cannot settle it again.
            let denied = matches!(status, Status::Declined).then_some(Denied { said: None });
            self.shared
                .event(
                    EventBody::AgentUser {
                        parent_tool_use_id: self.scope.parent.clone(),
                        blocks: vec![ContentBlock::CallResult {
                            tool_use_id: id,
                            output,
                            is_error: !matches!(status, Status::Completed),
                            denied,
                        }],
                    },
                    Some(raw),
                )
                .await;
        } else {
            self.shared.state.lock().work(self.scope.parent.as_ref()).calls.insert(id.clone(), call.clone());
            self.shared
                .event(
                    EventBody::Assistant {
                        message_id: Some(id.to_string().into()),
                        parent_tool_use_id: self.scope.parent.clone(),
                        model: self.scope.model.clone(),
                        blocks: vec![ContentBlock::Call { id, call }],
                        first_index: None,
                    },
                    Some(raw),
                )
                .await;
        }
    }
}

fn part(action: Action) -> CommandPart {
    match action {
        Action::Read { path } => CommandPart::Read { path },
        Action::ListFiles { path } => CommandPart::List { path },
        Action::Search { path, query } => CommandPart::Search { pattern: query, path },
        Action::Unknown => CommandPart::Other,
    }
}

fn mcp_result(result: &Value) -> CallOutput {
    let content = result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|block| {
            if let Some(text) = block["text"].as_str() {
                McpContent::Text { text: text.into() }
            } else {
                McpContent::Unknown
            }
        })
        .collect();
    CallOutput::Mcp { content, structured: result.get("structuredContent").cloned() }
}
