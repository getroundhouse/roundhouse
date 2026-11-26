//! A hub on this machine as a test holds it, connected to by its socket.

use std::path::{Path, PathBuf};

use super::Hub;
use crate::Client;
use crate::link::{Connect, Link, Lost, Refused};

/// The hub on `socket` as a view holds it, for a test of what a view opens from it against a real hub, with the link
/// that connects to it, which it holds so it goes on connecting for as long as this is held. It connects as the
/// command line does, with no device.
pub struct OnSocket {
    _link: Link<Socket>,
    hub: Hub,
}

impl OnSocket {
    /// Starts connecting, returning at once. The hub has caught up once [`Hub::caught_up`] says so.
    pub fn connect(socket: &Path) -> OnSocket {
        let link = Link::start(Socket(socket.to_path_buf()));
        let hub = link.hub();
        OnSocket { _link: link, hub }
    }

    pub fn hub(&self) -> &Hub {
        &self.hub
    }
}

/// Connects to the hub on a socket, saying why it can't as text.
struct Socket(PathBuf);

impl Connect for Socket {
    type With = ();
    type Failure = String;

    fn connect(&mut self) -> Result<(Client, ()), Refused<String>> {
        Client::connect(&self.0, "test").map(|client| (client, ())).map_err(|e| Refused::by(&e, e.to_string()))
    }

    fn lost(&self, lost: &Lost) -> String {
        lost.to_string()
    }
}
