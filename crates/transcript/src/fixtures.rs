//! Made-up events and logs, for tests that read the log the way drawing does.

use client_core::session_log::SessionLog;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::Seq;
use serde_json::json;

pub(crate) fn event(seq: Seq, body: EventBody) -> SessionEvent {
    SessionEvent { session_id: "s".into(), seq, ts_ms: 0, run: None, body, raw: None }
}

pub(crate) fn log(events: impl IntoIterator<Item = SessionEvent>) -> SessionLog {
    let mut log = SessionLog::new(0);
    for event in events {
        log.apply_event(event);
    }
    log
}

/// The agent running a command for each of `ids` at once.
pub(crate) fn calls(ids: &[&str]) -> EventBody {
    calls_to("Bash", ids)
}

/// The agent calling the tool `name` once for each of `ids`, at once.
pub(crate) fn calls_to(name: &str, ids: &[&str]) -> EventBody {
    let blocks = ids
        .iter()
        .map(|id| ContentBlock::ToolUse { id: (*id).to_string(), name: name.to_string(), input: json!({}) })
        .collect();
    EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None }
}

/// `body` as the subagent started by the call `parent` sends it.
pub(crate) fn within(parent: &str, mut body: EventBody) -> EventBody {
    if let EventBody::Assistant { parent_tool_use_id, .. } | EventBody::AgentUser { parent_tool_use_id, .. } = &mut body
    {
        *parent_tool_use_id = Some(parent.into());
    }
    body
}

/// The result of the call `id`.
pub(crate) fn result(id: &str) -> EventBody {
    let block = ContentBlock::ToolResult {
        tool_use_id: id.into(),
        is_error: false,
        text: format!("from {id}"),
        images: Vec::new(),
        change: None,
    };
    EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![block] }
}
