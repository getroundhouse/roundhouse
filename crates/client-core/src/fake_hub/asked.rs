//! What a fake hub was asked, by each request's method, for a test to tell what a client read.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::ClientRequest;
use serde_json::Value;

/// Each request the hub was asked, in order, which every clone shares, so a test keeps one before the hub is given the
/// world.
#[derive(Clone, Default)]
pub struct Asked(Arc<Mutex<Vec<Value>>>);

impl Asked {
    /// How many times a request of `method`, as it goes on the wire, such as `workspace_diff`, was asked.
    pub fn count(&self, method: &str) -> usize {
        self.0.lock().iter().filter(|asked| is_of(asked, method)).count()
    }

    /// The last request of `method`, as it goes on the wire, if any was asked.
    pub fn last(&self, method: &str) -> Option<Value> {
        self.0.lock().iter().rfind(|asked| is_of(asked, method)).cloned()
    }

    pub(super) fn heard(&self, request: &ClientRequest) {
        self.0.lock().push(serde_json::to_value(request).unwrap_or_default());
    }
}

fn is_of(asked: &Value, method: &str) -> bool {
    asked.get("method").and_then(Value::as_str) == Some(method)
}
