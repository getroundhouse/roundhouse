//! Reading what's held, including runs known to be empty without keeping their events.

use std::ops::Range;
use std::sync::Arc;

use protocol::event::SessionEvent;

use super::{Position, SessionLog};

impl SessionLog {
    /// The known rows, as joined runs in order: event bodies held or fetched empty rows. Every other row is unloaded.
    pub fn loaded_rows(&self) -> Vec<Range<usize>> {
        let events = self.events.keys().filter_map(|position| position.row()).map(|row| row..row + 1);
        let mut held: Vec<_> = self.empty.ranges().chain(events).collect();
        held.sort_unstable_by_key(|run| run.start);
        let mut runs: Vec<Range<usize>> = Vec::new();
        for run in held {
            match runs.last_mut() {
                Some(last) if last.end >= run.start => last.end = last.end.max(run.end),
                Some(_) | None => runs.push(run),
            }
        }
        runs
    }

    /// How many event bodies are held. Empty runs cost only their boundaries.
    pub fn loaded_events(&self) -> usize {
        self.events.len()
    }

    /// The event bodies that are held, oldest first. Permanently empty rows have no body here.
    pub fn loaded(&self) -> impl Iterator<Item = &SessionEvent> {
        self.events.values().map(Arc::as_ref)
    }

    /// Whether every row is known, so searching the loaded event bodies searches everything that can be shown.
    /// One that doesn't know yet how far its events go holds none.
    pub fn holds_every_event(&self) -> bool {
        self.row_count() > 0 && self.holds(0..self.row_count())
    }

    /// Whether every one of `rows` that the log goes to is loaded or known to be empty.
    pub fn holds(&self, rows: Range<usize>) -> bool {
        let rows = rows.start..rows.end.min(self.row_count());
        rows.is_empty()
            || self.events.range(Position::of_rows(rows.clone())).count() + self.empty.count(rows.clone()) == rows.len()
    }
}
