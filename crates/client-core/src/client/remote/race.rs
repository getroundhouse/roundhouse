//! Connecting to whichever of a hub's addresses answers first, in the manner of Happy Eyeballs (RFC 8305). A link
//! lists the hub's `.local` name first, and looking that up can take seconds, or give an address that never answers,
//! such as an IPv6 one this device can't reach. Trying one address at a time would wait each of those out before
//! the plain IPv4 address. So every name is looked up at once, and a connection to each address starts as soon as
//! it's known, a moment after the one before, and the first to finish a handshake with the pinned key wins.
//!
//! Blocking, with a thread for each lookup and each connection, and a channel to hear from them. A connection that
//! finishes after another has won finds nobody listening, and is dropped, which closes it.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

use super::tls::{self, Failure};
use crate::client::transport::Transport;

/// How long after one connection starts the next may, while the first hasn't failed, as RFC 8305 recommends.
const STAGGER: Duration = Duration::from_millis(250);

/// How a race ended without a connection.
pub(super) struct Lost {
    /// A host that answered with another key, if one did.
    pub(super) not_pinned: Option<String>,
    /// The last thing that went wrong otherwise.
    pub(super) last: Option<io::Error>,
}

enum Event {
    Found { host: usize, addresses: io::Result<Vec<SocketAddr>> },
    Finished { host: usize, result: Result<Transport, Failure> },
}

/// A connection to one of `hosts` that passed the pin. Each lookup, and each connection, gets `timeout`, as does the
/// handshake that follows, so a hub that can't be reached is given up on within a few seconds.
pub(super) fn race(hosts: &[String], fingerprint: &str, timeout: Duration) -> Result<Transport, Lost> {
    let (events, received) = flume::unbounded();
    let lookups_end = Instant::now() + timeout;
    let mut race = Race {
        hosts,
        fingerprint,
        timeout,
        events,
        ready: Vec::new(),
        looking_up: vec![false; hosts.len()],
        connecting: 0,
        next_start: Instant::now(),
        lost: Lost { not_pinned: None, last: None },
    };
    for host in 0..hosts.len() {
        race.look_up(host);
    }
    loop {
        race.start_if_due();
        if race.is_over() {
            return Err(race.lost);
        }
        let event = match race.wait_until(lookups_end) {
            Some(at) => received.recv_deadline(at).ok(),
            None => received.recv().ok(),
        };
        match event {
            Some(Event::Found { host, addresses }) => race.found(host, addresses),
            Some(Event::Finished { result: Ok(transport), .. }) => return Ok(transport),
            Some(Event::Finished { host, result: Err(failure) }) => race.failed(host, failure),
            None => race.give_up_lookups(lookups_end),
        }
    }
}

struct Race<'a> {
    hosts: &'a [String],
    fingerprint: &'a str,
    timeout: Duration,
    events: flume::Sender<Event>,
    /// Addresses to try, in the order of the hosts they're for, and then the order they were found in.
    ready: Vec<(usize, SocketAddr)>,
    /// Which hosts are still being looked up.
    looking_up: Vec<bool>,
    connecting: usize,
    /// When the next connection may start, if another is still going.
    next_start: Instant,
    lost: Lost,
}

impl Race<'_> {
    fn look_up(&mut self, host: usize) {
        if let Ok(address) = self.hosts[host].parse::<SocketAddr>() {
            self.add_ready(host, vec![address]);
            return;
        }
        let (events, name) = (self.events.clone(), self.hosts[host].clone());
        let looking_up = std::thread::Builder::new().name("hub-lookup".to_string()).spawn(move || {
            let addresses = name.to_socket_addrs().map(Iterator::collect);
            let _ = events.send(Event::Found { host, addresses });
        });
        match looking_up {
            Ok(_) => self.looking_up[host] = true,
            Err(e) => self.lost.last = Some(e),
        }
    }

    /// A lookup that answers after it was given up on is ignored.
    fn found(&mut self, host: usize, addresses: io::Result<Vec<SocketAddr>>) {
        if !std::mem::replace(&mut self.looking_up[host], false) {
            return;
        }
        match addresses {
            Ok(addresses) => self.add_ready(host, addresses),
            Err(e) => self.lost.last = Some(e),
        }
    }

    fn add_ready(&mut self, host: usize, addresses: Vec<SocketAddr>) {
        self.ready.extend(addresses.into_iter().map(|address| (host, address)));
        // Stable, so a host's own addresses keep the order the system gave them.
        self.ready.sort_by_key(|(host, _)| *host);
    }

    /// Starts the next connection if none is going, or the last started long enough ago.
    fn start_if_due(&mut self) {
        let now = Instant::now();
        if self.ready.is_empty() || (self.connecting > 0 && now < self.next_start) {
            return;
        }
        let (host, address) = self.ready.remove(0);
        let (events, fingerprint, timeout) = (self.events.clone(), self.fingerprint.to_string(), self.timeout);
        let connecting = std::thread::Builder::new().name("hub-connect".to_string()).spawn(move || {
            let result = tls::connect(address, &fingerprint, timeout);
            // Fails once another connection has won, and then this one is dropped.
            let _ = events.send(Event::Finished { host, result });
        });
        match connecting {
            Ok(_) => {
                self.connecting += 1;
                self.next_start = now + STAGGER;
            }
            Err(e) => self.lost.last = Some(e),
        }
    }

    fn failed(&mut self, host: usize, failure: Failure) {
        self.connecting -= 1;
        match failure {
            Failure::NotPinned => self.lost.not_pinned = Some(self.hosts[host].clone()),
            Failure::Io(e) => self.lost.last = Some(e),
        }
    }

    fn is_over(&self) -> bool {
        self.ready.is_empty() && self.connecting == 0 && !self.looking_up.contains(&true)
    }

    /// When to stop waiting to hear something, to start the next connection or give up on lookups. `None` waits for
    /// a connection that's going, which has its own time limits.
    fn wait_until(&self, lookups_end: Instant) -> Option<Instant> {
        let next_start = (!self.ready.is_empty()).then_some(self.next_start);
        let lookups = self.looking_up.contains(&true).then_some(lookups_end);
        next_start.into_iter().chain(lookups).min()
    }

    fn give_up_lookups(&mut self, lookups_end: Instant) {
        if Instant::now() < lookups_end {
            return;
        }
        for (host, looking_up) in self.looking_up.iter_mut().enumerate() {
            if std::mem::replace(looking_up, false) {
                let message = format!("looking up {} took too long", self.hosts[host]);
                self.lost.last = Some(io::Error::new(io::ErrorKind::TimedOut, message));
            }
        }
    }
}
