//! Pairing codes. Each pairs one device, once, until it expires, and they're only ever in memory, so a hub that
//! restarts forgets them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use protocol::pairing::{Device, PairingCode};

#[derive(Default)]
pub(crate) struct Codes {
    codes: HashMap<PairingCode, Code>,
}

struct Code {
    expires_at: Instant,
    state: State,
}

enum State {
    Live,
    /// A device is pairing with it. It can't be used again, whether or not that works.
    Claimed,
    /// Kept until it would have expired, so whoever is waiting on it hears which device it paired, however late
    /// they ask.
    Paired(Device),
}

/// Where a code someone is waiting on has got to.
#[derive(Debug, PartialEq)]
pub(crate) enum Waiting {
    Paired(Device),
    Until(Instant),
    /// It expired, or there never was one.
    Gone,
}

impl Codes {
    pub(crate) fn add(&mut self, code: PairingCode, now: Instant, life: Duration) -> Instant {
        let expires_at = now + life;
        self.codes.insert(code, Code { expires_at, state: State::Live });
        expires_at
    }

    /// Whether a device could pair with this code now.
    pub(crate) fn is_live(&self, code: &PairingCode, now: Instant) -> bool {
        matches!(self.codes.get(code), Some(Code { expires_at, state: State::Live }) if now < *expires_at)
    }

    /// Takes a code for a device to pair with. Only once, and only before it expires.
    pub(crate) fn claim(&mut self, code: &PairingCode, now: Instant) -> bool {
        let live = self.is_live(code, now);
        if let Some(claimed) = self.codes.get_mut(code).filter(|_| live) {
            claimed.state = State::Claimed;
        }
        live
    }

    pub(crate) fn paired(&mut self, code: &PairingCode, device: Device) {
        if let Some(code) = self.codes.get_mut(code) {
            code.state = State::Paired(device);
        }
    }

    pub(crate) fn waiting(&self, code: &PairingCode, now: Instant) -> Waiting {
        match self.codes.get(code) {
            Some(Code { state: State::Paired(device), .. }) => Waiting::Paired(device.clone()),
            Some(Code { expires_at, state: State::Live | State::Claimed }) if now < *expires_at => {
                Waiting::Until(*expires_at)
            }
            Some(_) | None => Waiting::Gone,
        }
    }

    /// Whether a device could still pair with one. One that's being paired counts until it expires, so the hub
    /// doesn't stop listening before the device it's pairing is saved.
    pub(crate) fn any_open(&self, now: Instant) -> bool {
        self.codes.values().any(|code| now < code.expires_at && !matches!(code.state, State::Paired(_)))
    }

    pub(crate) fn forget_expired(&mut self, now: Instant) {
        self.codes.retain(|_, code| now < code.expires_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIFE: Duration = Duration::from_secs(600);

    fn phone() -> Device {
        Device { id: "d".into(), name: "phone".into(), paired_at_ms: 1, last_seen_ms: 1, connections: 0 }
    }

    #[test]
    fn a_code_pairs_once() {
        let (mut codes, now, code) = (Codes::default(), Instant::now(), PairingCode::from("c"));
        codes.add(code.clone(), now, LIFE);

        assert!(codes.claim(&code, now));

        assert!(!codes.claim(&code, now));
        codes.paired(&code, phone());
        assert!(!codes.claim(&code, now));
        assert!(!codes.claim(&PairingCode::from("never made"), now));
    }

    #[test]
    fn a_code_stops_working_when_it_expires() {
        let (mut codes, now, code) = (Codes::default(), Instant::now(), PairingCode::from("c"));
        let expires_at = codes.add(code.clone(), now, LIFE);

        assert!(codes.any_open(expires_at.checked_sub(Duration::from_millis(1)).unwrap()));
        assert!(!codes.any_open(expires_at));
        assert!(!codes.claim(&code, expires_at));
        assert_eq!(codes.waiting(&code, expires_at), Waiting::Gone);
    }

    #[test]
    fn whoever_waits_on_a_code_hears_which_device_paired_until_it_would_have_expired() {
        let (mut codes, now, code) = (Codes::default(), Instant::now(), PairingCode::from("c"));
        let expires_at = codes.add(code.clone(), now, LIFE);
        assert_eq!(codes.waiting(&code, now), Waiting::Until(expires_at));
        assert!(codes.claim(&code, now));
        assert_eq!(codes.waiting(&code, now), Waiting::Until(expires_at));

        codes.paired(&code, phone());

        assert_eq!(codes.waiting(&code, now), Waiting::Paired(phone()));
        assert!(!codes.any_open(now), "a paired code keeps nothing listening");
        codes.forget_expired(expires_at);
        assert_eq!(codes.waiting(&code, expires_at), Waiting::Gone);
    }
}
