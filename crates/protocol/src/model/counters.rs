//! Numbers a hub hands out, each a type of its own.

use serde::{Deserialize, Serialize};

/// Declares counters a hub hands out: numbers that only go up, with a type each so that one can't be
/// passed as another. They're plain numbers on the wire and in storage.
macro_rules! counters {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$meta])*
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    )+};
}

counters!(
    /// When a record last changed, from one counter per hub, so a client can tell which of two copies is
    /// newer. A hub that doesn't keep revisions sends none, which reads as 0, and one that does never
    /// uses 0.
    #[derive(Default)]
    Rev,
    /// One launch of an agent. The hub picks it before the launch, and it never repeats, even across
    /// hub restarts, so whatever a run produced can always be told apart from its replacement's. A
    /// later launch has a higher one.
    RunId,
    /// An event's position in its session's log, which the hub gives it as it records it. The first is 1, and there are
    /// no gaps, so 0 is before the first, where no event is. So a bound that reaches no event yet is 0 rather than
    /// `None`, such as how far a session with no events has got, or how far one nobody has read is read.
    /// [`Session::reply_seq`](super::Session::reply_seq) is 0 until a turn has ended, as
    /// [`Session::has_reply`](super::Session::has_reply) says, and any other seq that may name no event is an `Option`.
    #[derive(Default)]
    Seq,
    /// An event's place among the events from the same agent or subagent in its session, so a client can follow one of
    /// them apart from the rest. The first is 1, and there are no gaps. A hub that doesn't record it sends none, which
    /// reads as 0.
    #[derive(Default)]
    Place,
);

/// Gives counters that number a log's events, from 1 with no gaps, the first and the one after each.
macro_rules! log_positions {
    ($($name:ident),+ $(,)?) => {$(
        impl $name {
            /// The first event's.
            pub const FIRST: Self = Self(1);

            /// The one after this one, which after 0 is the first. The last a `u64` holds is the last, rather than
            /// wrapping round to 0, though no log gets that far.
            #[must_use]
            pub const fn next(self) -> Self {
                Self(self.0.saturating_add(1))
            }
        }
    )+};
}

log_positions!(Seq, Place);

impl Rev {
    /// Whether a copy at this revision replaces one at `held`: it's newer, or it has none, and then copies replace each
    /// other in the order they arrive.
    pub fn supersedes(self, held: Rev) -> bool {
        self == Rev::default() || self > held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_event_comes_after_0_and_each_after_the_one_before() {
        assert_eq!((Seq::default().next(), Seq::FIRST.next()), (Seq::FIRST, Seq::new(2)));
        assert_eq!((Place::default().next(), Place::FIRST.next()), (Place::FIRST, Place::new(2)));
        assert_eq!(Seq::new(u64::MAX).next(), Seq::new(u64::MAX), "the last stays the last");
        assert_eq!(Place::new(u64::MAX).next(), Place::new(u64::MAX), "for a place too");
    }
}
