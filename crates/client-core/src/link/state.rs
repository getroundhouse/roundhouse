use crate::Client;

/// How a link to a hub stands, for an app to draw, with `F`, its [`Connect`](super::Connect)'s failure, saying why it
/// isn't connected.
#[derive(Clone)]
pub enum LinkState<F> {
    /// Trying to connect. A try the link makes by itself after a failure holds that failure, which an app can go on
    /// showing so it doesn't flash between the two. A try that was asked for, or that follows a connection going,
    /// holds none.
    Connecting(Option<F>),
    /// Connected, and catching up on the hub's records.
    CatchingUp(Client),
    /// Connected, and caught up on the hub's records.
    Ready(Client),
    /// The last try failed, and another comes after a wait.
    Waiting(F),
    /// The last try failed in a way only a person can fix, so none comes until the link is asked to connect again.
    Stopped(F),
}

impl<F> LinkState<F> {
    /// The connection, once it has caught up.
    pub fn ready(&self) -> Option<&Client> {
        match self {
            LinkState::Ready(client) => Some(client),
            LinkState::Connecting(_) | LinkState::CatchingUp(_) | LinkState::Waiting(_) | LinkState::Stopped(_) => None,
        }
    }

    /// The connection, whether or not it has caught up.
    pub fn connected(&self) -> Option<&Client> {
        match self {
            LinkState::Ready(client) | LinkState::CatchingUp(client) => Some(client),
            LinkState::Connecting(_) | LinkState::Waiting(_) | LinkState::Stopped(_) => None,
        }
    }
}
