//! Reading what the CLI writes.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use protocol::event::EventBody;
use protocol::model::CacheRequest;
use serde_json::json;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{ChildStderr, ChildStdout};
use tokio::sync::mpsc;
use tracing::warn;

use super::{STDERR_TAIL_LINES, Shared, context, plan, usage, write_line};
use crate::AgentOutput;
use crate::claude::normalize::{Normalized, Normalizer, Parsed, PromptProgress};

/// Reads bytes rather than text: one line of invalid UTF-8 mustn't end the whole stream, which would
/// leave the agent blocked and the session stuck.
async fn next_line<R: AsyncBufRead + Unpin>(reader: &mut R, buffer: &mut Vec<u8>) -> Option<String> {
    buffer.clear();
    match reader.read_until(b'\n', buffer).await {
        Ok(0) => None,
        Ok(_) => Some(String::from_utf8_lossy(buffer).trim_end_matches(['\n', '\r']).to_string()),
        Err(e) => {
            warn!("reading agent output failed: {e}");
            None
        }
    }
}

pub(super) async fn read_stdout(stdout: ChildStdout, shared: Arc<Shared>, output: mpsc::Sender<AgentOutput>) {
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::new();
    let mut normalizer = Normalizer::default();
    let mut answering = Answering::default();
    while let Some(line) = next_line(&mut reader, &mut buffer).await {
        if line.trim().is_empty() {
            continue;
        }
        let Normalized { dropped, parsed } = normalizer.line(&line);
        for tool_use_id in dropped {
            let said = AgentOutput::event(EventBody::ToolCallDropped { tool_use_id }, None);
            if !deliver(&shared, &output, said).await {
                return;
            }
        }
        let item = match parsed {
            Parsed::Drop => continue,
            Parsed::ControlReply { request_id, answer } => {
                if let Some(waiting) = shared.pending_controls.lock().remove(&request_id) {
                    let _ = waiting.send(answer);
                }
                continue;
            }
            Parsed::Delta(delta) => AgentOutput::Delta(delta),
            Parsed::EventWithImages { body, images, raw } => {
                AgentOutput::Event { body, raw: Some(raw), attachments: images }
            }
            Parsed::Prompt { uuid, progress } => answering.took(prompt_progress(&shared, &uuid, progress), line),
            Parsed::Request(request) => AgentOutput::Cache(answering.name(request)),
            Parsed::UnsupportedControl { request_id, subtype } => {
                refuse_control(&shared, &request_id, &subtype).await;
                let kind = format!("control_request/{subtype}");
                AgentOutput::event(EventBody::AgentOther { kind, parent_tool_use_id: None }, Some(line))
            }
            Parsed::Event(mut body) => {
                if !plan::passed_on(&shared, &output, &body).await {
                    continue;
                }
                note(&shared, &mut body);
                answering.note(&body);
                let opened = matches!(body, EventBody::AgentInit { .. });
                // Not of a launch that hasn't opened, which may be about to be replaced by another.
                if opened || shared.confirmed.load(Ordering::SeqCst) {
                    context::after(&shared, &body, &output);
                    usage::after(&shared, &body, &output);
                }
                let item = AgentOutput::event(body, Some(line));
                if opened && !confirm(&shared, &output).await {
                    return;
                }
                item
            }
        };
        if !deliver(&shared, &output, item).await {
            return;
        }
    }
}

/// Whether the CLI has taken a prompt that no request has answered yet, so the request that does can say it's the
/// first for it.
#[derive(Default)]
struct Answering(bool);

impl Answering {
    /// The CLI took a prompt, or ended a turn, which forgets it, since a turn the agent starts by itself answers none.
    fn note(&mut self, body: &EventBody) {
        if let EventBody::PromptTaken { .. } = body {
            self.0 = true;
        } else if let EventBody::TurnEnded { .. } = body {
            self.0 = false;
        }
    }

    /// What the CLI said of a prompt, `body`, in `line`, noted.
    fn took(&mut self, body: EventBody, line: String) -> AgentOutput {
        self.note(&body);
        AgentOutput::event(body, Some(line))
    }

    /// `request`, saying whether it's the first for a prompt.
    fn name(&mut self, mut request: CacheRequest) -> CacheRequest {
        request.first_for_prompt = std::mem::take(&mut self.0);
        request
    }
}

