/// How a link to a hub stands, for an app to draw, with `F`, its [`Connect`](super::Connect)'s failure, saying why it
/// isn't connected.
#[derive(Clone)]
pub enum LinkState<F> {
    /// Trying to connect. A try the link makes by itself after a failure holds that failure, which an app can go on
    /// showing so it doesn't flash between the two. A try that was asked for, or that follows a connection going,
    /// holds none.
    Connecting(Option<F>),
    /// Connected, and catching up on the hub's records.
    CatchingUp,
    /// Connected, and caught up on the hub's records.
    Ready,
    /// The last try failed, and another comes after a wait.
    Waiting(F),
    /// The last try failed in a way only a person can fix, so none comes until the link is asked to connect again.
    Stopped(F),
}

impl<F> LinkState<F> {
    /// Whether it's connected, and caught up.
    pub fn is_ready(&self) -> bool {
        matches!(self, LinkState::Ready)
    }

    /// Whether it's connected, whether or not it has caught up.
    pub fn is_connected(&self) -> bool {
        matches!(self, LinkState::Ready | LinkState::CatchingUp)
    }
}
