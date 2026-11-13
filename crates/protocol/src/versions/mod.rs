//! Which versions of a protocol each side speaks, and the one two sides use, as "Changing the protocol" in
//! `docs/architecture.md` says. Each protocol has a version of its own: [`crate::client::CLIENT_PROTOCOL_VERSION`],
//! [`crate::worker::WORKER_PROTOCOL_VERSION`] and [`crate::provider::PROVIDER_PROTOCOL_VERSION`].

use std::fmt;

/// The versions of one protocol that a side speaks, from its oldest to its newest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Versions {
    pub oldest: u32,
    pub newest: u32,
}

impl Versions {
    pub const fn only(version: u32) -> Self {
        Self { oldest: version, newest: version }
    }

    /// What a hello names: its newest, and its oldest unless it's from before hellos named a range, when it speaks
    /// only the one.
    pub fn from_hello(newest: u32, oldest: Option<u32>) -> Self {
        Self { oldest: oldest.unwrap_or(newest), newest }
    }

    pub fn speaks(self, version: u32) -> bool {
        self.oldest <= version && version <= self.newest
    }

    /// The newest version both sides speak, which a hello's reply names as the one chosen.
    pub fn newest_shared(self, other: Self) -> Option<u32> {
        let shared = self.newest.min(other.newest);
        (shared >= self.oldest.max(other.oldest)).then_some(shared)
    }
}

/// As a refusal says it, such as "protocol 3" or "protocols 3 to 4".
impl fmt::Display for Versions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.oldest == self.newest {
            write!(f, "protocol {}", self.newest)
        } else {
            write!(f, "protocols {} to {}", self.oldest, self.newest)
        }
    }
}

#[cfg(test)]
mod tests;
