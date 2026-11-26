use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;

/// What the proxy reads of a JSON-RPC message an agent sends: what it asks for, and its ID where it's a request. A
/// notification has no ID, and an answer to the server's own request has no method, so it isn't read as a message.
pub(super) struct Message<'a> {
    pub(super) method: Cow<'a, str>,
    pub(super) id: Option<Value>,
}

#[derive(Deserialize)]
struct Read<'a> {
    #[serde(borrow)]
    method: Cow<'a, str>,
    #[serde(default)]
    id: Value,
}

impl Message<'_> {
    /// The message `body` is, if it's one.
    pub(super) fn of(body: &[u8]) -> Option<Message<'_>> {
        let Read { method, id } = serde_json::from_slice(body).ok()?;
        Some(Message { method, id: (!id.is_null()).then_some(id) })
    }
}
