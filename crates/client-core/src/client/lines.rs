//! Each chat's line of prompts, so the hub records a chat's prompts in the order they were written, even while an
//! earlier one is still adding a large file and a later one carries none.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use parking_lot::{Condvar, Mutex};
use protocol::model::SessionId;

/// Where each chat's line has got to. A chat with nothing in line has no entry.
#[derive(Default)]
pub(super) struct Lines {
    lines: Mutex<HashMap<SessionId, Line>>,
    /// Told whenever a place leaves, so the next can see whether it's its turn.
    moved: Condvar,
}

#[derive(Default)]
struct Line {
    /// The number the next place gets.
    next: u64,
    /// The first place still in line, whose turn it is.
    first: u64,
    /// Places behind the first that left before their turn, such as when adding a file failed.
    left_early: BTreeSet<u64>,
}

/// A place in a chat's line, taken as its prompt is written. Its turn comes once every place before it has left, and
/// it leaves when it's dropped, whether or not its prompt got through, so one that fails never holds up the rest.
pub(crate) struct Place {
    lines: Arc<Lines>,
    session_id: SessionId,
    number: u64,
}

impl Lines {
    pub(super) fn join(self: &Arc<Self>, session_id: &SessionId) -> Place {
        let mut lines = self.lines.lock();
        let line = lines.entry(session_id.clone()).or_default();
        let number = line.next;
        line.next += 1;
        Place { lines: self.clone(), session_id: session_id.clone(), number }
    }
}

impl Place {
    /// Waits until every place before this one has left. It blocks, as every call does.
    pub(crate) fn wait_turn(&self) {
        let mut lines = self.lines.lines.lock();
        while !Self::is_turn_in(&lines, &self.session_id, self.number) {
            self.lines.moved.wait(&mut lines);
        }
    }

    fn is_turn_in(lines: &HashMap<SessionId, Line>, session_id: &SessionId, number: u64) -> bool {
        lines.get(session_id).is_none_or(|line| line.first == number)
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let mut lines = self.lines.lines.lock();
        if let Some(line) = lines.get_mut(&self.session_id) {
            line.left_early.insert(self.number);
            while line.left_early.remove(&line.first) {
                line.first += 1;
            }
            if line.first == line.next {
                lines.remove(&self.session_id);
            }
        }
        drop(lines);
        self.lines.moved.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_turn(place: &Place) -> bool {
        Place::is_turn_in(&place.lines.lines.lock(), &place.session_id, place.number)
    }

    #[test]
    fn each_place_waits_for_every_one_before_it_in_its_own_chat() {
        let lines = Arc::new(Lines::default());
        let first = lines.join(&"s".into());
        let second = lines.join(&"s".into());
        let elsewhere = lines.join(&"t".into());

        assert!(is_turn(&first) && is_turn(&elsewhere), "another chat's line is its own");
        assert!(!is_turn(&second));
        drop(first);
        assert!(is_turn(&second));
    }

    #[test]
    fn a_place_that_leaves_early_holds_up_nobody() {
        let lines = Arc::new(Lines::default());
        let first = lines.join(&"s".into());
        let second = lines.join(&"s".into());
        let third = lines.join(&"s".into());

        drop(second);
        assert!(!is_turn(&third), "still behind the first");
        drop(first);

        assert!(is_turn(&third));
        drop(third);
        assert!(lines.lines.lock().is_empty(), "an empty line is forgotten");
    }

    #[test]
    fn waiting_for_a_turn_ends_when_the_place_before_leaves() {
        let lines = Arc::new(Lines::default());
        let first = lines.join(&"s".into());
        let second = lines.join(&"s".into());
        let (turned, turn) = flume::bounded(1);

        let waiting = std::thread::spawn(move || {
            second.wait_turn();
            turned.send(()).unwrap();
        });
        drop(first);

        turn.recv_timeout(std::time::Duration::from_secs(10)).expect("the second never got its turn");
        waiting.join().unwrap();
    }
}
