use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use protocol::model::{Level, Rev};

use super::super::{Kept, Taken};

/// A kind of copy the hub sends whole, with its revision, which a client keeps whichever copy of is newer. Every hub a
/// client of this version reaches gives every copy a revision, as a test of the hub's checks.
pub(in super::super) trait Revised: PartialEq + Debug {
    type Key: Clone + Eq + Hash + Debug;

    /// Which one it is, or `None` for one of a kind this build doesn't know, which isn't kept.
    fn key(&self) -> Option<Self::Key>;
    fn rev(&self) -> Rev;
    /// The level whose list has every one there is of what `key` is of, so a list of that level that lacks one says
    /// it's gone. `None` for one of no level this build can follow, which no list has.
    fn level(key: &Self::Key) -> Option<Level>;
}

/// The copies held of one kind, each shared with what views read, what's been removed, and how far each level's list
/// went.
#[derive(Debug)]
pub(in super::super) struct ByRevision<T: Revised> {
    pub(in super::super) held: HashMap<T::Key, Kept<Arc<T>>>,
    /// Removed since the newest list of its level, as of which revision, so an older copy that arrives late can't bring
    /// one back. One of no level stays here, since no list says it's gone.
    removed: HashMap<T::Key, Kept<Rev>>,
    /// The revision of the newest list taken in of each level listed since it was last let go of. Everything of that
    /// level up to it is already reflected.
    listed: HashMap<Level, Kept<Rev>>,
    /// When the connection copies come on now began. A copy, a removal or a list taken in before it came on an earlier
    /// connection, maybe to another hub, or to the same hub after its revisions started again, as after its database
    /// is replaced from a copy, so its revision says nothing of one taken in since.
    connected: Taken,
}

impl<T: Revised> Default for ByRevision<T> {
    fn default() -> Self {
        ByRevision {
            held: HashMap::new(),
            removed: HashMap::new(),
            listed: HashMap::new(),
            connected: Taken::default(),
        }
    }
}

impl<T: Revised> ByRevision<T> {
    /// Takes in a list of everything at `level`, read at `at`, unless a list of it taken in before on this connection is
    /// newer. One held of that level that the list lacks is gone, unless the copy held came on this connection and is
    /// newer than the list, and one of any other level stays. So the first list of a level on a connection replaces
    /// whatever earlier connections left of it, whatever their revisions. Returns whether anything changed, which the
    /// first list of a level since it was let go of always does, since what's read of it then says the hub sends it.
    pub(in super::super) fn apply_list(
        &mut self,
        level: &Level,
        at: Rev,
        list: impl IntoIterator<Item = T>,
        taken: &mut Taken,
    ) -> bool {
        // Whether it was taken in on this connection. A copy a list repeats keeps when it was first taken in, which can
        // be on an earlier connection, and that's no matter, since a later list on this one is at least as new.
        let this_connection = |taken: Taken| taken >= self.connected;
        let newest = self.listed.get(level).filter(|listed| this_connection(listed.taken)).map(|listed| listed.copy);
        // Older than one already taken in, so everything in it is too.
        if newest.is_some_and(|newest| at < newest) {
            return false;
        }
        let covers = |key: &T::Key| T::level(key).as_ref() == Some(level);
        // Removed on this connection since a copy this list has, so the list read it before it went.
        let removed_since = |key: &T::Key, rev: Rev| {
            self.removed.get(key).is_some_and(|gone| this_connection(gone.taken) && gone.copy > rev)
        };
        let mut next: HashMap<T::Key, Kept<Arc<T>>> = HashMap::new();
        for copy in list {
            let Some(key) = copy.key() else { continue };
            if removed_since(&key, copy.rev()) {
                continue;
            }
            // The copy held, when it's the same, so a view doesn't take it for another.
            let kept = match self.held.get(&key) {
                Some(held) if *held.copy == copy => held.clone(),
                _ => Kept { copy: Arc::new(copy), taken: taken.next() },
            };
            next.insert(key, kept);
        }
        for (key, held) in &self.held {
            let compared = this_connection(held.taken) || !covers(key);
            let outlasts_list = !covers(key) || (compared && held.copy.rev() > at);
            let newer_than_listed = next.get(key).is_none_or(|listed| compared && listed.copy.rev() <= held.copy.rev());
            if outlasts_list && newer_than_listed {
                next.insert(key.clone(), held.clone());
            }
        }
        self.removed.retain(|key, gone| !covers(key) || (this_connection(gone.taken) && gone.copy > at));
        let was_listed = self.listed.contains_key(level);
        self.listed.insert(level.clone(), Kept { copy: newest.unwrap_or_default().max(at), taken: *taken });
        let changed = next != self.held || !was_listed;
        self.held = next;
        changed
    }

