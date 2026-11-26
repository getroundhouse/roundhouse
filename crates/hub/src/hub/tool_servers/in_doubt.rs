use std::collections::HashMap;

use parking_lot::Mutex;
use protocol::model::ToolServerId;
use tool_sign_in::Registration;

/// The registrations that were used to sign in again and that nothing has come of, by server. An authorization server
/// that has forgotten one shows the person an error on its own page, and sends nothing back to say so. So one that a
/// sign-in used and wasn't finished with isn't used by the next, which registers again. In memory, so a hub that starts
/// again uses one once more before it finds that out.
#[derive(Default)]
pub(super) struct InDoubt {
    by_server: Mutex<HashMap<ToolServerId, Registration>>,
}

impl InDoubt {
    /// Signing in to the server `id` again uses `registration`.
    pub(super) fn used(&self, id: &ToolServerId, registration: &Registration) {
        self.by_server.lock().insert(id.clone(), registration.clone());
    }

    /// Whether a sign-in to the server `id` used `registration` and hasn't been finished.
    pub(super) fn holds(&self, id: &ToolServerId, registration: &Registration) -> bool {
        self.by_server.lock().get(id) == Some(registration)
    }

    /// Nothing is in doubt of the server `id`'s registration any more: it was signed in to, with whichever client, which
    /// is the one it has now, the page sent the person back with the sign-in's own state, which it does only for a client
    /// it knows, or the server was removed.
    pub(super) fn settled(&self, id: &ToolServerId) {
        self.by_server.lock().remove(id);
    }
}
