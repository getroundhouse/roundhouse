//! A listener bound first, so whoever sends the browser back can be told where, and serving once there's something to
//! listen for.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::time::Duration;

use super::answering::{Answer, Asked};
use super::listening::{self, Listening};

/// A listener bound to `127.0.0.1`, not serving yet. Letting go of it frees its port.
pub(crate) struct Bound {
    listener: TcpListener,
    address: SocketAddr,
}

impl Bound {
    /// Binds to `127.0.0.1` only, on a port the system picks.
    pub(crate) fn new() -> io::Result<Bound> {
        Bound::on(0)
    }

    /// Binds to `127.0.0.1` only, on the first of `ports` that's free, and otherwise on one the system picks.
    pub(crate) fn first_free(ports: &[u16]) -> io::Result<Bound> {
        ports.iter().find_map(|port| Bound::on(*port).ok()).map_or_else(Bound::new, Ok)
    }

    fn on(port: u16) -> io::Result<Bound> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        let address = listener.local_addr()?;
        Ok(Bound { listener, address })
    }

    /// Where it's bound.
    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    /// Answers each request as `answers` says, until it says one is what it listened for, the listener is let go of, or
    /// `limit` has passed.
    pub(crate) fn serve<H: Send + 'static>(
        self,
        answers: impl Fn(&Asked) -> Answer<H> + Send + Sync + 'static,
        limit: Duration,
    ) -> io::Result<Listening<H>> {
        listening::start(self.listener, self.address, Box::new(answers), limit)
    }
}
