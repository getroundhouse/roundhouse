//! What a link keeps for every view of its hub, and carries on to each connection it makes.

use parking_lot::Mutex;

use crate::client::Carried;
use crate::{Client, RecordsFeed};

/// What a [`Link`](crate::link::Link) keeps for every [`Hub`](super::Hub) it gives out, over each connection it makes.
#[derive(Default)]
pub(crate) struct Kept {
    /// Started on the first connection, and carried on on every later one.
    feed: Mutex<Option<RecordsFeed>>,
    /// The connection, while there's one that has caught up on the records, which is the only one asked anything.
    ready: Mutex<Option<Client>>,
    opened: Mutex<Opened>,
}

/// What views have open, and the newest connection, which each follows on.
#[derive(Default)]
struct Opened {
    /// Whether or not it has caught up, since what's followed starts again from where it stands on it.
    connection: Option<Client>,
    each: Vec<Box<dyn Carried>>,
}

impl Kept {
    pub(crate) fn feed(&self) -> Option<RecordsFeed> {
        self.feed.lock().clone()
    }

    pub(crate) fn ready(&self) -> Option<Client> {
        self.ready.lock().clone()
    }

    /// Carries the records on to `client`, a new connection, starting them on the first, then everything open, since
    /// what follows a session takes what it's sent into the records, as [`RecordsFeed::reconnect`] says. Returns the
    /// records.
    pub(crate) fn connected(&self, client: &Client) -> RecordsFeed {
        let feed = {
            let mut feed = self.feed.lock();
            match &*feed {
                Some(records) => {
                    records.reconnect(client);
                    records.clone()
                }
                None => feed.insert(RecordsFeed::start(client)).clone(),
            }
        };
        let mut opened = self.opened.lock();
        opened.connection = Some(client.clone());
        opened.each.retain(|open| open.carry_on(client));
        feed
    }

    /// The connection that has caught up, or `None` while there isn't one.
    pub(crate) fn set_ready(&self, ready: Option<Client>) {
        *self.ready.lock() = ready;
    }

    /// What `start` opens on the newest connection, or with none if there hasn't been one, carried on to each after it
    /// by what `carried` gives, until every handle on it has gone. It's started under the lock a new connection takes
    /// to carry what's open on to it, so nothing opened as one comes is left on the one before.
    pub(crate) fn open<T>(
        &self,
        start: impl FnOnce(Option<&Client>) -> T,
        carried: impl FnOnce(&T) -> Box<dyn Carried>,
    ) -> T {
        let mut opened = self.opened.lock();
        opened.each.retain(|open| !open.gone());
        let open = start(opened.connection.as_ref());
        opened.each.push(carried(&open));
        open
    }

    /// Lets go of the connections it holds, once the link has gone, so no view keeps one open.
    pub(crate) fn let_go(&self) {
        *self.ready.lock() = None;
        self.opened.lock().connection = None;
    }
}
