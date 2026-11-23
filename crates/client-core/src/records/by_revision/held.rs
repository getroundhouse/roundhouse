use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use protocol::model::Rev;

use super::super::{Kept, Taken};

/// A kind of copy the hub sends whole, with its revision, which a client keeps as [`Rev::supersedes`] says.
pub(in super::super) trait Revised: Clone + PartialEq + Debug {
    type Key: Clone + Eq + Hash + Debug;

    /// Which one it is, or `None` for one of a kind this build doesn't know, which isn't kept.
    fn key(&self) -> Option<Self::Key>;
    fn rev(&self) -> Rev;
    /// Whether a list of everything has every one there is of `key`'s kind, so one it lacks is gone, and it already
    /// says what a copy or a removal no newer than it says.
    fn in_every_list(key: &Self::Key) -> bool;
}

/// The copies held of one kind, and what's been removed.
#[derive(Debug)]
pub(in super::super) struct ByRevision<T: Revised> {
    pub(in super::super) held: HashMap<T::Key, Kept<T>>,
    /// Removed since the newest list, and when, so an older copy that arrives late can't bring one back. One of a kind
    /// no list has stays here, since no list says it's gone.
    removed: HashMap<T::Key, Rev>,
}

impl<T: Revised> Default for ByRevision<T> {
    fn default() -> Self {
        ByRevision { held: HashMap::new(), removed: HashMap::new() }
    }
}

impl<T: Revised> ByRevision<T> {
    /// Takes in what a list read at `at` has of this kind, when no list taken in before is newer. One held that the
    /// list lacks is gone, if its kind is in every list, unless the copy held is newer than the list. Returns whether
    /// anything changed.
    pub(in super::super) fn apply_list(
        &mut self,
        at: Rev,
        list: impl IntoIterator<Item = T>,
        taken: &mut Taken,
    ) -> bool {
        let mut next: HashMap<T::Key, Kept<T>> = HashMap::new();
        for copy in list {
            let Some(key) = copy.key() else { continue };
            if self.removed.get(&key).is_some_and(|&gone| at != Rev::default() && gone > copy.rev()) {
                continue;
            }
            // The copy held, when it's the same, so a view doesn't take it for another.
            let taken = match self.held.get(&key) {
                Some(held) if held.copy == copy => held.taken,
                _ => taken.next(),
            };
            next.insert(key, Kept { copy, taken });
        }
        for (key, held) in &self.held {
            let outlasts_list = !T::in_every_list(key) || (at != Rev::default() && held.copy.rev() > at);
            let newer_than_listed = next.get(key).is_none_or(|listed| !listed.copy.rev().supersedes(held.copy.rev()));
            if outlasts_list && newer_than_listed {
                next.insert(key.clone(), held.clone());
            }
        }
        self.removed.retain(|key, gone| !T::in_every_list(key) || (at != Rev::default() && *gone > at));
        let changed = next != self.held;
        self.held = next;
        changed
    }

    /// Takes in one copy, from a notice or a reply, given the revision of the newest list taken in. Returns whether it
    /// changed anything.
    pub(in super::super) fn apply(&mut self, copy: T, listed: Rev, taken: &mut Taken) -> bool {
        let Some(key) = copy.key() else { return false };
        let rev = copy.rev();
        let removed_since = self.removed.get(&key).is_some_and(|&gone| !rev.supersedes(gone));
        let held = self.held.get(&key).map(|held| &held.copy);
        let held_is_newer = held.is_some_and(|held| !rev.supersedes(held.rev()));
        if listed_already::<T>(&key, rev, listed) || removed_since || held_is_newer || held == Some(&copy) {
            return false;
        }
        self.held.insert(key, Kept { copy, taken: taken.next() });
        true
    }

    /// Takes in that `key` is gone as of `rev`, given the revision of the newest list taken in. Returns whether it
    /// dropped a copy.
    pub(in super::super) fn remove(&mut self, key: &T::Key, rev: Rev, listed: Rev) -> bool {
        if listed_already::<T>(key, rev, listed) {
            return false;
        }
        let gone = self.removed.entry(key.clone()).or_default();
        *gone = (*gone).max(rev);
        if self.held.get(key).is_some_and(|held| rev.supersedes(held.copy.rev())) {
            self.held.remove(key);
            return true;
        }
        false
    }
}

impl<T: Revised> ByRevision<T> {
    /// Lets go of every copy, and every removal, of what `of` says, as of a level a client no longer follows, so
    /// nothing of it is held, and following it again takes in whatever comes. Returns whether it dropped a copy.
    pub(in super::super) fn let_go(&mut self, of: impl Fn(&T::Key) -> bool) -> bool {
        let before = self.held.len();
        self.held.retain(|key, _| !of(key));
        self.removed.retain(|key, _| !of(key));
        self.held.len() != before
    }
}

/// Whether the newest list taken in, at `listed`, already says what a copy or a removal of `key` at `rev` says.
fn listed_already<T: Revised>(key: &T::Key, rev: Rev, listed: Rev) -> bool {
    T::in_every_list(key) && rev != Rev::default() && rev <= listed
}
