//! The handle an app holds on a link, and what it shares with the link's thread.

use std::sync::Arc;

use parking_lot::Mutex;

use super::linker::Linker;
use super::{Connect, LinkState, Lost, Waits};
use crate::hub::Kept;
use crate::{Client, FollowError, Hub};

/// The connection to one hub, made again on a thread of its own by [the rule every client follows](crate::link), and
/// the [`Hub`] every view of it holds, kept over every connection. The thread stops once this has gone, and with it
/// every connection the views' [`Hub`] holds.
pub struct Link<C: Connect> {
    shared: Arc<Shared<C>>,
    changed: flume::Receiver<()>,
    asks: flume::Sender<Ask<C>>,
}

pub(super) struct Shared<C: Connect> {
    pub(super) state: Mutex<LinkState<C::Failure>>,
    /// The records, the connection that has caught up and what's open, for every [`Hub`] this gives out.
    pub(super) kept: Arc<Kept>,
    /// The newest connection the app hasn't taken, with what was read as it was made.
    pub(super) connection: Mutex<Option<(Client, C::With)>>,
    /// What went wrong following the hub's records on a connection that's caught up, for a person to hear about.
    pub(super) errors: Mutex<Vec<FollowError>>,
    /// Holds one signal at a time, so changes nobody has looked at yet gather into one.
    signal: flume::Sender<()>,
}

impl<C: Connect> Shared<C> {
    pub(super) fn tell(&self) {
        let _ = self.signal.try_send(());
    }
}

/// What an app asks of the link's thread.
pub(super) enum Ask<C: Connect> {
    /// Connect now, whatever the link is doing.
    Now,
    /// Connect now, in this way from now on.
    With(C),
    /// Do this, then connect, trying nothing until it's done.
    After(First<C>),
}

/// What's done before connecting again, which fails in a way only a person can fix.
pub(super) type First<C> = Box<dyn FnOnce(&mut C) -> Result<(), <C as Connect>::Failure> + Send>;

impl<C: Connect> Link<C> {
    /// Starts connecting with `connect`, by the waits [every client](Waits::EVERY_CLIENT) has.
    pub fn start(connect: C) -> Link<C> {
        Link::start_waiting(connect, Waits::EVERY_CLIENT)
    }

    /// Starts connecting with `connect`, waiting as `waits` says.
    pub fn start_waiting(connect: C, waits: Waits) -> Link<C> {
        let (signal, changed) = flume::bounded(1);
        let (asks, asked) = flume::unbounded();
        let shared = Arc::new(Shared {
            state: Mutex::new(LinkState::Connecting(None)),
            kept: Arc::new(Kept::new()),
            connection: Mutex::new(None),
            errors: Mutex::new(Vec::new()),
            signal,
        });
        // The connector goes to the thread once it has started, so one that couldn't start can still say why.
        let (give, given) = flume::bounded::<Linker<C>>(1);
        let spawned = std::thread::Builder::new().name("link to hub".to_string()).spawn(move || {
            if let Ok(linker) = given.recv() {
                linker.run();
            }
        });
        let linker = Linker::new(connect, shared.clone(), asked, waits);
        match spawned {
            Ok(_) => {
                let _ = give.send(linker);
            }
            Err(e) => {
                *shared.state.lock() = LinkState::Stopped(linker.lost(&Lost::Unstarted(e)));
                shared.tell();
            }
        }
        Link { shared, changed, asks }
    }

    /// Says that something changed: how the link stands, a new connection, the records, or an error to hear about.
    /// It holds one signal at a time, so changes nobody has looked at yet gather into one. Each signal goes to one
    /// receiver, so one reader listens.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.changed
    }

    pub fn state(&self) -> LinkState<C::Failure> {
        self.shared.state.lock().clone()
    }

    /// The hub this connects to, as every view of it holds it, whichever connection there is.
    pub fn hub(&self) -> Hub {
        Hub::of(self.shared.kept.clone())
    }

    /// The newest connection since this was last asked, with what was read of the hub as it was made, for everything
    /// else the app has open to carry on on. Only the newest, since anything on one before it would only move on again.
    pub fn take_connection(&self) -> Option<(Client, C::With)> {
        self.shared.connection.lock().take()
    }

    /// What was read of the hub as the newest connection since this was last asked was made, such as its status, for
    /// an app that goes by it. Only the newest, since anything of one before it is out of date.
    pub fn take_connected(&self) -> Option<C::With> {
        self.take_connection().map(|(_, with)| with)
    }

    /// What went wrong following the hub's records on a connection that had caught up, which the link carries on with.
    pub fn take_errors(&self) -> Vec<FollowError> {
        std::mem::take(&mut self.shared.errors.lock())
    }

    /// Connects again now, whatever the link is doing, such as when a person asks to try again, or a phone comes back
    /// to the front holding a connection that may have gone without a word. The waits start again from the first.
    pub fn connect_now(&self) {
        let _ = self.asks.send(Ask::Now);
    }

    /// Connects again now, in this way from now on, such as with a token that pairing again renewed.
    pub fn connect_with(&self, connect: C) {
        let _ = self.asks.send(Ask::With(connect));
    }

    /// Does `first` on the link's thread, trying nothing until it's done, then connects, such as stopping a hub so
    /// another can start. If `first` fails, the link waits until it's asked to connect again.
    pub fn connect_after(&self, first: impl FnOnce(&mut C) -> Result<(), C::Failure> + Send + 'static) {
        let _ = self.asks.send(Ask::After(Box::new(first)));
    }
}
