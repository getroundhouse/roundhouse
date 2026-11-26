//! What a link keeps for every view of its hub, and carries on to each connection it makes.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::Mutex;

use crate::client::Carried;
use protocol::model::DeviceId;

use crate::{Client, Drafts, RecordsFeed};

/// What a [`Link`](crate::link::Link) keeps for every [`Hub`](super::Hub) it gives out, over each connection it makes.
pub(crate) struct Kept {
    /// Started with the link, followed from its first connection, and carried on on every later one, so what's opened
    /// before there's one has the records to take its own into.
    feed: RecordsFeed,
    /// Started with the link, and kept on whichever connection there is, catching up or not, as they've always been.
    drafts: Drafts,
    /// The device the newest connection is on, as the hub takes it, which a chat's conversation is on when its record
    /// names it.
    device: Mutex<Option<DeviceId>>,
    /// What's told each time a new connection catches up, for what waits for one, such as telling the hub how a
    /// reply read out was heard.
    rounds: Mutex<Vec<flume::Sender<()>>>,
    /// Whether there's been a connection, before which the records are no copy of the hub's.
    connected_once: AtomicBool,
    /// Whether the newest connection reached the hub on this machine, by its socket, which where the hub is doesn't
    /// change with its connection, so it holds once that connection has gone.
    on_this_machine: AtomicBool,
    /// The connection, while there's one that has caught up on the records, which is the only one asked anything.
    ready: Mutex<Option<Client>>,
    /// How many connections have caught up.
    round: AtomicU64,
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
    /// What a link that hasn't connected yet keeps.
    pub(crate) fn new() -> Kept {
        Kept {
            feed: RecordsFeed::started(None),
            drafts: Drafts::started(None),
            device: Mutex::new(None),
            rounds: Mutex::default(),
            connected_once: AtomicBool::new(false),
            on_this_machine: AtomicBool::new(false),
            ready: Mutex::new(None),
            round: AtomicU64::new(0),
            opened: Mutex::default(),
        }
    }

    /// The records, once there's been a connection.
    pub(crate) fn records(&self) -> Option<RecordsFeed> {
        self.connected_once.load(Ordering::SeqCst).then(|| self.feed.clone())
    }

    /// The records, whether or not there's been a connection, for what's opened to take its own records into.
    pub(crate) fn feed(&self) -> &RecordsFeed {
        &self.feed
    }

    /// Whether the newest connection reached the hub on this machine, false before there's been one.
    pub(crate) fn on_this_machine(&self) -> bool {
        self.on_this_machine.load(Ordering::SeqCst)
    }

    pub(crate) fn drafts(&self) -> &Drafts {
        &self.drafts
    }

    /// The device the newest connection is on, `None` before there's been one, or for one on no device.
    pub(crate) fn device(&self) -> Option<DeviceId> {
        self.device.lock().clone()
    }

    /// What hears each time a new connection catches up, from now on, while it's held.
    pub(crate) fn rounds(&self) -> flume::Receiver<()> {
        let (tell, told) = flume::bounded(1);
        self.rounds.lock().push(tell);
        told
    }

    pub(crate) fn ready(&self) -> Option<Client> {
        self.ready.lock().clone()
    }

    pub(crate) fn round(&self) -> u64 {
        self.round.load(Ordering::SeqCst)
    }

    /// Carries the records on to `client`, a new connection, then the drafts, then everything open, since what follows
    /// a session takes what it's sent into the records, as [`RecordsFeed::reconnect`] says. Returns the records.
    pub(crate) fn connected(&self, client: &Client) -> RecordsFeed {
        self.feed.reconnect(client);
        self.drafts.reconnect(client);
        *self.device.lock() = client.device().cloned();
        self.connected_once.store(true, Ordering::SeqCst);
        self.on_this_machine.store(client.hub_on_this_machine(), Ordering::SeqCst);
        let mut opened = self.opened.lock();
        opened.connection = Some(client.clone());
        opened.each.retain(|open| open.carry_on(client));
        self.feed.clone()
    }

    /// The connection that has caught up, or `None` while there isn't one. One that has caught up where there was none
    /// starts a new round.
    pub(crate) fn set_ready(&self, ready: Option<Client>) {
        let mut was = self.ready.lock();
        let new_round = was.is_none() && ready.is_some();
        if new_round {
            self.round.fetch_add(1, Ordering::SeqCst);
        }
        *was = ready;
        drop(was);
        if new_round {
            // One that's full has been told already, and one whose receiver has gone is let go of.
            self.rounds.lock().retain(|tell| !matches!(tell.try_send(()), Err(flume::TrySendError::Disconnected(()))));
        }
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
