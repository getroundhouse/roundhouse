use std::collections::HashMap;

use protocol::model::{ToolServerId, ToolSignInId};

/// The tool servers that are signed in to, each by the sign-in a person approved last, which renewing it leaves the same.
/// It has no token in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SignedInBy(HashMap<ToolServerId, ToolSignInId>);

impl SignedInBy {
    /// The sign-in the server `id` is signed in to by. `None` for one that needs signing in to, and one with no sign-in.
    pub(crate) fn by(&self, id: &ToolServerId) -> Option<&ToolSignInId> {
        self.0.get(id)
    }

    pub(crate) fn set(&mut self, id: ToolServerId, by: ToolSignInId) {
        self.0.insert(id, by);
    }
}
