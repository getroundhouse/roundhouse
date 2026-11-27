//! The thread that follows something: it subscribes, takes in what comes, does what readers ask, and starts again
//! from where it got to when the subscription falls behind or there's a new connection.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use protocol::client::ErrorCode;

use super::following::{FollowError, Shared, Source};
use crate::client::{ClientError, Connection, Ended, Subscription, SubscriptionItem};

/// Its fields drop in order, so its source, which lets go of what it took in through its subscription, as a session's
/// does of the session's own records, goes before the subscription does.
pub(super) struct Follower<S: Source> {
    pub(super) source: S,
    /// `None` until there's been a connection.
    pub(super) connection: Option<Connection>,
    pub(super) shared: Arc<Shared<S::Held>>,
    pub(super) woken: flume::Receiver<()>,
    /// `None` while there's no connection to follow on, or while it's let go.
    pub(super) subscription: Option<Subscription>,
    /// Not followed, since it isn't wanted, on a connection that hasn't gone as far as it knows.
    pub(super) let_go: bool,
}

enum Next {
    Item(std::result::Result<SubscriptionItem, flume::RecvError>),
    Woken(std::result::Result<(), flume::RecvError>),
    /// When it was wanted until has come.
    Due,
}

impl<S: Source> Follower<S> {
    pub(super) fn run(mut self) {
        self.resume_if_wanted();
        loop {
            match self.next() {
                Next::Item(Ok(item)) => self.apply(item),
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

    /// Takes in an item, which only comes while there's a subscription, so a connection.
    fn apply(&mut self, item: SubscriptionItem) {
        let Some(connection) = &self.connection else { return };
        match self.source.apply(connection, &self.shared.held, item) {
            Ok(changed) => {
                if changed {
                    self.shared.changed();
                }
            }
            Err(e) => self.lost(Some(FollowError::Following(e))),
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
        if let Some(connection) = reconnect {
            // What was asked before the next connection came is done without one, whether or not this has heard the
            // last go, so what that would send the hub, such as what was typed, isn't sent on the next, as typing over
            // a connection that's going isn't. A source that fetches leaves what was asked for the next.
            self.ask(false);
            self.connection = Some(connection);
            self.subscription = None;
            self.source.reconnected();
            self.resume_if_wanted();
        }
        self.ask(self.subscription.is_some() || self.let_go);
    }

    /// Asks the source to do what was asked of it, on the connection if `connected`.
    fn ask(&mut self, connected: bool) {
        let connection = self.connection.as_ref().filter(|_| connected);
        match self.source.asked(connection, &self.shared.held, &|| self.shared.changed()) {
            Ok(true) => self.shared.changed(),
            Ok(false) => {}
            Err(e) => self.shared.failed(FollowError::Asked(e)),
        }
    }

    /// Follows on the connection there is. With none yet, it waits for one, as after a connection goes.
    fn resume(&mut self) {
        self.subscription = None;
        self.let_go = false;
        let Some(connection) = &self.connection else { return };
        match self.source.resume(connection, &self.shared.held) {
            // Of a connection before the one handed over meanwhile, its subscription goes with it, and the thread
            // carries on on the newer once it wakes, as it's been woken to.
            Ok(subscription) => {
                if self.shared.caught_up_unless_newer() {
                    self.subscription = Some(subscription);
                }
            }
            // Deleted, which the records say too, so whoever reads it lets go of it rather than hearing it failed.
            Err(ClientError::Api(e)) if e.code == ErrorCode::NotFound => self.lost(None),
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
