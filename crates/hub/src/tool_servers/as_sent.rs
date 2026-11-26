//! How each tool server is as workers are sent it, with nothing of what it's known by but whether that's the same: what
//! tells whether an agent has a server as it is now.

use std::collections::HashMap;

use protocol::model::{ToolServerId, ToolServerPlace, ToolSignInId};
use protocol::worker::ProxiedServer;

/// The tool servers as workers are sent them. It has no key and no token in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServersAsSent(HashMap<ToolServerId, ServerAsSent>);

/// One of them. A rename leaves it the same, and so does its URL spelled otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServerAsSent {
    place: ToolServerPlace,
    query: Option<String>,
    known_by: KnownBy,
    /// The last change of it that the person saved though the hub couldn't use the server.
    saved_anyway: Option<Change>,
}

/// What it knows the person by, the same until that's another.
#[derive(Clone, Debug, PartialEq, Eq)]
enum KnownBy {
    /// The sign-in a person approved last, which renewing it leaves the same.
    SignIn(ToolSignInId),
    NeedsSignIn,
    /// A key in its header, or nothing, since this change.
    Header(Change),
}

/// Which of the changes to how the servers are known and saved one is, each higher than the one before it, for as long as
/// this hub runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Change(u64);

impl Change {
    fn next(&mut self) -> Change {
        self.0 += 1;
        *self
    }
}

impl ServersAsSent {
    /// How the server `id` is. `None` for one workers aren't sent, and one that needs signing in to, which no agent has
    /// anything to connect to by.
    pub(crate) fn of(&self, id: &ToolServerId) -> Option<&ServerAsSent> {
        self.0.get(id).filter(|server| server.known_by != KnownBy::NeedsSignIn)
    }

    pub(crate) fn set(&mut self, id: ToolServerId, server: ServerAsSent) {
        self.0.insert(id, server);
    }

    /// The server `id` as workers were sent it, and as they're sent it `now`, known by the sign-in `signed_in_by` if it's
    /// signed in to, after what `last` numbers. `saved_anyway` is whether the person saved what has changed though the hub
    /// couldn't use it, which where nothing has is nothing.
    pub(super) fn after(
        &self,
        was: Option<&ProxiedServer>,
        now: &ProxiedServer,
        signed_in_by: Option<ToolSignInId>,
        saved_anyway: bool,
        last: &mut Change,
    ) -> ServerAsSent {
        let before = self.0.get(&now.id);
        let known_by = match (signed_in_by, now.sign_in) {
            (Some(by), _) => KnownBy::SignIn(by),
            (None, Some(_)) => KnownBy::NeedsSignIn,
            (None, None) => match before.map(|before| &before.known_by) {
                Some(KnownBy::Header(since)) if was.is_some_and(|was| was.header == now.header) => {
                    KnownBy::Header(*since)
                }
                Some(KnownBy::Header(_) | KnownBy::SignIn(_) | KnownBy::NeedsSignIn) | None => {
                    KnownBy::Header(last.next())
                }
            },
        };
        let saved_before = before.and_then(|before| before.saved_anyway);
        let query = now.url.query().map(str::to_owned);
        let now = ServerAsSent { place: now.url.place(), query, known_by, saved_anyway: saved_before };
        if saved_anyway && before != Some(&now) {
            return ServerAsSent { saved_anyway: Some(last.next()), ..now };
        }
        now
    }
}

impl ServerAsSent {
    /// Which server it is.
    pub(crate) fn place(&self) -> &ToolServerPlace {
        &self.place
    }

    /// Whether an agent that's connected to it and has had it as `had`, at the same place, has to list its tools again: it
    /// may have others by another query, and may have none where the hub couldn't use it. What it's known by changes none
    /// of them, and goes with the agent's next call.
    pub(crate) fn lists_other_tools_than(&self, had: &ServerAsSent) -> bool {
        self.query != had.query || self.saved_anyway > had.saved_anyway
    }
}
