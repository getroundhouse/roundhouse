//! Where an event is in a log, which says the row it's in.

use std::ops::Range;

use protocol::model::{Place, Seq};

/// An event's position in a [`super::SessionLog`]: its `seq` in a log read [`super::By::Seq`], and its place among
/// the events from one agent or subagent in one read [`super::By::Place`]. The first is 1, and there are no gaps. 0 is
/// before the first, where no event is, such as the end of a log that reaches nowhere yet.
///
/// The log makes one from a `seq` or a place by what it reads by, but for a line an older hub listed without its place,
/// which goes by its `seq` in a log by place, as the outline's `row_at` says. A search, in `Job::search`, makes one from
/// each place the hub's find names, since a search reads by place. Only the requests for a log's events turn one back
/// into a `seq` or a place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position(u64);

impl Position {
    /// The position at `seq`, in a log by `seq`.
    pub(crate) const fn of_seq(seq: Seq) -> Self {
        Self(seq.get())
    }

    /// The position at `place`, in a log by place.
    pub(crate) const fn of_place(place: Place) -> Self {
        Self(place.get())
    }

    /// The `seq` it is, in a log by `seq`, for asking the hub for what's there.
    pub(crate) const fn seq(self) -> Seq {
        Seq::new(self.0)
    }

    /// The place it is, in a log by place, for asking the hub for what's there.
    pub(crate) const fn place(self) -> Place {
        Place::new(self.0)
    }

    /// The row the event here is in, or `None` for 0, where no event is.
    pub fn row(self) -> Option<usize> {
        // Every position fits in a row on the 64-bit targets the product builds for.
        usize::try_from(self.0.checked_sub(1)?).ok()
    }

    /// The position of the event in `row`.
    pub fn of_row(row: usize) -> Self {
        // A `usize` is never wider than a `u64` on a target Rust builds for, so only `usize::MAX`, a row no log has,
        // saturates.
        Self(u64::try_from(row).unwrap_or(u64::MAX).saturating_add(1))
    }

    /// The positions of the events in `rows`.
    pub fn of_rows(rows: Range<usize>) -> Range<Self> {
        Self::of_row(rows.start)..Self::of_row(rows.end)
    }

    /// The position after this one.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::*;

    #[test]
    fn nothing_is_at_0_and_the_first_event_is_in_the_first_row() {
        let first = Position::of_seq(Seq::FIRST);

        assert_eq!(Position::default().row(), None);
        assert_eq!(first.row(), Some(0));
        assert_eq!(Position::of_row(0), first);
        assert_eq!(Position::default().next(), first, "and comes after 0");
    }

    #[test]
    fn a_seq_and_a_place_turn_back_into_what_they_were() {
        let (seq, place) = (Position::of_seq(Seq::new(7)), Position::of_place(Place::new(7)));

        assert_eq!((seq.seq(), place.place()), (Seq::new(7), Place::new(7)));
        assert_eq!(seq, place, "the same number is the same position, which only the log can tell apart");
    }

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        #[test]
        fn a_row_s_position_is_in_that_row_and_the_next_is_in_the_next(row in 0..usize::MAX - 1) {
            let position = Position::of_row(row);

            prop_assert_eq!(position.row(), Some(row));
            prop_assert_eq!(position.next(), Position::of_row(row + 1));
        }
    }
}
