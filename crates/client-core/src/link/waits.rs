use std::time::Duration;

/// How long a link waits before trying again after a failure, and for a new connection to catch up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waits {
    /// The wait after the first failure in a row, doubled after each one after it, up to `longest`.
    first: Duration,
    longest: Duration,
    /// How long a new connection has to catch up on the hub's records.
    catch_up: Duration,
}

impl Waits {
    /// Every client's. A phone loses its connection whenever it sleeps or changes networks, so the first try again is
    /// quick, and a hub on a sleeping Mac shouldn't be tried every second all night. A hub that's there sends its records
    /// within moments, so ten seconds is long enough for one that's slow, such as a hub that's just started.
    pub const EVERY_CLIENT: Waits =
        Waits { first: Duration::from_secs(1), longest: Duration::from_secs(30), catch_up: Duration::from_secs(10) };

    /// Tries again after `first`, doubling up to `longest`, for a test that would otherwise sit through the waits.
    /// The limit for catching up stays, since a test only changes a limit when it's about that limit.
    #[cfg(any(test, feature = "testing"))]
    pub const fn retrying(first: Duration, longest: Duration) -> Waits {
        Waits { first, longest, catch_up: Waits::EVERY_CLIENT.catch_up }
    }

    /// With `catch_up` as the limit for catching up, for a test of that limit.
    #[cfg(test)]
    pub(super) const fn catching_up_within(self, catch_up: Duration) -> Waits {
        Waits { catch_up, ..self }
    }

    /// The wait before the next try, after `failures` in a row before this one.
    pub(super) fn before_try(self, failures: u32) -> Duration {
        self.first.saturating_mul(2u32.saturating_pow(failures)).min(self.longest)
    }

    pub(super) fn catch_up(self) -> Duration {
        self.catch_up
    }
}
