//! Where an event a session records comes from.

use protocol::model::RunId;

/// Where an event comes from, which decides what it may change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// Something the hub knows itself.
    Hub,
    /// One launch of the agent, from it or about it. `current` is false for a leftover from one that's
    /// been replaced, which mustn't change the status of the session its replacement is now running. `numbered` is its
    /// number among what the agent's worker sent, which the session keeps with it, so a hub that connects to the
    /// worker again drops what the worker sends it again.
    Run { run: RunId, current: bool, numbered: Option<u64> },
}

impl Origin {
    pub(super) fn run(self) -> Option<RunId> {
        match self {
            Origin::Hub => None,
            Origin::Run { run, .. } => Some(run),
        }
    }

    pub(super) fn speaks_for_session(self) -> bool {
        match self {
            Origin::Hub => true,
            Origin::Run { current, .. } => current,
        }
    }
}
