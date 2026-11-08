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

use super::{STDERR_TAIL_LINES, Shared, write_line};
use crate::AgentOutput;
use crate::claude::normalize::{Normalizer, Parsed};

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
        let item = match normalizer.line(&line) {
            Parsed::Drop => continue,
            Parsed::ControlReply { request_id, error } => {
                if let Some(waiting) = shared.pending_controls.lock().remove(&request_id) {
                    let _ = waiting.send(error.map_or(Ok(()), Err));
                }
                continue;
            }
            Parsed::Delta(delta) => AgentOutput::Delta(delta),
            Parsed::UnsupportedControl { request_id, subtype } => {
                refuse_control(&shared, &request_id, &subtype).await;
                AgentOutput::Event {
                    body: EventBody::AgentOther { kind: format!("control_request/{subtype}") },
                    raw: Some(line),
                }
            }
            Parsed::Event(body) => {
                track_approval(&shared, &body);
                let opened = matches!(body, EventBody::AgentInit { .. });
                let item = AgentOutput::Event { body, raw: Some(line) };
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
        if !shared.confirmed.load(Ordering::SeqCst) {
            shared.held_output.lock().push(item);
        } else if output.send(item).await.is_err() {
            return;
        }
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
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
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
