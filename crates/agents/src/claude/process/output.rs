//! Reading what the CLI writes.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use protocol::event::EventBody;
use serde_json::json;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{ChildStderr, ChildStdout};
use tokio::sync::mpsc;
use tracing::warn;

use super::{STDERR_TAIL_LINES, Shared, context, usage, write_line};
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
            Parsed::Prompt { uuid, progress } => {
                AgentOutput::event(prompt_progress(&shared, &uuid, progress), Some(line))
            }
            Parsed::UnsupportedControl { request_id, subtype } => {
                refuse_control(&shared, &request_id, &subtype).await;
                let kind = format!("control_request/{subtype}");
                AgentOutput::event(EventBody::AgentOther { kind, parent_tool_use_id: None }, Some(line))
            }
            Parsed::Event(body) => {
                track_approval(&shared, &body);
                let opened = matches!(body, EventBody::AgentInit { .. });
                // Not of a launch that hasn't opened, which may be about to be replaced by another.
                if opened || shared.confirmed.load(Ordering::SeqCst) {
                    context::after(&shared, &body, &output);
                    usage::after(&shared, &body, &output);
                }
                let item = AgentOutput::event(body, Some(line));
                if opened && !shared.confirmed.swap(true, Ordering::SeqCst) {
                    shared.unconfirmed_prompts.lock().clear();
                    let held: Vec<_> = shared.held_output.lock().drain(..).collect();
                    for item in held {
                        if output.send(item).await.is_err() {
                            return;
                        }
                    }
                }
                item
            }
        };
        if !deliver(&shared, &output, item).await {
            return;
        }
    }
}

/// Sends `item` on, or holds it until the CLI has confirmed the session. False once nobody is reading.
async fn deliver(shared: &Shared, output: &mpsc::Sender<AgentOutput>, item: AgentOutput) -> bool {
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
