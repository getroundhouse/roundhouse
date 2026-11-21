//! The handles on something followed, and the thread that follows it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Instant;

use parking_lot::Mutex;
use protocol::client::ErrorCode;

use crate::client::{Client, ClientError, Ended, Result, Subscription, SubscriptionItem};

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

    /// Whether `error`, from [`Source::resume`], is one its readers do without, rather than hear of, as a hub too old
    /// to serve something whose absence the source says itself.
    fn does_without(&self, _error: &ClientError) -> bool {
        false
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
    held: Mutex<H>,
    /// A new connection to carry on with.
    reconnect: Mutex<Option<Client>>,
    connected: AtomicBool,
    errors: Mutex<Vec<FollowError>>,
    /// Holds one signal at a time, so changes nobody has looked at yet gather into one.
    signal: flume::Sender<()>,
}

/// A handle on something followed. Its thread stops once every clone of this has gone.
pub(in crate::client) struct Follow<H> {
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

impl<H> Clone for Follow<H> {
    fn clone(&self) -> Self {
        Self { shared: self.shared.clone(), changed: self.changed.clone(), wake: self.wake.clone() }
    }
}

/// A handle on something followed that doesn't keep it, for changing it from outside for as long as it's there, such
/// as from a connection that takes in what its replies carry.
pub(in crate::client) struct Reach<H>(Weak<Shared<H>>);

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

    /// Whether this reaches what `follow` follows.
    pub(in crate::client) fn reaches(&self, follow: &Follow<H>) -> bool {
        std::ptr::eq(self.0.as_ptr(), Arc::as_ptr(&follow.shared))
    }
}

/// A handle that gives what's followed something from another thread, such as a reply that thread waited for, and
/// wakes its thread to look, without keeping it followed.
pub(in crate::client) struct Giver<H> {
    shared: Weak<Shared<H>>,
    wake: flume::WeakSender<()>,
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

impl<H: Send + 'static> Follow<H> {
    /// Starts following on a thread of its own, so this doesn't wait for the hub, with the source `source` makes there.
    pub(in crate::client) fn start<S: Source<Held = H>>(
        client: &Client,
        held: H,
        source: impl FnOnce() -> S + Send + 'static,
    ) -> Self {
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
        let (client, thread_shared) = (client.clone(), shared.clone());
        let follow = move || {
            let source = source();
            Follower { source, client, shared: thread_shared, woken, subscription: None, let_go: false }.run();
        };
        if let Err(e) = std::thread::Builder::new().name("follow hub".to_string()).spawn(follow) {
            shared.failed(FollowError::Following(ClientError::Io(e)));
        }
        Self { shared, changed, wake }
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
        *self.shared.reconnect.lock() = Some(client.clone());
        self.shared.connected.store(false, Ordering::SeqCst);
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

impl<H> Shared<H> {
    fn changed(&self) {
        let _ = self.signal.try_send(());
    }

    fn failed(&self, error: FollowError) {
        let (FollowError::Following(cause) | FollowError::Asked(cause)) = &error;
        if !matches!(cause, ClientError::Disconnected) {
            self.errors.lock().push(error);
        }
        self.changed();
    }

    fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::SeqCst);
        self.changed();
    }
}

struct Follower<S: Source> {
    source: S,
    client: Client,
    shared: Arc<Shared<S::Held>>,
    woken: flume::Receiver<()>,
    /// `None` while there's no connection to follow on, or while it's let go.
    subscription: Option<Subscription>,
    /// Not followed, since it isn't wanted, on a connection that hasn't gone as far as it knows.
    let_go: bool,
}

enum Next {
    Item(std::result::Result<SubscriptionItem, flume::RecvError>),
    Woken(std::result::Result<(), flume::RecvError>),
    /// When it was wanted until has come.
    Due,
}

