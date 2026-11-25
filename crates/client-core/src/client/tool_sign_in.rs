//! The calls that sign in to a tool server: starting a sign-in, finishing it and cancelling it. What a start
//! comes to with no page, and what a refusal means, are worded by [`crate::tool_sign_ins`].

use protocol::client::tool_sign_ins::{
    ToolSignInBegun, ToolSignInCancel, ToolSignInFinish, ToolSignInReturned, ToolSignInStart, ToolSignInTo,
};
use protocol::model::{ToolServer, ToolSignInId};

use super::{Client, Result};

impl Client {
    /// Starts the sign-in `id`, a new one from [`new_tool_sign_in_id`], `to` a tool server, with the person sent back to
    /// `redirect`, where the client listens, and answers with the page to open, or with why there's none. It can take as
    /// long as 30 seconds. See [`ToolSignInStart`].
    pub fn tool_sign_in_start(&self, id: &ToolSignInId, to: ToolSignInTo, redirect: String) -> Result<ToolSignInBegun> {
        self.call(ToolSignInStart { id: id.clone(), to, redirect })
    }

    /// Finishes the sign-in `id` with what the person was sent back with, the whole query, and answers with the server's
    /// record. See [`ToolSignInFinish`].
    pub fn tool_sign_in_finish(&self, id: &ToolSignInId, returned: ToolSignInReturned) -> Result<ToolServer> {
        self.call(ToolSignInFinish { id: id.clone(), returned })
    }

    /// Has the hub forget the sign-in `id`, if it isn't finished. See [`ToolSignInCancel`].
    pub fn tool_sign_in_cancel(&self, id: &ToolSignInId) -> Result<()> {
        self.call(ToolSignInCancel { id: id.clone() })
    }
}

/// A new ID for a sign-in to a tool server, chosen by the client, so sending its start again is the same sign-in.
pub fn new_tool_sign_in_id() -> ToolSignInId {
    ToolSignInId::from(uuid::Uuid::new_v4().to_string())
}
