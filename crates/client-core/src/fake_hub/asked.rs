//! What a fake hub was asked, by each request's method, for a test to tell what a client read.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::ClientRequest;

/// The method of each request the hub was asked, in order, which every clone shares, so a test keeps one before the
/// hub is given the world.
#[derive(Clone, Default)]
pub struct Asked(Arc<Mutex<Vec<String>>>);

impl Asked {
    /// How many times a request of `method`, as it goes on the wire, such as `workspace_diff`, was asked.
    pub fn count(&self, method: &str) -> usize {
        self.0.lock().iter().filter(|asked| *asked == method).count()
    }

    pub(super) fn heard(&self, request: &ClientRequest) {
        let method =
            serde_json::to_value(request).ok().and_then(|sent| sent.get("method")?.as_str().map(str::to_string));
        self.0.lock().push(method.unwrap_or_default());
    }
}