    /// Takes in one copy, from a notice or a reply. Returns whether it changed anything.
    pub(in super::super) fn apply(&mut self, copy: T, taken: &mut Taken) -> bool {
        let Some(key) = copy.key() else { return false };
        let rev = copy.rev();
        let removed_since = self.removed.get(&key).is_some_and(|gone| rev <= gone.copy);
        let held = self.held.get(&key).map(|held| &*held.copy);
        let held_is_newer = held.is_some_and(|held| rev <= held.rev());
        if self.listed_already(&key, rev) || removed_since || held_is_newer || held == Some(&copy) {
            return false;
        }
        self.held.insert(key, Kept { copy: Arc::new(copy), taken: taken.next() });
        true
    }

    /// Takes in that `key` is gone as of `rev`. Returns whether it dropped a copy.
    pub(in super::super) fn remove(&mut self, key: &T::Key, rev: Rev, taken: &mut Taken) -> bool {
        if self.listed_already(key, rev) {
            return false;
        }
        let connected = self.connected;
        let gone = self.removed.entry(key.clone()).or_insert(Kept { copy: rev, taken: *taken });
        // One from an earlier connection says nothing of this one's revisions.
        let copy = if gone.taken >= connected { gone.copy.max(rev) } else { rev };
        *gone = Kept { copy, taken: *taken };
        if self.held.get(key).is_some_and(|held| rev > held.copy.rev()) {
            self.held.remove(key);
            return true;
        }
        false
    }

    /// Lets go of every copy and every removal of `level`, and how far its list went, as of a level a client no longer
    /// follows, so nothing of it is held, and following it again takes in whatever comes. Returns whether it dropped a
    /// copy, or a list.
    pub(in super::super) fn let_go(&mut self, level: &Level) -> bool {
        let listed = self.listed.remove(level).is_some();
        self.forget(|key| T::level(key).as_ref() == Some(level)) || listed
    }

    /// Lets go of every copy, and every removal, of what `of` says. Returns whether it dropped a copy.
    pub(in super::super) fn forget(&mut self, of: impl Fn(&T::Key) -> bool) -> bool {
        let before = self.held.len();
        self.held.retain(|key, _| !of(key));
        self.removed.retain(|key, _| !of(key));
        self.held.len() != before
    }

    /// The levels listed since each was last let go of.
    pub(in super::super) fn levels_listed(&self) -> impl Iterator<Item = &Level> {
        self.listed.keys()
    }

    /// Starts a new connection, maybe to another hub, after which the first list of each level replaces what's held of
    /// it, as [`ByRevision::apply_list`] says. `taken` is the count of copies taken in, which this moves on, so that
    /// what's taken in from now on is told apart from what was before.
    pub(in super::super) fn connected_again(&mut self, taken: &mut Taken) {
        self.connected = taken.next();
    }

    /// Whether the newest list of `key`'s level already says what a copy or a removal of it at `rev` says.
    fn listed_already(&self, key: &T::Key, rev: Rev) -> bool {
        let listed = T::level(key).and_then(|level| self.listed.get(&level).map(|listed| listed.copy));
        listed.is_some_and(|listed| rev <= listed)
    }
}