impl<S: Source> Follower<S> {
    fn run(mut self) {
        self.resume_if_wanted();
        loop {
            match self.next() {
                Next::Item(Ok(item)) => match self.source.apply(&self.client, &self.shared.held, item) {
                    Ok(changed) => {
                        if changed {
                            self.shared.changed();
                        }
                    }
                    Err(e) => self.lost(Some(FollowError::Following(e))),
                },
                Next::Item(Err(_)) => match self.subscription.take().and_then(|ended| ended.ended()) {
                    Some(Ended::FellBehind) => self.resume(),
                    Some(Ended::Disconnected) | None => self.lost(None),
                },
                Next::Woken(Ok(())) => self.woken(),
                // Every handle has gone.
                Next::Woken(Err(_)) => return,
                Next::Due => {}
            }
            self.let_go_or_resume();
        }
    }

    /// Waits for an item, while there's a subscription, or to be woken, or for when it's wanted until.
    fn next(&mut self) -> Next {
        let selector = flume::Selector::new();
        let selector = match &self.subscription {
            Some(subscription) => selector.recv(&subscription.items, Next::Item),
            None => selector,
        };
        let selector = selector.recv(&self.woken, Next::Woken);
        let due = self.subscription.as_ref().and(self.source.wanted_until(&self.shared.held));
        match due {
            Some(due) => selector.wait_deadline(due).unwrap_or(Next::Due),
            None => selector.wait(),
        }
    }

    /// Whether it's wanted now.
    fn wanted(&self) -> bool {
        self.source.wanted_until(&self.shared.held).is_none_or(|until| Instant::now() < until)
    }

    /// Lets the subscription go once it isn't wanted, and follows again once it's wanted again.
    fn let_go_or_resume(&mut self) {
        if self.subscription.is_some() && !self.wanted() {
            // Dropping it tells the hub to stop sending.
            self.subscription = None;
            self.let_go = true;
        } else if self.let_go && self.wanted() {
            self.resume();
        }
    }

    /// Follows on the connection it has, unless it isn't wanted, when it's let go on it until it is.
    fn resume_if_wanted(&mut self) {
        if self.wanted() {
            self.resume();
        } else {
            self.let_go = true;
        }
    }

    fn woken(&mut self) {
        let reconnect = self.shared.reconnect.lock().take();
        if let Some(client) = reconnect {
            // What was asked before the next connection came is done without one, whether or not this has heard the
            // last go, so what that would send the hub, such as what was typed, isn't sent on the next, as typing over
            // a connection that's going isn't. A source that fetches leaves what was asked for the next.
            self.ask(false);
            self.client = client;
            self.subscription = None;
            self.source.reconnected();
            self.resume_if_wanted();
        }
        self.ask(self.subscription.is_some() || self.let_go);
    }

    /// Asks the source to do what was asked of it, on the connection if `connected`.
    fn ask(&mut self, connected: bool) {
        let connection = connected.then_some(&self.client);
        match self.source.asked(connection, &self.shared.held, &|| self.shared.changed()) {
            Ok(true) => self.shared.changed(),
            Ok(false) => {}
            Err(e) => self.shared.failed(FollowError::Asked(e)),
        }
    }

    fn resume(&mut self) {
        self.subscription = None;
        self.let_go = false;
        match self.source.resume(&self.client, &self.shared.held) {
            Ok(subscription) => {
                self.subscription = Some(subscription);
                self.shared.set_connected(true);
            }
            // Deleted, which the records say too, so whoever reads it lets go of it rather than hearing it failed.
            Err(ClientError::Api(e)) if e.code == ErrorCode::NotFound => self.lost(None),
            Err(e) if self.source.does_without(&e) => self.lost(None),
            Err(e) => self.lost(Some(FollowError::Following(e))),
        }
    }

    fn lost(&mut self, error: Option<FollowError>) {
        self.subscription = None;
        self.let_go = false;
        self.shared.connected.store(false, Ordering::SeqCst);
        match error {
            Some(error) => self.shared.failed(error),
            None => self.shared.changed(),
        }
    }
}
