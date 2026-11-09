//! The connection to the hub this phone paired with, made again whenever it goes, and the records client-core keeps
//! from it.

use std::time::Duration;

use client_core::records::{HubRecords, Record};
use client_core::{Client, ClientError, RecordsFeed, RemoteHub};
use gpui_kit::{AppContext as _, Context, EventEmitter, Task};

/// What the hub's list of clients calls the app.
const CLIENT_NAME: &str = "roundhouse-ios";

/// The first wait before trying again, doubled after each failure up to the longest. A phone loses its
/// connection whenever it sleeps or changes networks, so the first retry is quick, and a hub on a sleeping Mac
/// shouldn't be tried every second all night.
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LONGEST_RETRY: Duration = Duration::from_secs(30);
/// How long a new connection has to catch up on the hub's records. Catching up can fail with nothing to report,
/// such as when the connection drops part way, so taking too long counts as failing.
const CATCH_UP_WITHIN: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkState {
    Connecting,
    /// Connected, and client-core has caught up on the hub's records.
    Ready,
    /// The last try failed, with what went wrong. Another is on its way.
    Unreachable(String),
}

/// A new connection, which whatever was following the last one carries on with.
pub(crate) struct Reconnected(pub(crate) Client);

pub(crate) struct HubLink {
    hub: RemoteHub,
    state: LinkState,
    client: Option<Client>,
    records: Option<Records>,
    failures: u32,
    /// A try to connect, or the wait before the next.
    connecting: Option<Task<()>>,
    /// A new connection's deadline to catch up, until it has.
    catching_up: Option<Task<()>>,
}

/// What client-core keeps of the hub's records, and the task following it. Made on the first connection, and
/// carried on over every later one.
struct Records {
    feed: RecordsFeed,
    _following: Task<()>,
}

impl EventEmitter<Reconnected> for HubLink {}

impl HubLink {
    pub(crate) fn new(hub: RemoteHub, cx: &mut Context<Self>) -> HubLink {
        let mut link = HubLink {
            hub,
            state: LinkState::Connecting,
            client: None,
            records: None,
            failures: 0,
            connecting: None,
            catching_up: None,
        };
        link.connect_now(cx);
        link
    }

    pub(crate) fn hub(&self) -> &RemoteHub {
        &self.hub
    }

    pub(crate) fn state(&self) -> &LinkState {
        &self.state
    }

    /// The connection, while there is one that's caught up.
    pub(crate) fn client(&self) -> Option<Client> {
        (self.state == LinkState::Ready).then(|| self.client.clone()).flatten()
    }

    /// What client-core holds of the hub's records, once it has followed them at all.
    pub(crate) fn read<T>(&self, read: impl FnOnce(&HubRecords) -> T) -> Option<T> {
        self.records.as_ref().map(|records| records.feed.read(read))
    }

    /// Passes on a record a reply carried, so what's shown needn't wait for the feed.
    pub(crate) fn take_reply(&self, record: Record) {
        if let Some(records) = &self.records {
            records.feed.take_reply(record);
        }
    }

    /// Connects again now, such as when the app comes back to the front. A phone that slept may hold a
    /// connection that looks open but has gone, so this doesn't trust the one it has.
    pub(crate) fn connect_now(&mut self, cx: &mut Context<Self>) {
        self.state = LinkState::Connecting;
        self.catching_up = None;
        cx.notify();
        let hub = self.hub.clone();
        self.connecting = Some(cx.spawn(async move |this, cx| {
            let connected = cx.background_spawn(async move { Client::connect_remote(&hub, CLIENT_NAME) }).await;
            let _ = this.update(cx, |link, cx| link.connected(connected, cx));
        }));
    }

    fn connected(&mut self, connected: Result<Client, ClientError>, cx: &mut Context<Self>) {
        self.connecting = None;
        match connected {
            Ok(client) => {
                if let Some(records) = &self.records {
                    records.feed.reconnect(&client);
                } else {
                    let feed = RecordsFeed::start(&client);
                    self.records = Some(Records { feed: feed.clone(), _following: Self::follow(feed, cx) });
                }
                self.client = Some(client.clone());
                self.catching_up = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(CATCH_UP_WITHIN).await;
                    let _ = this.update(cx, |link, cx| link.unreachable("the hub didn't send its records".into(), cx));
                }));
                cx.emit(Reconnected(client));
            }
            Err(e) => self.unreachable(e.to_string(), cx),
        }
        cx.notify();
    }

    fn unreachable(&mut self, why: String, cx: &mut Context<Self>) {
        self.state = LinkState::Unreachable(why);
        self.client = None;
        self.catching_up = None;
        let wait = retry_after(self.failures);
        self.failures = self.failures.saturating_add(1);
        self.connecting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, Self::connect_now);
        }));
        cx.notify();
    }

    /// Shows what client-core holds whenever it changes, and connects again when the connection goes.
    fn follow(feed: RecordsFeed, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let changed = feed.changed().clone();
            while changed.recv_async().await.is_ok() {
                if this.update(cx, |link, cx| link.feed_changed(&feed, cx)).is_err() {
                    return;
                }
            }
        })
    }

    fn feed_changed(&mut self, feed: &RecordsFeed, cx: &mut Context<Self>) {
        let error = feed.take_errors().first().map(ToString::to_string);
        // A try under way decides what comes next.
        if self.connecting.is_none() {
            match feed_says(feed.connected(), error, self.catching_up.is_some()) {
                FeedSays::CaughtUp if self.state != LinkState::Ready => {
                    self.state = LinkState::Ready;
                    self.failures = 0;
                    self.catching_up = None;
                }
                FeedSays::CaughtUp | FeedSays::Nothing => {}
                FeedSays::Lost(why) => self.unreachable(why, cx),
            }
        }
        cx.notify();
    }
}

/// What a change in the feed says of the connection.
#[derive(Debug, PartialEq, Eq)]
enum FeedSays {
    CaughtUp,
    Lost(String),
    Nothing,
}

fn feed_says(connected: bool, error: Option<String>, catching_up: bool) -> FeedSays {
    match (connected, error) {
        (true, _) => FeedSays::CaughtUp,
        (false, Some(error)) => FeedSays::Lost(error),
        // While a new connection catches up, the feed isn't connected yet, and anything else it says, such as a
        // reply taken in, or the last connection going, is no reason to drop it.
        (false, None) if catching_up => FeedSays::Nothing,
        // Caught up, and now not, so the connection went.
        (false, None) => FeedSays::Lost("the connection closed".into()),
    }
}

fn retry_after(failures: u32) -> Duration {
    FIRST_RETRY.saturating_mul(2u32.saturating_pow(failures)).min(LONGEST_RETRY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_double_up_to_the_longest_wait() {
        let waits: Vec<u64> = (0..8).map(|failures| retry_after(failures).as_secs()).collect();

        assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn a_new_connection_is_given_until_it_catches_up() {
        assert_eq!(feed_says(false, None, true), FeedSays::Nothing);
        assert_eq!(feed_says(true, None, true), FeedSays::CaughtUp);
        assert_eq!(feed_says(false, Some("refused".into()), true), FeedSays::Lost("refused".into()));
    }

    #[test]
    fn a_connection_that_caught_up_is_lost_when_the_feed_stops_being_connected() {
        assert_eq!(feed_says(false, None, false), FeedSays::Lost("the connection closed".into()));
        assert_eq!(feed_says(true, None, false), FeedSays::CaughtUp);
    }
}
