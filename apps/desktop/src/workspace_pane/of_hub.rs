use client_core::{Client, Hub, RecordsFeed};
use conversation::Conversations;
use gpui_kit::Entity;

use crate::tool_sign_in::ToolSigningIn;

/// What a pane is given of its hub's view, which every pane of it shares.
pub(crate) struct OfHub {
    pub(crate) linked: Hub,
    pub(crate) client: Client,
    pub(crate) records: RecordsFeed,
    pub(crate) conversations: Entity<Conversations>,
    pub(crate) tool_sign_in: Entity<ToolSigningIn>,
}
