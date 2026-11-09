//! What `RecordsFeed` and `OpenSession` share: following something a hub serves, on a thread of its own,
//! and telling readers when it changed. When the subscription falls behind, or there's a new connection, it
//! starts again from where it got to.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use super::{Client, ClientError, Ended, Result, Subscription, SubscriptionItem};

/// What's followed, and how.
pub(super) trait Source: Send + 'static {
    type Held: Send + 'static;

    /// Subscribes, and catches up on anything the subscription won't send, such as a list of records. Runs
    /// to start with, after falling behind, and on each new connection.
    fn resume(&mut self, client: &Client, held: &Mutex<Self::Held>) -> Result<Subscription>;

    /// Takes in one item. Returns whether it changed what's held.
    fn apply(&mut self, client: &Client, held: &Mutex<Self::Held>, item: SubscriptionItem) -> Result<bool>;

    /// Does what a reader asked for, such as fetching history. Returns whether it changed what's held.
    fn asked(&mut self, client: &Client, held: &Mutex<Self::Held>) -> Result<bool>;
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

pub(super) struct Shared<H> {
    held: Mutex<H>,
    /// A new connection to carry on with.
    reconnect: Mutex<Option<Client>>,
    connected: AtomicBool,
    errors: Mutex<Vec<FollowError>>,
    /// Holds one signal at a time, so changes nobody has looked at yet gather into one.
    signal: flume::Sender<()>,
}

/// A handle on something followed. Its thread stops once every clone of this has gone.
pub(super) struct Follow<H> {
    shared: Arc<Shared<H>>,
    pub(super) changed: flume::Receiver<()>,
    wake: flume::Sender<()>,
}

impl<H> Clone for Follow<H> {
    fn clone(&self) -> Self {
        Self { shared: self.shared.clone(), changed: self.changed.clone(), wake: self.wake.clone() }
    }
}

impl<H: Send + 'static> Follow<H> {
    /// Starts following on a thread of its own, so this doesn't wait for the hub.
    pub(super) fn start<S: Source<Held = H>>(client: &Client, held: H, source: S) -> Self {
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
        let follower = Follower { source, client: client.clone(), shared: shared.clone(), woken, subscription: None };
        if let Err(e) = std::thread::Builder::new().name("follow hub".to_string()).spawn(move || follower.run()) {
            shared.failed(FollowError::Following(ClientError::Io(e)));
        }
        Self { shared, changed, wake }
    }

    pub(super) fn read<T>(&self, read: impl FnOnce(&H) -> T) -> T {
        read(&self.shared.held.lock())
    }

    /// Changes what's held from here, such as with a reply, and tells readers.
    pub(super) fn update<T>(&self, update: impl FnOnce(&mut H) -> T) -> T {
        let result = update(&mut self.shared.held.lock());
        self.shared.changed();
        result
    }

    /// Changes what's held without telling readers, for a change only the reader making it cares about,
    /// such as taking what changed since it last looked.
    pub(super) fn quietly<T>(&self, change: impl FnOnce(&mut H) -> T) -> T {
        change(&mut self.shared.held.lock())
    }

    /// Has the thread look at what's been asked of it.
    pub(super) fn wake(&self) {
        let _ = self.wake.try_send(());
    }

    /// Carries on on a new connection, which isn't caught up on until the thread says so.
    pub(super) fn reconnect(&self, client: &Client) {
        *self.shared.reconnect.lock() = Some(client.clone());
        self.shared.connected.store(false, Ordering::SeqCst);
        self.wake();
    }

    /// True once it has caught up on a connection, and false from when that connection goes until it has
    /// caught up on a new one.
    pub(super) fn connected(&self) -> bool {
        self.shared.connected.load(Ordering::SeqCst)
    }

    pub(super) fn take_errors(&self) -> Vec<FollowError> {
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
    /// `None` while there's no connection to follow on.
    subscription: Option<Subscription>,
}

#[expect(
    clippy::large_enum_variant,
    reason = "made for each item and matched straight away, so its size costs nothing"
)]
enum Next {
    Item(std::result::Result<SubscriptionItem, flume::RecvError>),
    Woken(std::result::Result<(), flume::RecvError>),
}

impl<S: Source> Follower<S> {
    fn run(mut self) {
        self.resume();
        loop {
            let next = match &self.subscription {
                Some(subscription) => {
                    flume::Selector::new().recv(&subscription.items, Next::Item).recv(&self.woken, Next::Woken).wait()
                }
                None => Next::Woken(self.woken.recv()),
            };
            match next {
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
            }
        }
    }

    fn woken(&mut self) {
        let reconnect = self.shared.reconnect.lock().take();
        if let Some(client) = reconnect {
            self.client = client;
            self.resume();
        }
        // Nothing can be fetched without a connection, so what's asked waits for one.
        if self.subscription.is_some() {
            match self.source.asked(&self.client, &self.shared.held) {
                Ok(true) => self.shared.changed(),
                Ok(false) => {}
                Err(e) => self.shared.failed(FollowError::Asked(e)),
            }
        }
    }

    fn resume(&mut self) {
        self.subscription = None;
        match self.source.resume(&self.client, &self.shared.held) {
            Ok(subscription) => {
                self.subscription = Some(subscription);
                self.shared.set_connected(true);
            }
            Err(e) => self.lost(Some(FollowError::Following(e))),
        }
    }

    fn lost(&mut self, error: Option<FollowError>) {
        self.subscription = None;
        self.shared.connected.store(false, Ordering::SeqCst);
        match error {
            Some(error) => self.shared.failed(error),
            None => self.shared.changed(),
        }
    }
}
