//! The connection to the hub, made again whenever it goes, and the records client-core keeps from it.

use std::sync::Arc;
use std::time::Duration;

use client_core::records::{HubRecords, Record};
use client_core::{Client, ClientError, RecordsFeed};
use gpui_kit::{AppContext as _, Context, EventEmitter, SharedString, Task};

/// Makes a new connection to the hub. On the phone that's over the network, and in tests a local socket.
pub(crate) type Connect = Arc<dyn Fn() -> Result<Client, ClientError> + Send + Sync>;

/// The first wait before trying again, doubled after each failure up to the longest. A phone loses its
/// connection whenever it sleeps or changes networks, so the first retry is quick, and a hub on a sleeping Mac
/// shouldn't be tried every second all night.
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LONGEST_RETRY: Duration = Duration::from_secs(30);

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
    name: SharedString,
    connect: Connect,
    state: LinkState,
    client: Option<Client>,
    followed: Option<Followed>,
    failures: u32,
    connecting: Option<Task<()>>,
}

/// What client-core keeps of the hub's records, and the task showing it whenever it changes. Made on the first
/// connection, and carried on over every later one.
struct Followed {
    feed: RecordsFeed,
    _showing: Task<()>,
}

impl EventEmitter<Reconnected> for HubLink {}

impl HubLink {
    pub(crate) fn new(name: SharedString, connect: Connect, cx: &mut Context<Self>) -> HubLink {
        let mut link = HubLink {
            name,
            connect,
            state: LinkState::Connecting,
            client: None,
            followed: None,
            failures: 0,
            connecting: None,
        };
        link.connect_now(cx);
        link
    }

    pub(crate) fn name(&self) -> &SharedString {
        &self.name
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
        self.followed.as_ref().map(|followed| followed.feed.read(read))
    }

    /// Passes on a record a reply carried, so what's shown needn't wait for the feed.
    pub(crate) fn take_reply(&self, record: Record) {
        if let Some(followed) = &self.followed {
            followed.feed.take_reply(record);
        }
    }

    /// Connects again now, such as when the app comes back to the front. A phone that slept may hold a
    /// connection that looks open but has gone, so this doesn't trust the one it has.
    pub(crate) fn connect_now(&mut self, cx: &mut Context<Self>) {
        self.state = LinkState::Connecting;
        cx.notify();
        let connect = self.connect.clone();
        self.connecting = Some(cx.spawn(async move |this, cx| {
            let connected = cx.background_spawn(async move { connect() }).await;
            let _ = this.update(cx, |link, cx| link.connected(connected, cx));
        }));
    }

    fn connected(&mut self, connected: Result<Client, ClientError>, cx: &mut Context<Self>) {
        self.connecting = None;
        match connected {
            Ok(client) => {
                if let Some(followed) = &self.followed {
                    followed.feed.reconnect(&client);
                } else {
                    let feed = RecordsFeed::start(&client);
                    self.followed = Some(Followed { feed: feed.clone(), _showing: Self::show(feed, cx) });
                }
                self.client = Some(client.clone());
                cx.emit(Reconnected(client));
            }
            Err(e) => self.unreachable(e.to_string(), cx),
        }
        cx.notify();
    }

    fn unreachable(&mut self, why: String, cx: &mut Context<Self>) {
        self.state = LinkState::Unreachable(why);
        self.client = None;
        let wait = retry_after(self.failures);
        self.failures = self.failures.saturating_add(1);
        self.connecting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, Self::connect_now);
        }));
    }

    /// Shows what client-core holds whenever it changes, and connects again when the connection goes.
    fn show(feed: RecordsFeed, cx: &mut Context<Self>) -> Task<()> {
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
        let errors = feed.take_errors();
        if feed.connected() {
            if self.state != LinkState::Ready && self.connecting.is_none() {
                self.state = LinkState::Ready;
                self.failures = 0;
            }
        } else if self.connecting.is_none() {
            let why = errors.first().map_or_else(|| "the connection closed".to_string(), ToString::to_string);
            self.unreachable(why, cx);
        }
        cx.notify();
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
}
