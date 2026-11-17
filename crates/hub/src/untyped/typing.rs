//! Typing events' bodies in the order they were recorded, so each result is read against the call it answers.

use std::collections::HashMap;

use memchr::memmem;
use protocol::capability::calls::Call;
use protocol::event::ContentBlock;
use protocol::frame;
use protocol::model::ToolUseId;
use protocol::worker::WorkerFrame;
use serde::Deserialize as _;
use serde_json::Value;

use super::older::{Returned, Use};
use super::outputs::{self, Came};
use super::{calls, requests};

/// What a frame holds when it may hold one of the older forms: a block that's a call or what came back for one, or a
/// request for approval or its resolution. Each is a key and its value as JSON writes them, which a string, such as a
/// text or a raw line, can't hold, since its quotes are escaped. A typed request or resolution holds it too, which
/// typing leaves as it is.
const OLDER: [&str; 4] = [
    r#""type":"tool_use""#,
    r#""type":"tool_result""#,
    r#""type":"approval_requested""#,
    r#""type":"approval_resolved""#,
];

/// Types what bodies hold in the older forms, read in the order they were recorded, each session's on a `Typing` of its
/// own, or each worker's: call IDs are the agent's, unique to each call.
#[derive(Default)]
pub(crate) struct Typing {
    /// The calls typed so far whose results haven't come, by their IDs.
    calls: HashMap<ToolUseId, Call>,
}

impl Typing {
    /// Types what `body`, an event's body as JSON, holds in the older forms, in place, and says whether it held any.
    pub(crate) fn body(&mut self, body: &mut Value) -> bool {
        match body.get("type").and_then(Value::as_str) {
            Some("assistant" | "agent_user") => self.blocks(body),
            Some("approval_requested") => requests::type_request(body),
            // An agent closes a request only by cancelling it, which says nothing of what it asked.
            Some("approval_resolved") => requests::type_resolution(body, None, None),
            _ => false,
        }
    }

    /// A worker's frame, `bytes`, as [`frame::decode`] reads it, but with what an agent's event holds in the older
    /// forms typed, as a worker of a build from before they were sends it. Only a frame that may hold one is read as
    /// JSON first, since every frame is read here.
    pub(crate) fn frame(&mut self, bytes: &[u8]) -> serde_json::Result<WorkerFrame> {
        if !OLDER.iter().any(|older| memmem::find(bytes, older.as_bytes()).is_some()) {
            return frame::decode(bytes);
        }
        let mut frame: Value = frame::decode(bytes)?;
        if frame.get("frame").and_then(Value::as_str) == Some("agent_event")
            && let Some(body) = frame.get_mut("body")
        {
            self.body(body);
        }
        serde_json::from_value(frame)
    }

    fn blocks(&mut self, body: &mut Value) -> bool {
        let Some(blocks) = body.get_mut("blocks").and_then(Value::as_array_mut) else { return false };
        let mut typed_any = false;
        for block in blocks {
            let typed = match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => Use::deserialize(&*block).ok().map(|made| self.call(made)),
                Some("tool_result") => Returned::deserialize(&*block).ok().map(|came| self.result(came)),
                _ => None,
            };
            // A block of an older form that can't be read is left as it is, which reads as one this build doesn't know.
            if let Some(Ok(typed)) = typed.map(serde_json::to_value) {
                *block = typed;
                typed_any = true;
            }
        }
        typed_any
    }

    fn call(&mut self, Use { id, name, input }: Use) -> ContentBlock {
        let call = calls::call(&name, &input);
        self.calls.insert(id.clone(), call.clone());
        ContentBlock::Call { id, call }
    }

    fn result(&mut self, returned: Returned) -> ContentBlock {
        // A call has one result, so it's let go of once that's come.
        let call = self.calls.remove(&returned.tool_use_id);
        let came = Came {
            text: &returned.text,
            is_error: returned.is_error,
            exit_code: returned.exit_code,
            encrypted: returned.encrypted,
            change: returned.change.as_ref(),
            answered: returned.answered(),
        };
        let output = outputs::of(call.as_ref(), &came);
        let Returned { tool_use_id, is_error, images, denied, .. } = returned;
        ContentBlock::CallResult { tool_use_id, output, is_error, denied, images }
    }
}
