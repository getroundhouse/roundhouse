//! The handles on something followed, and what its source does on the thread that follows it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Instant;

use parking_lot::Mutex;

use super::thread::Follower;
use crate::client::{Client, ClientError, Result, Subscription, SubscriptionItem};

/// What's followed, and how. It's made on the thread that follows it, so it needn't be sent between threads.
pub(in crate::client) trait Source: 'static {
    type Held: Send + 'static;

    /// Subscribes, and catches up on anything the subscription won't send, such as a list of records. Runs
    /// to start with, after falling behind, and on each new connection.
    fn resume(&mut self, client: &Client, held: &Mutex<Self::Held>) -> Result<Subscription>;

    /// Takes in one item. Returns whether it changed what's held.
    fn apply(&mut self, client: &Client, held: &Mutex<Self::Held>, item: SubscriptionItem) -> Result<bool>;

    /// Does what a reader asked for, such as fetching history, on `connection`, or without one while there's none, as
    /// a source that does what's asked from what it holds can. Returns whether it changed what's held. `tell` tells
    /// readers of a change made part way, such as a page put in place before the next is fetched, so they see each as
    /// it comes rather than once everything asked for is in.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<Self::Held>, tell: &dyn Fn()) -> Result<bool>;

    /// There's a new connection, which may be to another hub, such as a newer one after a restart. Runs just before
    /// [`Source::resume`] does for it, or before it's let go on it, as [`Source::wanted_until`] says.
    fn reconnected(&mut self) {}

    /// Until when it's followed, after which its subscription is let go, until this says it's wanted again, such as a
    /// terminal nobody is shown. `None`, as by default, is for as long as there's a connection. What it holds stays,
    /// and what readers ask is still done on the connection while it's let go. Following again resumes it, as after
    /// falling behind. Asked after each item, what readers give it, and a new connection.
    fn wanted_until(&self, _held: &Mutex<Self::Held>) -> Option<Instant> {
        None
    }
}

/// What went wrong while following, for a person to hear about. A connection going isn't one of these:
/// following stops until there's a new connection, and carries on from there.
#[derive(Debug, thiserror::Error)]
pub enum FollowError {
    #[error("couldn't follow the hub: {0}")]
    Following(ClientError),
    #[error("couldn't load what was asked for: {0}")]
    Asked(ClientError),
}

pub(in crate::client) struct Shared<H> {
    pub(super) held: Mutex<H>,
    /// A new connection to carry on with.
    pub(super) reconnect: Mutex<Option<Client>>,
    pub(super) connected: AtomicBool,
    errors: Mutex<Vec<FollowError>>,
    /// Holds one signal at a time, so changes nobody has looked at yet gather into one.
    signal: flume::Sender<()>,
}

/// A handle on something followed. Its thread stops once every clone of this has gone.
pub(in crate::client) struct Following<H> {
    shared: Arc<Shared<H>>,
    pub(in crate::client) changed: flume::Receiver<()>,
    wake: flume::Sender<()>,
}

/// Tells the readers of something followed that it changed, from outside it, such as a search through it, without
/// keeping it followed.
#[derive(Clone)]
pub(in crate::client) struct Teller(flume::Sender<()>);

impl Teller {
    pub(in crate::client) fn tell(&self) {
        let _ = self.0.try_send(());
    }
}

impl<H> Clone for Following<H> {
    fn clone(&self) -> Self {
        Self { shared: self.shared.clone(), changed: self.changed.clone(), wake: self.wake.clone() }
    }
}

/// A handle on something followed that doesn't keep it, for changing it from outside for as long as it's there, such
/// as from a connection that takes in what its replies carry.
pub(in crate::client) struct Reach<H>(Weak<Shared<H>>);

impl<H> Clone for Reach<H> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<H> Reach<H> {
    /// Changes what's held, telling readers when `change` says it changed it. Returns false, changing nothing, once
    /// every handle on it and its thread have gone.
    pub(in crate::client) fn change(&self, change: impl FnOnce(&mut H) -> bool) -> bool {
        let Some(shared) = self.0.upgrade() else { return false };
        if change(&mut shared.held.lock()) {
            shared.changed();
        }
        true
    }

    /// What `read` makes of what's held, or `None` once every handle on it and its thread have gone, for a test that
    /// drives a source itself.
    #[cfg(test)]
    pub(in crate::client) fn read<T>(&self, read: impl FnOnce(&H) -> T) -> Option<T> {
        let shared = self.0.upgrade()?;
        Some(read(&shared.held.lock()))
    }

    /// Whether this reaches what `follow` follows.
    pub(in crate::client) fn reaches(&self, follow: &Following<H>) -> bool {
        std::ptr::eq(self.0.as_ptr(), Arc::as_ptr(&follow.shared))
    }
}

/// A handle that gives what's followed something from another thread, such as a reply that thread waited for, and
/// wakes its thread to look, without keeping it followed.
pub(in crate::client) struct Giver<H> {
    shared: Weak<Shared<H>>,
    wake: flume::WeakSender<()>,
}

/// Something followed that carries on over each new connection it's given, for as long as it's there, without being
/// kept followed by what gives it them.
pub(crate) trait Carried: Send {
    /// Carries on on `client`, a new connection, as [`Following::reconnect`] does. Returns false, doing nothing, once
    /// every handle on it has gone.
    fn carry_on(&self, client: &Client) -> bool;

    /// Whether every handle on it has gone, so there's nothing to carry on.
    fn gone(&self) -> bool;
}

impl<H: Send + 'static> Carried for Giver<H> {
    fn carry_on(&self, client: &Client) -> bool {
        let (Some(shared), Some(wake)) = (self.shared.upgrade(), self.wake.upgrade()) else { return false };
        shared.reconnect_on(client);
        let _ = wake.try_send(());
        true
    }

    fn gone(&self) -> bool {
        self.shared.strong_count() == 0
    }
}