/// The CLI has opened the session, so what was held for it goes on. False once nobody is reading.
async fn confirm(shared: &Shared, output: &mpsc::Sender<AgentOutput>) -> bool {
    if shared.confirmed.swap(true, Ordering::SeqCst) {
        return true;
    }
    shared.unconfirmed_prompts.lock().clear();
    let held: Vec<_> = shared.held_output.lock().drain(..).collect();
    for item in held {
        if output.send(item).await.is_err() {
            return false;
        }
    }
    true
}

/// Sends `item` on, or holds it until the CLI has confirmed the session. False once nobody is reading.
pub(super) async fn deliver(shared: &Shared, output: &mpsc::Sender<AgentOutput>, item: AgentOutput) -> bool {
    if !shared.confirmed.load(Ordering::SeqCst) {
        shared.held_output.lock().push(item);
        return true;
    }
    output.send(item).await.is_ok()
}

/// What the CLI's report on a prompt means for the log. Only a prompt the CLI hasn't taken can be taken or dropped, so
/// the cancel that ends a turn says nothing of the prompts it took.
fn prompt_progress(shared: &Shared, uuid: &str, progress: PromptProgress) -> EventBody {
    let unchanged = || EventBody::AgentOther { kind: "command_lifecycle".to_string(), parent_tool_use_id: None };
    let moved = match progress {
        PromptProgress::Started | PromptProgress::Cancelled => shared.untaken.lock().remove(uuid),
        PromptProgress::Other => None,
    };
    match progress {
        PromptProgress::Started => moved.iter().for_each(|prompt| shared.compacts.lock().taken(uuid, *prompt)),
        PromptProgress::Cancelled => shared.compacts.lock().dropped(uuid),
        PromptProgress::Other => {}
    }
    match (progress, moved) {
        (PromptProgress::Started, Some(prompt)) => EventBody::PromptTaken { prompt },
        (PromptProgress::Cancelled, Some(prompt)) => EventBody::PromptDropped { prompt },
        (PromptProgress::Started | PromptProgress::Cancelled | PromptProgress::Other, _) => unchanged(),
    }
}

async fn refuse_control(shared: &Shared, request_id: &str, subtype: &str) {
    warn!("answering unsupported control request {subtype} with an error");
    let reply = json!({"type":"control_response","response":{
        "subtype":"error","request_id":request_id,"error":format!("{subtype} is not supported by this host"),
    }});
    if let Err(e) = write_line(shared, &reply).await {
        warn!("could not answer a control request: {e:#}");
    }
}

/// Keeps what the adapter needs of an event before it goes on, and adds what only the adapter knows: which prompt
/// asked for a compaction.
fn note(shared: &Shared, body: &mut EventBody) {
    shared.compacts.lock().answer(body);
    track_approval(shared, body);
}

/// Keeps each open approval's input, which is what answering it sends back.
fn track_approval(shared: &Shared, body: &EventBody) {
    match body {
        EventBody::ApprovalRequested { approval_id, input, .. } => {
            shared.pending_inputs.lock().insert(approval_id.clone(), input.clone());
        }
        EventBody::ApprovalResolved { approval_id, .. } => {
            shared.pending_inputs.lock().remove(approval_id);
        }
        EventBody::UserPrompt { .. }
        | EventBody::CompactionAsked { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::Unknown => {}
    }
}

pub(super) async fn read_stderr(stderr: ChildStderr) -> Vec<String> {
    let mut reader = BufReader::new(stderr);
    let mut buffer = Vec::new();
    let mut tail = VecDeque::with_capacity(STDERR_TAIL_LINES);
    while let Some(line) = next_line(&mut reader, &mut buffer).await {
        if tail.len() == STDERR_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    tail.into()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(body: serde_json::Value) -> EventBody {
        serde_json::from_value(body).unwrap()
    }

    fn named(answering: &mut Answering) -> bool {
        answering.name(serde_json::from_str("{}").unwrap()).first_for_prompt
    }

    #[test]
    fn only_the_first_request_after_a_prompt_is_the_first_for_it() {
        let mut answering = Answering::default();
        assert!(!named(&mut answering), "a turn the agent started by itself");

        answering.note(&event(json!({"type": "prompt_taken", "prompt": 7})));

        assert!(named(&mut answering));
        assert!(!named(&mut answering), "the next request of the same turn");
    }

    #[test]
    fn a_turn_that_ends_before_any_request_forgets_its_prompt() {
        let mut answering = Answering::default();
        answering.note(&event(json!({"type": "prompt_taken", "prompt": 7})));

        answering.note(&event(json!({"type": "turn_ended", "outcome": "cancelled", "result": null,
            "duration_ms": null, "cost_usd": null, "num_turns": null})));

        assert!(!named(&mut answering), "the next turn's first request is the agent's own");
    }
}
