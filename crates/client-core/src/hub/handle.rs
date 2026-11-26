//! The handle a view holds on its hub.

use std::sync::Arc;

use protocol::client::Request;
use protocol::model::{AttachmentId, Session, SessionId, TerminalId};

use super::Kept;
use crate::{Client, ClientError, OpenSession, OpenTerminal, RecordsFeed, Reply, Result, View};

/// The hub a view shows, for as long as the pairing lasts, whichever connection its
/// [`Link`](crate::link::Link) has: what a view reads its records from, asks the hub through, and opens what it
/// follows from. Each clone is the same hub.
#[derive(Clone)]
pub struct Hub {
    kept: Arc<Kept>,
}

impl Hub {
    pub(crate) fn of(kept: Arc<Kept>) -> Hub {
        Hub { kept }
    }

    /// The hub's records, once there's been a connection, which carry on over every later one.
    pub fn records(&self) -> Option<RecordsFeed> {
        self.kept.records()
    }

    /// Whether there's a connection that has caught up on the hub's records, which is the only one
    /// [`Hub::ask`] sends on.
    pub fn caught_up(&self) -> bool {
        self.kept.ready().is_some()
    }

    /// Which connection that has caught up this is on, which changes each time a new one does, so a view that keeps
    /// what it asked on one, such as what a chat's agent offers, or that something couldn't be read on it, knows to let
    /// it go. A view reads it whenever it's told something changed, as the records do once a new connection has caught
    /// up.
    pub fn round(&self) -> Round {
        Round(self.kept.round())
    }

    /// Asks the hub `request`, on a thread of its own, so the caller never waits on the hub, and returns at once. It's
    /// sent only on a connection that has caught up on the hub's records, so nothing acts on records the first list on
    /// a new one is about to replace. Before that it fails at once with [`ClientError::Disconnected`], as it does with
    /// no connection, and isn't sent later. Once sent, it runs to the end whatever the caller does next, as
    /// [`Client::start`](crate::Client::start) says, and the record its reply carries is in the records by the time
    /// the reply comes.
    pub fn ask<R>(&self, request: R) -> Reply<R::Reply>
    where
        R: Request + Send + 'static,
        R::Reply: Send + 'static,
    {
        match self.ready() {
            Ok(client) => client.start(move |client| client.call(request)),
            Err(e) => Reply::now(Err(e)),
        }
    }

    /// Asks the hub `request` and waits for its reply, on the caller's thread, for one that can wait, such as one reading
    /// a video a part at a time. It's sent only as [`Hub::ask`] says.
    pub fn call<R: Request>(&self, request: R) -> Result<R::Reply> {
        self.ready()?.call(request)
    }

    /// The bytes of the file `id` in the chat `session_id`, waited for as [`Hub::call`] does, as
    /// [`Client::attachment_read`] reads them.
    pub fn read_attachment(&self, session_id: &SessionId, id: &AttachmentId) -> Result<Vec<u8>> {
        self.ready()?.attachment_read(session_id, id)
    }

    /// The connection that has caught up, or the failure a request meets without one.
    pub(crate) fn ready(&self) -> Result<Client> {
        self.kept.ready().ok_or(ClientError::Disconnected)
    }

    /// Follows the events of `session` that `view` takes, as [`OpenSession`] says, on each connection from now on,
    /// until every clone of what this returns has gone. What's asked through it is sent as [`Hub::ask`] says.
    pub fn open_session(&self, session: &Session, view: View) -> OpenSession {
        let records = self.kept.feed();
        self.kept.open(|connection| OpenSession::start(self, connection, records, session, view), OpenSession::carried)
    }

    /// Follows terminal `id`, as [`OpenTerminal`] says, on each connection from now on, until every clone of what this
    /// returns has gone.
    pub fn open_terminal(&self, id: &TerminalId) -> OpenTerminal {
        self.kept.open(|connection| OpenTerminal::start(connection, id), OpenTerminal::carried)
    }
}

/// Which connection that has caught up a [`Hub`] is on, as [`Hub::round`] says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Round(u64);

#[cfg(any(test, feature = "testing"))]
impl Hub {
    /// A hub with no link, on `client`, taken to have caught up at once, for a test of a view on a stand-in hub that
    /// doesn't send its records, which a link would wait for.
    pub fn caught_up_on(client: &Client) -> Hub {
        let hub = Hub { kept: Arc::new(Kept::new()) };
        hub.kept.connected(client);
        hub.kept.set_ready(Some(client.clone()));
        hub
    }
}

#[cfg(test)]
impl Hub {
    /// Carries what's open on to `client`, as a link does a new connection, and takes it to have caught up at once.
    pub(crate) fn carry_on_to(&self, client: &Client) {
        self.kept.set_ready(None);
        self.kept.connected(client);
        self.kept.set_ready(Some(client.clone()));
    }
}
