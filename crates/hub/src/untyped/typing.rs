//! Typing events' bodies in the order they were recorded, so each result is read against the call it answers.

use std::collections::HashMap;

use protocol::capability::calls::Call;
use protocol::event::ContentBlock;
use protocol::model::ToolUseId;
use serde::Deserialize as _;
use serde_json::Value;

use super::older::{Returned, Use};
use super::outputs::{self, Came};
use super::{calls, requests};

/// Types what bodies hold in the older forms, read in the order they were recorded, each session's on a `Typing` of its
/// own: call IDs are the agent's, unique to each call.
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
            _ => false,
        }
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
