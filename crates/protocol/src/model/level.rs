use serde::{Deserialize, Serialize};

use super::{ReferenceAsk, SessionId, TerminalId, WorkspaceId};

/// What a subscription follows: one of the levels `docs/principles/serving.md` names, and which thing at it. The levels
/// are fixed, and a client can't narrow one by a condition. Each notice on the feed is sent at one level, as
/// [`HubNotice::level`](crate::client::HubNotice::level) says, to the subscriptions that follow it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum Level {
    /// Every record but those of one session, and the observations a sidebar draws for every workspace.
    Hub,
    /// What only a workspace's own pane draws.
    Workspace { id: WorkspaceId },
    /// A session's log, its live snapshot, the records of that session alone and what only its pane draws.
    Session { id: SessionId },
    /// What a terminal shows.
    Terminal { id: TerminalId },
    /// An issue or a pull request, as a chip draws it.
    Reference { reference: ReferenceAsk },
    /// From a newer peer.
    #[serde(other)]
    Unknown,
}
