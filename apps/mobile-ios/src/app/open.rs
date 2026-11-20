//! What's open, kept apart from how it's laid out, as a device's view is in "State ownership" in
//! `docs/architecture.md`: a workspace's chat, and a panel over the list of workspaces. The compact layout shows the
//! panel if one's open, then the chat, then the list, a screen at a time with a way back. A wider one would show the
//! list beside the chat from the same, so a change of width opens nothing again.

use gpui_kit::{Entity, Subscription};

use crate::chat::ChatView;

/// A panel over the list of workspaces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Panel {
    /// The hub: how it's reached, and forgetting it.
    Hub,
    /// Starting a workspace in one of the hub's repositories.
    NewWorkspace,
}

/// A workspace's chat that's open, and the subscription to what it asks of the app, for as long as it's open.
pub(super) struct OpenChat {
    pub(super) chat: Entity<ChatView>,
    pub(super) _events: Subscription,
}

/// What's open.
#[derive(Default)]
pub(super) struct Open {
    pub(super) chat: Option<OpenChat>,
    pub(super) panel: Option<Panel>,
}

/// What the compact layout shows of what's open.
pub(super) enum OneAtATime<'a> {
    List,
    Panel(Panel),
    Chat(&'a Entity<ChatView>),
}

impl Open {
    /// What a compact layout shows: the panel over everything, then the chat over the list.
    pub(super) fn one_at_a_time(&self) -> OneAtATime<'_> {
        match (self.panel, &self.chat) {
            (Some(panel), _) => OneAtATime::Panel(panel),
            (None, Some(open)) => OneAtATime::Chat(&open.chat),
            (None, None) => OneAtATime::List,
        }
    }
}
