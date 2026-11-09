//! Connecting to whichever of a hub's addresses answers first, in the manner of Happy Eyeballs (RFC 8305). A link
//! lists the hub's `.local` name first, and looking that up can take seconds, or give an address that never answers,
//! such as an IPv6 one this device can't reach. Trying one address at a time would wait each of those out before
//! the plain IPv4 address. So every name is looked up at once, and a connection to each address starts as soon as
//! it's known, a moment after the one before, and the first to finish a handshake with the pinned key wins.
//!
//! Blocking, with a thread for each lookup and each connection, and a channel to hear from them. The race ends once
//! a connection wins, or every lookup and connection has failed or run out of time. A thread still going then finds
//! nobody listening when it finishes, and what it made is dropped, which closes any connection. A connection's thread
//! lasts at most [`ATTEMPT_TIMEOUT`], and a lookup's as long as the system takes to answer.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

use protocol::pairing::Fingerprint;

use super::tls::{self, Failure};
use crate::client::transport::Transport;

/// How long a name gets to be looked up.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);
/// How long each address gets to connect and finish its handshake, together. Far more than a hub on the same network
/// or tailnet needs, and short enough that a device away from both gives up soon.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long after one connection starts the next may, while the first hasn't failed, as RFC 8305 recommends.
const STAGGER: Duration = Duration::from_millis(250);

/// How a race ended without a connection.
pub(super) struct Lost {
    /// The hosts that answered with a key other than the pinned one, in the order given.
    pub(super) not_pinned: Vec<String>,
    /// Whether every host did, so none is left that might have been the hub.
    pub(super) all_not_pinned: bool,
    /// The last thing that went wrong otherwise.
    pub(super) last: Option<io::Error>,
}

enum Event {
    Found { host: usize, addresses: io::Result<Vec<SocketAddr>> },
    Finished { attempt: usize, result: Result<Transport, Failure> },
}

/// A connection to one of `hosts` that passed the pin.
pub(super) fn race(hosts: &[String], fingerprint: &Fingerprint) -> Result<Transport, Lost> {
    let (events, received) = flume::unbounded();
    let lookups_end = Instant::now() + LOOKUP_TIMEOUT;
    let mut race = Race {
        hosts,
        fingerprint,
        events,
        ready: Vec::new(),
        looking_up: vec![false; hosts.len()],
        attempts: Vec::new(),
        next_start: Instant::now(),
        not_pinned: vec![false; hosts.len()],
        last: None,
    };
    for host in 0..hosts.len() {
        race.look_up(host);
    }
    loop {
        race.start_if_due();
        if race.is_over() {
            return Err(race.lost());
        }
        let event = match race.wait_until(lookups_end) {
            Some(at) => received.recv_deadline(at).ok(),
            None => received.recv().ok(),
        };
        match event {
            Some(Event::Found { host, addresses }) => race.found(host, addresses),
            // Even one that ran out of time here, since it works.
            Some(Event::Finished { result: Ok(transport), .. }) => {
                // Another that also won, and is waiting here, closes now rather than once the last thread ends.
                drop(received.drain());
                return Ok(transport);
            }
            Some(Event::Finished { attempt, result: Err(failure) }) => race.failed(attempt, failure),
            None => race.give_up_on_what_is_late(lookups_end),
        }
    }
}

struct Race<'a> {
    hosts: &'a [String],
    fingerprint: &'a Fingerprint,
    events: flume::Sender<Event>,
    /// Addresses to try, in the order of the hosts they're for, and then the order they were found in.
    ready: Vec<(usize, SocketAddr)>,
    /// Which hosts are still being looked up.
    looking_up: Vec<bool>,
    attempts: Vec<Attempt>,
    /// When the next connection may start, if another is still going.
    next_start: Instant,
    /// Which hosts answered with another key.
    not_pinned: Vec<bool>,
    last: Option<io::Error>,
}

struct Attempt {
    host: usize,
    deadline: Instant,
    going: bool,
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
            Err(e) => self.last = Some(e),
        }
    }

    /// A lookup that answers after it was given up on is ignored.
    fn found(&mut self, host: usize, addresses: io::Result<Vec<SocketAddr>>) {
        if !std::mem::replace(&mut self.looking_up[host], false) {
            return;
        }
        match addresses {
            Ok(addresses) => self.add_ready(host, addresses),
            Err(e) => self.last = Some(e),
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
        let connecting = self.attempts.iter().any(|attempt| attempt.going);
        if self.ready.is_empty() || (connecting && now < self.next_start) {
            return;
        }
        let (host, address) = self.ready.remove(0);
        let (attempt, deadline) = (self.attempts.len(), now + ATTEMPT_TIMEOUT);
        let (events, fingerprint) = (self.events.clone(), self.fingerprint.clone());
        let connecting = std::thread::Builder::new().name("hub-connect".to_string()).spawn(move || {
            let result = tls::connect(address, &fingerprint, deadline);
            // Fails once the race is over, and then what was made is dropped.
            let _ = events.send(Event::Finished { attempt, result });
        });
        match connecting {
            Ok(_) => {
                self.attempts.push(Attempt { host, deadline, going: true });
                self.next_start = now + STAGGER;
            }
            Err(e) => self.last = Some(e),
        }
    }

    /// A connection that fails after it was given up on is ignored.
    fn failed(&mut self, attempt: usize, failure: Failure) {
        let attempt = &mut self.attempts[attempt];
        if !std::mem::replace(&mut attempt.going, false) {
            return;
        }
        match failure {
            Failure::NotPinned => self.not_pinned[attempt.host] = true,
            Failure::Io(e) => self.last = Some(e),
        }
    }

    fn is_over(&self) -> bool {
        self.ready.is_empty() && !self.looking_up.contains(&true) && !self.attempts.iter().any(|attempt| attempt.going)
    }

    /// When to stop waiting to hear something: to start the next connection, or give up on a lookup or connection.
    fn wait_until(&self, lookups_end: Instant) -> Option<Instant> {
        let next_start = (!self.ready.is_empty()).then_some(self.next_start);
        let lookups = self.looking_up.contains(&true).then_some(lookups_end);
        let attempts = self.attempts.iter().filter(|attempt| attempt.going).map(|attempt| attempt.deadline);
        next_start.into_iter().chain(lookups).chain(attempts).min()
    }

    fn give_up_on_what_is_late(&mut self, lookups_end: Instant) {
        let now = Instant::now();
        for (host, looking_up) in self.looking_up.iter_mut().enumerate() {
            if now >= lookups_end && std::mem::replace(looking_up, false) {
                let message = format!("looking up {} took too long", self.hosts[host]);
                self.last = Some(io::Error::new(io::ErrorKind::TimedOut, message));
            }
        }
        for attempt in self.attempts.iter_mut().filter(|attempt| attempt.going && now >= attempt.deadline) {
            attempt.going = false;
            let message = format!("{} didn't finish connecting in time", self.hosts[attempt.host]);
            self.last = Some(io::Error::new(io::ErrorKind::TimedOut, message));
        }
    }

    fn lost(self) -> Lost {
        let not_pinned = self.hosts.iter().zip(&self.not_pinned).filter(|(_, not)| **not);
        Lost {
            not_pinned: not_pinned.map(|(host, _)| host.clone()).collect(),
            all_not_pinned: !self.hosts.is_empty() && !self.not_pinned.contains(&false),
            last: self.last,
        }
    }
}
