//! The thread that connects, and connects again, by the rule in [the module's docs](super).

use std::sync::Arc;
use std::time::Instant;

use super::handle::{Ask, Shared};
use super::{Connect, LinkState, Lost, Refused, Waits};
use crate::{Connection, RecordsFeed};

pub(super) struct Linker<C: Connect> {
    connect: C,
    shared: Arc<Shared<C>>,
    asks: flume::Receiver<Ask<C>>,
    waits: Waits,
    /// What the app sees of the feed, which this listens to on its behalf, telling it of each change.
    feed: Option<RecordsFeed>,
    /// Failures in a row since a connection last caught up, or since the app asked to connect.
    failures: u32,
}

/// Where the link has got to, between tries.
enum Stand<F> {
    /// Connected on `connection`, catching up until `catching_up_until`, or caught up once that's `None`.
    Connected { connection: Connection, catching_up_until: Option<Instant> },
    /// The last try failed with `failure`, and the next is at `until`.
    Waiting { until: Instant, failure: F },
    /// The last try failed in a way only a person can fix.
    Stopped,
}

enum Woke<C: Connect> {
    Asked(Ask<C>),
    /// The feed changed.
    Feed,
    /// The time the link was waiting for came.
    Due,
    /// The app let go of the link.
    Gone,
}

impl<C: Connect> Linker<C> {
    pub(super) fn new(connect: C, shared: Arc<Shared<C>>, asks: flume::Receiver<Ask<C>>, waits: Waits) -> Self {
        Linker { connect, shared, asks, waits, feed: None, failures: 0 }
    }

    pub(super) fn lost(&self, lost: &Lost) -> C::Failure {
        self.connect.lost(lost)
    }

    pub(super) fn run(mut self) {
        let mut stand = self.try_now(None);
        loop {
            let deadline = match &stand {
                Stand::Connected { catching_up_until, .. } => *catching_up_until,
                Stand::Waiting { until, .. } => Some(*until),
                Stand::Stopped => None,
            };
            stand = match self.next(deadline) {
                Woke::Asked(ask) => {
                    #[cfg(any(test, feature = "testing"))]
                    self.shared.kept.until_asks_let_go();
                    // Asked to connect again, it doesn't trust the connection it has, which may have gone without a
                    // word, as a phone's does while it sleeps. One a hub is stopped on goes with the hub, and the
                    // stopping may need it.
                    if !matches!(ask, Ask::After(_)) {
                        let_go(stand);
                    }
                    self.asked(ask)
                }
                Woke::Feed => self.feed_changed(stand),
                Woke::Due => self.due(stand),
                Woke::Gone => return self.shared.kept.let_go(),
            };
        }
    }

    /// Waits for the app to ask something, for the feed to change, or for `deadline`.
    fn next(&self, deadline: Option<Instant>) -> Woke<C> {
        let selector = flume::Selector::new().recv(&self.asks, |ask| ask.map_or(Woke::Gone, Woke::Asked));
        let selector = match &self.feed {
            Some(feed) => selector.recv(feed.changed(), |_| Woke::Feed),
            None => selector,
        };
        match deadline {
            Some(deadline) => selector.wait_deadline(deadline).unwrap_or(Woke::Due),
            None => selector.wait(),
        }
    }

    fn asked(&mut self, ask: Ask<C>) -> Stand<C::Failure> {
        self.failures = 0;
        match ask {
            Ask::Now => self.try_now(None),
            Ask::With(connect) => {
                self.connect = connect;
                self.try_now(None)
            }
            Ask::After(first) => {
                self.set(LinkState::Connecting(None));
                match first(&mut self.connect) {
                    Ok(()) => self.try_now(None),
                    Err(failure) => self.failed(Refused::for_a_person(failure)),
                }
            }
        }
    }

