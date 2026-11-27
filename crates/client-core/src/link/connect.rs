//! What an app gives a link: how it connects to its hub, and how it says why it isn't connected.

use crate::{ClientError, Connection, FollowError};

/// How an app connects to one hub, which a [`Link`](super::Link) calls on a thread of its own each time it connects.
/// The iPhone app reaches a paired hub over the network, and the desktop app can also start a hub on this machine,
/// which only it may.
pub trait Connect: Send + 'static {
    /// What the app reads of the hub as it connects, such as its status, handed to the app with the connection.
    type With: Send + 'static;
    /// Why it isn't connected, as the app shows it.
    type Failure: Clone + Send + 'static;

    /// Connects, blocking until it has or can't.
    fn connect(&mut self) -> Result<(Connection, Self::With), Refused<Self::Failure>>;

    /// Why it isn't connected, once the link has given up on a connection, as [`Lost`] says.
    fn lost(&self, lost: &Lost) -> Self::Failure;
}

/// Why a try to connect failed, and whether trying again could fix it.
pub struct Refused<F> {
    pub failure: F,
    /// False for what only a person can fix, so no other try comes until the link is asked to connect again.
    pub again: bool,
}

impl<F> Refused<F> {
    /// Refused with `error`, which is tried again unless [`ClientError::needs_a_person`] says it needs a person.
    pub fn by(error: &ClientError, failure: F) -> Self {
        Refused { failure, again: !error.needs_a_person() }
    }

    /// Refused in a way only a person can fix, such as this machine's own hub failing to start.
    pub fn for_a_person(failure: F) -> Self {
        Refused { failure, again: false }
    }
}

/// Why the link gave up on a connection it had made.
#[derive(Debug, thiserror::Error)]
pub enum Lost {
    /// Following the hub's records failed on it, which it's tried again for unless the failure needs a person.
    #[error(transparent)]
    Following(FollowError),
    /// It hadn't caught up on the records within [`Waits`](super::Waits)' limit. Catching up can fail with nothing to
    /// say, such as when the connection drops part way, so taking too long counts as failing, and is tried again.
    #[error("the hub didn't send its records")]
    CatchingUp,
    /// The link couldn't start the thread it connects on, so it never tries.
    #[error("couldn't start connecting to the hub: {0}")]
    Unstarted(std::io::Error),
}

impl Lost {
    /// Whether trying again could fix it.
    pub(super) fn again(&self) -> bool {
        match self {
            Lost::Following(FollowError::Following(error) | FollowError::Asked(error)) => !error.needs_a_person(),
            Lost::CatchingUp => true,
            Lost::Unstarted(_) => false,
        }
    }
}
