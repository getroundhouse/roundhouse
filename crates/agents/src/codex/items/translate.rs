use protocol::attachment::NewAttachment;
use protocol::capability::calls::{
    Call, CallOutput,
    call::{Command, Runs, Sleep},
    output::{CommandOutput, Ended, McpContent},
};
use protocol::event::{CompactedBy, CompactionOutcome, ContentBlock, Denied, EventBody};
use protocol::model::ToolUseId;
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use serde_json::Value;

use super::{
    images,
    native::{Item, Status},
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
            Item::ImageView { id, path } => self.looked(id, path, completed, raw).await,
            Item::ImageGeneration(made) => self.made(made, completed, raw).await,
            Item::Sleep { id, duration_ms } => {
                // It says nothing as it ends but that it has waited.
                let output = CallOutput::Words { text: String::new() };
                self.tool(id, Call::Sleep(Sleep { duration_ms }), output, Status::Completed, completed, raw).await;
            }
            Item::WebSearch(searched) => self.searched(searched, completed, raw).await,
            // A subagent's says nothing of the agent's own context, so it's kept as it is, as Claude's adapter keeps
            // a subagent's.
            Item::ContextCompaction { started_at_ms, completed_at_ms } if self.scope.parent.is_none() => {
                let took_ms = completed_at_ms.zip(started_at_ms).map(|(ended, began)| ended.saturating_sub(began));
                self.shared.event(compaction(completed, took_ms), Some(raw)).await;
            }
            Item::ContextCompaction { .. } | Item::Unknown => {
                let kind = value["type"].as_str().unwrap_or("item").into();
                self.shared.event(EventBody::AgentOther { kind, subagent: self.scope.parent.clone() }, Some(raw)).await;
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
        let super::native::CommandExecution { id, command, cwd, process_id, output, exit_code, duration_ms, status } =
            command;
        let parts = crate::command_parts::of(&command);
        let call = Call::Command(Command {
            command,
            cwd: Some(cwd),
            description: None,
            timeout_ms: None,
            runs: Runs::Waits,
            parts,
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
                        subagent: self.scope.parent.clone(),
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
                    subagent: self.scope.parent.clone(),
                    change: DeltaChange::BlockStart {
                        index: 0,
                        kind: if thinking { BlockKind::Thinking } else { BlockKind::Text },
                        tool_name: None,
                    },
                }))
                .await;
        }
    }

    /// A look at the image at `path`, or once it's `completed`, the image it saw.
    async fn looked(&self, id: String, path: String, completed: bool, raw: String) {
        if completed {
            let kept = images::looked_at(&path).await;
            self.settled(id.into(), kept.output, kept.images, Status::Completed, raw).await;
        } else {
            self.started(id.into(), images::look(path), raw).await;
        }
    }

    /// Making an image, or once it's `completed`, the image it made, or why there isn't one.
    async fn made(&self, made: images::Made, completed: bool, raw: String) {
        if completed {
            let (id, ended) = (made.id.clone(), made.ended());
            let kept = made.kept().await;
            self.settled(id.into(), kept.output, kept.images, ended, raw).await;
        } else {
            self.started(made.id.clone().into(), made.call(), raw).await;
        }
    }

    /// A web search, whose start is held, since it says nothing of what it searches for. Once it completes, its call is
    /// recorded with the line that started it, then what it found.
    async fn searched(&self, search: super::web::Searched, completed: bool, raw: String) {
        let id: ToolUseId = search.id.clone().into();
        let started = {
            let mut state = self.shared.state.lock();
            let held = &mut state.work(self.scope.parent.as_ref()).searches;
            if !completed {
                held.insert(id, raw);
                return;
            }
            held.remove(&id)
        };
        // A search whose start this process didn't read, as when the adapter started again mid-search, has its call
        // recorded with the line that completed it.
        self.started(id.clone(), search.call(), started.unwrap_or_else(|| raw.clone())).await;
        self.settled(id, search.output(), Vec::new(), Status::Completed, raw).await;
    }

    /// A call Codex started, or, once it's `completed`, what came back from it.
    async fn tool(&self, id: String, call: Call, output: CallOutput, status: Status, completed: bool, raw: String) {
        if completed {
            self.settled(id.into(), output, Vec::new(), status, raw).await;
        } else {
            self.started(id.into(), call, raw).await;
        }
    }

    async fn started(&self, id: ToolUseId, call: Call, raw: String) {
        self.shared.state.lock().work(self.scope.parent.as_ref()).calls.insert(id.clone(), call.clone());
        self.shared
            .event(
                EventBody::Assistant {
                    message_id: Some(id.to_string().into()),
                    subagent: self.scope.parent.clone(),
                    model: self.scope.model.clone(),
                    blocks: vec![ContentBlock::Call { id, call }],
                    first_index: None,
                },
                Some(raw),
            )
            .await;
    }

    /// What came back from the call `id`, with `images` it names for the hub to keep, such as the one a look at an
    /// image saw.
    async fn settled(
        &self,
        id: ToolUseId,
        output: CallOutput,
        images: Vec<NewAttachment>,
        status: Status,
        raw: String,
    ) {
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
        let body = EventBody::AgentUser {
            subagent: self.scope.parent.clone(),
            blocks: vec![ContentBlock::CallResult {
                tool_use_id: id,
                output,
                is_error: !matches!(status, Status::Completed),
                denied,
                images: Vec::new(),
            }],
        };
        self.shared.emit(AgentOutput::Event { body, raw: Some(raw), attachments: images }).await;
    }
}

/// Codex compacting the agent's context as it fills, or, once it has, how long that took. It says nothing of the
/// tokens before or after, or the summary.
fn compaction(completed: bool, took_ms: Option<u64>) -> EventBody {
    if !completed {
        return EventBody::AgentCompacting { written_chars: None };
    }
    let outcome = CompactionOutcome::Compacted {
        by: CompactedBy::Agent,
        before_tokens: None,
        after_tokens: None,
        took_ms,
        summary: None,
    };
    EventBody::AgentCompacted { asked: None, keeping: None, outcome }
}

pub(super) fn mcp_result(result: &Value) -> CallOutput {
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
    // A null one is none, which is how it reads back.
    let structured = result.get("structuredContent").filter(|value| !value.is_null()).cloned();
    CallOutput::Mcp { content, structured }
}