impl<H> Giver<H> {
    /// Changes what's held without telling readers, then wakes the thread, as a reader that asks for something does.
    /// Returns false, changing nothing, once every handle on it has gone.
    pub(in crate::client) fn give(&self, change: impl FnOnce(&mut H)) -> bool {
        let (Some(shared), Some(wake)) = (self.shared.upgrade(), self.wake.upgrade()) else { return false };
        change(&mut shared.held.lock());
        let _ = wake.try_send(());
        true
    }
}

impl<H: Send + 'static> Following<H> {
    /// Starts following on a thread of its own, so this doesn't wait for the hub, with the source `source` makes there,
    /// on `connection`, or on the first given to [`Following::reconnect`] if there's none yet.
    pub(in crate::client) fn start<S: Source<Held = H>>(
        connection: Option<&Client>,
        held: H,
        source: impl FnOnce() -> S + Send + 'static,
    ) -> Self {
        Following::unstarted(held).start(connection, source)
    }

    /// What's to be followed, holding `held`, before its thread starts, so what its source needs can be given a handle
    /// on it first.
    pub(in crate::client) fn unstarted(held: H) -> Unstarted<H> {
        let (signal, changed) = flume::bounded(1);
        let (wake, woken) = flume::bounded(1);
        let shared = Arc::new(Shared {
            held: Mutex::new(held),
            reconnect: Mutex::new(None),
            // Until it has caught up for the first time.
            connected: AtomicBool::new(false),
            errors: Mutex::new(Vec::new()),
            signal,
        });
        Unstarted { follow: Following { shared, changed, wake }, woken }
    }

    pub(in crate::client) fn read<T>(&self, read: impl FnOnce(&H) -> T) -> T {
        read(&self.shared.held.lock())
    }

    /// Changes what's held from here, such as with a reply, and tells readers.
    pub(in crate::client) fn update<T>(&self, update: impl FnOnce(&mut H) -> T) -> T {
        let result = update(&mut self.shared.held.lock());
        self.shared.changed();
        result
    }

    /// Changes what's held without telling readers, for a change only the reader making it cares about,
    /// such as taking what changed since it last looked.
    pub(in crate::client) fn quietly<T>(&self, change: impl FnOnce(&mut H) -> T) -> T {
        change(&mut self.shared.held.lock())
    }

    pub(in crate::client) fn teller(&self) -> Teller {
        Teller(self.shared.signal.clone())
    }

    /// A handle on what's held that doesn't keep it followed.
    pub(in crate::client) fn reach(&self) -> Reach<H> {
        Reach(Arc::downgrade(&self.shared))
    }

    /// A handle for giving it what another thread waited for.
    pub(in crate::client) fn giver(&self) -> Giver<H> {
        Giver { shared: Arc::downgrade(&self.shared), wake: self.wake.downgrade() }
    }

    /// Has the thread look at what's been asked of it.
    pub(in crate::client) fn wake(&self) {
        let _ = self.wake.try_send(());
    }

    /// Carries on on a new connection, which isn't caught up on until the thread says so.
    pub(in crate::client) fn reconnect(&self, client: &Client) {
        self.shared.reconnect_on(client);
        self.wake();
    }

    /// True once it has caught up on a connection, and false from when that connection goes until it has
    /// caught up on a new one.
    pub(in crate::client) fn connected(&self) -> bool {
        self.shared.connected.load(Ordering::SeqCst)
    }

    pub(in crate::client) fn take_errors(&self) -> Vec<FollowError> {
        std::mem::take(&mut self.shared.errors.lock())
    }
}

/// What's to be followed, before the thread that follows it has started.
pub(in crate::client) struct Unstarted<H> {
    follow: Following<H>,
    woken: flume::Receiver<()>,
}

impl<H: Send + 'static> Unstarted<H> {
    /// A handle on what's held that doesn't keep it followed, as [`Following::reach`] gives once it's started.
    pub(in crate::client) fn reach(&self) -> Reach<H> {
        self.follow.reach()
    }

    /// What tells its readers that what's held changed, for a test that drives a source itself to see they're told.
    #[cfg(test)]
    pub(in crate::client) fn changed(&self) -> &flume::Receiver<()> {
        &self.follow.changed
    }

    /// Starts following on a thread of its own, as [`Following::start`] does.
    pub(in crate::client) fn start<S: Source<Held = H>>(
        self,
        connection: Option<&Client>,
        source: impl FnOnce() -> S + Send + 'static,
    ) -> Following<H> {
        let Unstarted { follow, woken } = self;
        let (client, shared) = (connection.cloned(), follow.shared.clone());
        let thread = move || {
            let source = source();
            Follower { source, client, shared, woken, subscription: None, let_go: false }.run();
        };
        if let Err(e) = std::thread::Builder::new().name("follow hub".to_string()).spawn(thread) {
            follow.shared.failed(FollowError::Following(ClientError::Io(e)));
        }
        follow
    }
}

impl<H> Shared<H> {
    /// Has the thread carry on on `client` once it next wakes, which it isn't caught up on until the thread says so.
    fn reconnect_on(&self, client: &Client) {
        *self.reconnect.lock() = Some(client.clone());
        self.connected.store(false, Ordering::SeqCst);
    }

    pub(super) fn changed(&self) {
        let _ = self.signal.try_send(());
    }

    pub(super) fn failed(&self, error: FollowError) {
        let (FollowError::Following(cause) | FollowError::Asked(cause)) = &error;
        if !matches!(cause, ClientError::Disconnected) {
            self.errors.lock().push(error);
        }
        self.changed();
    }

    pub(super) fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::SeqCst);
        self.changed();
    }
}
