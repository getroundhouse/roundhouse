//! The handle a view holds on its hub.

use std::sync::Arc;

use protocol::client::Request;
use protocol::model::TerminalId;

use super::Kept;
use crate::{ClientError, OpenTerminal, RecordsFeed, Reply};

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
        self.kept.feed()
    }

    /// Whether there's a connection that has caught up on the hub's records, which is the only one
    /// [`Hub::ask`] sends on.
    pub fn caught_up(&self) -> bool {
        self.kept.ready().is_some()
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
        match self.kept.ready() {
            Some(client) => client.start(move |client| client.call(request)),
            None => Reply::now(Err(ClientError::Disconnected)),
        }
    }

    /// Follows terminal `id`, as [`OpenTerminal`] says, on each connection from now on, until every clone of what this
    /// returns has gone.
    pub fn open_terminal(&self, id: &TerminalId) -> OpenTerminal {
        self.kept.open(|connection| OpenTerminal::start(connection, id), OpenTerminal::carried)
    }
}
