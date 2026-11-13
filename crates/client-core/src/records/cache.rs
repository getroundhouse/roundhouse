use super::{snapshot, subagents};
use crate::Agents;
use protocol::client::{HubNotice, RecordNotice};
use protocol::model::{Record, RecordKey, Records, Rev};
use snapshot::{Listed, Snapshot};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;
use subagents::Subagents;

#[derive(Debug, Default)]
pub struct HubRecords {
    pub(super) held: HashMap<RecordKey, Kept<Record>>,
    /// Records removed since the newest list, and when, so an older copy that arrives late can't bring
    /// one back.
    pub(super) removed: HashMap<RecordKey, Rev>,
    /// The newest list taken in. Everything up to it is already reflected.
    pub(super) listed: Rev,
    pub(super) subagents: Subagents,
    /// The newest copy taken in.
    pub(super) taken: Taken,
    /// What views read of the records, made when they next look after a change.
    pub(super) shown: OnceCell<Arc<Listed>>,
    /// As the hub said on the newest connection, shared with every snapshot made since.
    pub(super) agents: Arc<Agents>,
}

/// A copy held, and when it was taken in.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Kept<T> {
    pub(super) record: T,
    pub(super) taken: Taken,
}

/// When a copy was taken in, as a count of the copies taken in before it, so a copy taken in later is newer. It tells
/// views which copies they've already seen, which a record's revision can't, since a hub that doesn't keep revisions
/// gives every copy the same.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Taken(pub(super) u64);

impl Taken {
    /// The next copy's.
    pub(super) fn next(&mut self) -> Taken {
        self.0 += 1;
        *self
    }
}

impl HubRecords {
    /// Takes in a list of every record. A record missing from it is gone, unless the copy held is
    /// newer than the list. Returns whether anything changed.
    pub fn apply_list(&mut self, list: Records) -> bool {
        let at = list.rev;
        // Older than one already taken in, so everything in it is too.
        if at != Rev::default() && at < self.listed {
            return false;
        }
        let mut next: HashMap<RecordKey, Kept<Record>> = HashMap::new();
        for record in list.into_records() {
            let key = record.key();
            if self.removed.get(&key).is_some_and(|&gone| at != Rev::default() && gone > record.rev()) {
                continue;
            }
            // The copy held, when it's the same, so a view doesn't take it for another.
            let taken = match self.held.get(&key) {
                Some(held) if held.record == record => held.taken,
                _ => self.taken.next(),
            };
            next.insert(key, Kept { record, taken });
        }
        for (key, held) in &self.held {
            let newer_than_list = at != Rev::default() && held.record.rev() > at;
            let newer_than_listed =
                next.get(key).is_none_or(|listed| !listed.record.rev().supersedes(held.record.rev()));
            if newer_than_list && newer_than_listed {
                next.insert(key.clone(), held.clone());
            }
        }
        self.removed.retain(|_, gone| at != Rev::default() && *gone > at);
        self.listed = self.listed.max(at);
        let changed = next != self.held;
        self.held = next;
        if changed {
            self.shown.take();
        }
        changed
    }

    /// Takes in what the feed said. Returns whether anything changed. `Resync` changes nothing here:
    /// it asks for a new list.
    pub fn apply_notice(&mut self, notice: HubNotice) -> bool {
        match RecordNotice::try_from(notice) {
            Ok(RecordNotice::Copy(record)) => self.apply_record(record),
            Ok(RecordNotice::Removed { record, rev }) => self.remove(&record, rev),
            Err(_) => false,
        }
    }

    pub(super) fn already_listed(&self, rev: Rev) -> bool {
        rev != Rev::default() && rev <= self.listed
    }

    /// Takes in one record, from a notice or a reply. Returns whether it changed anything.
    pub fn apply_record(&mut self, record: Record) -> bool {
        // Kept apart, since only the subagents of the sessions a view shows are kept, each session's listed on its
        // own rather than with everything, so a list of everything says nothing of them.
        if let Record::Subagent(subagent) = record {
            return self.subagents.apply(subagent);
        }
        let (key, rev) = (record.key(), record.rev());
        let removed_since = self.removed.get(&key).is_some_and(|&gone| !rev.supersedes(gone));
        let held = self.held.get(&key).map(|held| &held.record);
        let held_is_newer = held.is_some_and(|held| !rev.supersedes(held.rev()));
        if self.already_listed(rev) || removed_since || held_is_newer || held == Some(&record) {
            return false;
        }
        let taken = self.taken.next();
        self.held.insert(key, Kept { record, taken });
        self.shown.take();
        true
    }

    pub(super) fn remove(&mut self, key: &RecordKey, rev: Rev) -> bool {
        if self.already_listed(rev) {
            return false;
        }
        let gone = self.removed.entry(key.clone()).or_default();
        *gone = (*gone).max(rev);
        if self.held.get(key).is_some_and(|held| rev.supersedes(held.record.rev())) {
            self.held.remove(key);
            self.shown.take();
            return true;
        }
        false
    }

    /// The records as they are now, for a view to read and keep. It's made once for each change, however many views
    /// ask for it.
    pub fn snapshot(&self) -> Snapshot {
        let records =
            self.shown.get_or_init(|| Arc::new(Listed::of(self.held.values().map(|held| (&held.record, held.taken)))));
        Snapshot::new(records.clone(), self.subagents.listed(), self.agents.clone())
    }

    pub(crate) fn subagents(&self) -> &Subagents {
        &self.subagents
    }

    pub(crate) fn subagents_mut(&mut self) -> &mut Subagents {
        &mut self.subagents
    }

    /// Takes in what the hub said of its agents. Returns whether that changed anything.
    pub(crate) fn set_agents(&mut self, agents: Agents) -> bool {
        let changed = agents != *self.agents;
        if changed {
            self.agents = Arc::new(agents);
        }
        changed
    }
}
