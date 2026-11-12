//! How many matches each row has, and moving from one match to the next, in the order they're drawn: row by row, and
//! within a row from its top.

use std::collections::BTreeMap;

/// A match, by the row it's drawn in and which of that row's matches it is, counting from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct At {
    pub(crate) row: usize,
    pub(crate) nth: usize,
}

/// The rows with matches, and how many each has.
#[derive(Debug, Default)]
pub(crate) struct Counts {
    rows: BTreeMap<usize, usize>,
}

impl Counts {
    /// Says `row` has `count` matches. None takes it out.
    pub(crate) fn set(&mut self, row: usize, count: usize) {
        if count == 0 {
            self.rows.remove(&row);
        } else {
            self.rows.insert(row, count);
        }
    }

    /// How many matches `row` has.
    pub(crate) fn of(&self, row: usize) -> usize {
        self.rows.get(&row).copied().unwrap_or(0)
    }

    pub(crate) fn total(&self) -> usize {
        self.rows.values().sum()
    }

    pub(crate) fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.rows.keys().copied()
    }

    /// The rows in `range` with matches.
    pub(crate) fn rows_in(&self, range: std::ops::Range<usize>) -> impl Iterator<Item = usize> + '_ {
        self.rows.range(range).map(|(row, _)| *row)
    }

    /// Where `at` is among all the matches, counting from 0.
    pub(crate) fn place(&self, at: At) -> Option<usize> {
        let count = *self.rows.get(&at.row)?;
        (at.nth < count).then(|| self.rows.range(..at.row).map(|(_, count)| count).sum::<usize>() + at.nth)
    }

    /// The match after `from`, or the first after the last, or the first if there's nowhere to go from.
    pub(crate) fn next(&self, from: Option<At>) -> Option<At> {
        let Some(from) = from else { return self.first() };
        match self.rows.get(&from.row) {
            Some(&count) if from.nth + 1 < count => Some(At { row: from.row, nth: from.nth + 1 }),
            Some(_) | None => {
                self.rows.range(from.row + 1..).next().map(|(&row, _)| At { row, nth: 0 }).or(self.first())
            }
        }
    }

    /// The match before `from`, or the last before the first, or the last if there's nowhere to go from.
    pub(crate) fn previous(&self, from: Option<At>) -> Option<At> {
        let Some(from) = from else { return self.last() };
        match self.rows.get(&from.row) {
            Some(_) if from.nth > 0 => Some(At { row: from.row, nth: from.nth.min(self.rows[&from.row]) - 1 }),
            Some(_) | None => self.rows.range(..from.row).next_back().map(|(&row, &count)| At { row, nth: count - 1 }),
        }
        .or(self.last())
    }

    /// The first match in `row` or after it, or the first of all if there's none from there.
    pub(crate) fn first_from(&self, row: usize) -> Option<At> {
        self.rows.range(row..).next().map(|(&row, _)| At { row, nth: 0 }).or(self.first())
    }

    /// `at` once the rows' counts have changed: the same match if it's still there, the row's last if the row has
    /// fewer now, and the next one on if the row has none.
    pub(crate) fn kept(&self, at: At) -> Option<At> {
        match self.rows.get(&at.row) {
            Some(&count) => Some(At { row: at.row, nth: at.nth.min(count - 1) }),
            None => self.first_from(at.row),
        }
    }

    fn first(&self) -> Option<At> {
        self.rows.keys().next().map(|&row| At { row, nth: 0 })
    }

    fn last(&self) -> Option<At> {
        self.rows.iter().next_back().map(|(&row, &count)| At { row, nth: count - 1 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(rows: &[(usize, usize)]) -> Counts {
        let mut counts = Counts::default();
        for &(row, count) in rows {
            counts.set(row, count);
        }
        counts
    }

    #[test]
    fn matches_go_in_order_row_by_row_and_round_again() {
        let counts = counts(&[(3, 2), (9, 1)]);

        assert_eq!(counts.next(None), Some(At { row: 3, nth: 0 }));
        assert_eq!(counts.next(Some(At { row: 3, nth: 0 })), Some(At { row: 3, nth: 1 }));
        assert_eq!(counts.next(Some(At { row: 3, nth: 1 })), Some(At { row: 9, nth: 0 }));
        assert_eq!(
            counts.next(Some(At { row: 9, nth: 0 })),
            Some(At { row: 3, nth: 0 }),
            "after the last comes the first"
        );
        assert_eq!(
            counts.previous(Some(At { row: 3, nth: 0 })),
            Some(At { row: 9, nth: 0 }),
            "and before the first, the last"
        );
        assert_eq!(counts.previous(Some(At { row: 9, nth: 0 })), Some(At { row: 3, nth: 1 }));
        assert_eq!(counts.previous(None), Some(At { row: 9, nth: 0 }));
        assert_eq!(counts.total(), 3);
        assert_eq!(counts.place(At { row: 9, nth: 0 }), Some(2));
        assert_eq!(counts.place(At { row: 9, nth: 1 }), None, "a row has only so many");
    }

    #[test]
    fn finding_starts_at_the_first_match_from_where_the_chat_is() {
        let counts = counts(&[(3, 2), (9, 1)]);

        assert_eq!(counts.first_from(4), Some(At { row: 9, nth: 0 }));
        assert_eq!(counts.first_from(3), Some(At { row: 3, nth: 0 }));
        assert_eq!(counts.first_from(10), Some(At { row: 3, nth: 0 }), "with none below, the first of all");
        assert_eq!(Counts::default().first_from(0), None);
    }

    #[test]
    fn a_match_shown_stays_as_near_as_it_can_when_counts_change() {
        let mut counts = counts(&[(3, 2), (9, 1)]);

        counts.set(3, 1);
        assert_eq!(
            counts.kept(At { row: 3, nth: 1 }),
            Some(At { row: 3, nth: 0 }),
            "the row's last, when it has fewer"
        );
        counts.set(3, 0);
        assert_eq!(counts.kept(At { row: 3, nth: 0 }), Some(At { row: 9, nth: 0 }), "the next on, when it has none");
        assert_eq!(
            counts.previous(Some(At { row: 3, nth: 0 })),
            Some(At { row: 9, nth: 0 }),
            "and from a row with none, back round"
        );
    }
}
