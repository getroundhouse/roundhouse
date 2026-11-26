//! Holding back the list of a hub's records on a new connection, as a hub that's slow to send it does, for a test of
//! what a client does while a connection catches up, and closing a connection as a read is on its way.

use std::sync::Arc;

use parking_lot::{Condvar, Mutex};

/// While it's held, each connection that starts following the hub is answered, but the list of the hub's records that
/// follows the answer waits until it's let go, so the client's link stays catching up. Every clone shares it, so a test
/// keeps one before the hub is given the world.
#[derive(Clone, Default)]
pub struct HeldList(Arc<Held>);

#[derive(Default)]
struct Held {
    state: Mutex<State>,
    let_go: Condvar,
}

#[derive(Default)]
struct State {
    holding: bool,
    /// How many lists are waiting.
    held: usize,
}

impl HeldList {
    /// Holds back each list of the hub's records from now on, until [`HeldList::let_go`].
    pub fn hold(&self) {
        self.0.state.lock().holding = true;
    }

    /// Sends each list held back, and holds back none from now on.
    pub fn let_go(&self) {
        self.0.state.lock().holding = false;
        self.0.let_go.notify_all();
    }

    /// How many lists are held back now.
    pub fn held(&self) -> usize {
        self.0.state.lock().held
    }

    /// Whether a list sent now is held back, which it's then counted as until it's let go.
    pub(super) fn holds_back(&self) -> bool {
        let mut state = self.0.state.lock();
        state.held += usize::from(state.holding);
        state.holding
    }

    /// Waits until the lists held back are let go.
    pub(super) fn wait(&self) {
        let mut state = self.0.state.lock();
        while state.holding {
            self.0.let_go.wait(&mut state);
        }
        state.held = state.held.saturating_sub(1);
    }
}

/// Once it's armed, the next read of a file the hub keeps closes the connection it's asked on rather than answering, as
/// a connection that goes while a read is on its way does, and the world's [`HeldList`] holds back the list on the
/// next, so the client's new connection stays catching up until the test lets it go. Every clone shares it, as
/// [`HeldList`]'s do.
#[derive(Clone, Default)]
pub struct HangsUp(Arc<std::sync::atomic::AtomicBool>);

impl HangsUp {
    /// Has the next read of a file close its connection.
    pub fn arm(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether this read closes its connection, which only the first after it's armed does.
    pub(super) fn now(&self) -> bool {
        self.0.swap(false, std::sync::atomic::Ordering::SeqCst)
    }
}