    fn due(&mut self, stand: Stand<C::Failure>) -> Stand<C::Failure> {
        match stand {
            Stand::Waiting { failure, .. } => self.try_now(Some(failure)),
            Stand::Connected { connection, .. } => self.given_up(&connection, &Lost::CatchingUp),
            Stand::Stopped => Stand::Stopped,
        }
    }

    /// Tries to connect, saying it's trying again after `after`, if it is.
    fn try_now(&mut self, after: Option<C::Failure>) -> Stand<C::Failure> {
        self.set(LinkState::Connecting(after));
        let tried = self.connect.connect();
        // What was asked while it tried, such as to connect another way, outdates it.
        if let Ok(ask) = self.asks.try_recv() {
            return self.asked(ask);
        }
        match tried {
            Ok((connection, with)) => self.connected(connection, with),
            Err(refused) => self.failed(refused),
        }
    }

    fn connected(&mut self, connection: Connection, with: C::With) -> Stand<C::Failure> {
        let feed = self.shared.kept.connected(&connection);
        self.feed.get_or_insert(feed);
        *self.shared.connected.lock() = Some(with);
        self.set(LinkState::CatchingUp);
        Stand::Connected { connection, catching_up_until: Some(Instant::now() + self.waits.catch_up()) }
    }

    fn failed(&mut self, refused: Refused<C::Failure>) -> Stand<C::Failure> {
        if !refused.again {
            self.set(LinkState::Stopped(refused.failure));
            return Stand::Stopped;
        }
        let wait = self.waits.before_try(self.failures);
        self.failures = self.failures.saturating_add(1);
        self.set(LinkState::Waiting(refused.failure.clone()));
        Stand::Waiting { until: Instant::now() + wait, failure: refused.failure }
    }

    /// Gives up on `connection`, closing it, so nothing waits on a reply that may never come, such as the feed, which
    /// couldn't take the next connection until it had.
    fn given_up(&mut self, connection: &Connection, lost: &Lost) -> Stand<C::Failure> {
        connection.disconnect();
        self.failed(Refused { failure: self.connect.lost(lost), again: lost.again() })
    }

    /// Tells the app of the change, and goes by what the feed says of the connection.
    fn feed_changed(&mut self, stand: Stand<C::Failure>) -> Stand<C::Failure> {
        let stand = self.connection_says(stand);
        self.shared.tell();
        stand
    }

    fn connection_says(&mut self, stand: Stand<C::Failure>) -> Stand<C::Failure> {
        let Some(feed) = &self.feed else { return stand };
        let errors = feed.take_errors();
        // Otherwise they're of a connection already given up on.
        let Stand::Connected { connection, catching_up_until } = stand else { return stand };
        if feed.connected() {
            if catching_up_until.is_some() {
                self.failures = 0;
                self.set_caught_up(Some(connection.clone()));
                self.set(LinkState::Ready);
            }
            self.shared.errors.lock().extend(errors);
            return Stand::Connected { connection, catching_up_until: None };
        }
        if let Some(error) = errors.into_iter().next() {
            return self.given_up(&connection, &Lost::Following(error));
        }
        // A connection that caught up and isn't now has gone, as has one that closed while it caught up. Anything else
        // the feed says while a new one catches up, such as the last one going, is no reason to drop it.
        if catching_up_until.is_none() || connection.is_disconnected() {
            connection.disconnect();
            return self.try_now(None);
        }
        Stand::Connected { connection, catching_up_until }
    }

    /// The connection that has caught up, or `None` while there isn't one, set before the state that says so.
    fn set_caught_up(&self, ready: Option<Connection>) {
        self.shared.kept.set_ready(ready);
    }

    fn set(&self, state: LinkState<C::Failure>) {
        if !state.is_ready() {
            self.set_caught_up(None);
        }
        *self.shared.state.lock() = state;
        self.shared.tell();
    }
}

/// Closes the connection `stand` has, if it has one.
fn let_go<F>(stand: Stand<F>) {
    if let Stand::Connected { connection, .. } = stand {
        connection.disconnect();
    }
}
