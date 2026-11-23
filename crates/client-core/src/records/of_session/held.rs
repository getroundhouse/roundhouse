use super::super::snapshot::SessionsListed;
use super::super::{Kept, Taken};
use super::OfSession;
use protocol::model::Rev;
use protocol::model::SessionId;
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

/// The records of one kind that the sessions a client follows have.
#[derive(Debug)]
pub(crate) struct BySession<R: OfSession> {
    /// By session, then by record. Each is shared with what views read.
    held: HashMap<SessionId, BTreeMap<R::Id, Kept<Arc<R>>>>,
    /// The sessions followed, each with how many subscriptions follow it, whose records of this kind are kept.
    wanted: HashMap<SessionId, usize>,
    /// The newest copy taken in.
    taken: Taken,
    /// What views read of them, made when they next look after a change.
    shown: OnceCell<Arc<SessionsListed<R>>>,
}

impl<R: OfSession> Default for BySession<R> {
    fn default() -> Self {
        BySession { held: HashMap::new(), wanted: HashMap::new(), taken: Taken::default(), shown: OnceCell::new() }
    }
}

impl<R: OfSession> BySession<R> {
    /// Takes in one record, if its session is one followed. Returns whether it changed anything.
    pub(crate) fn apply(&mut self, record: R) -> bool {
        if !self.wanted.contains_key(record.session()) {
            return false;
        }
        let session = self.held.entry(record.session().clone()).or_default();
        // As for every record. A hub that keeps these records keeps revisions, so a copy without one only comes from a
        // hub that doesn't, which has none to send.
        let held = session.get(&record.id()).map(|held| &held.copy);
        if held.is_some_and(|held| !record.rev().supersedes(held.rev()) || **held == record) {
            return false;
        }
        let taken = self.taken.next();
        session.insert(record.id(), Kept { copy: Arc::new(record), taken });
        self.shown.take();
        true
    }

    /// Takes in where a session's records stand, as its subscription lists them at `at`, if it's still followed. One
    /// held that the list lacks is gone, unless the copy held is newer than the list. Returns whether it changed
    /// anything.
    pub(crate) fn apply_list(&mut self, session: &SessionId, list: Vec<R>, at: Rev) -> bool {
        if !self.wanted.contains_key(session) {
            return false;
        }
        let listed = list.iter().filter(|record| record.session() == session).map(R::id).collect::<BTreeSet<_>>();
        let held = self.held.entry(session.clone()).or_default();
        let before = held.len();
        held.retain(|id, held| listed.contains(id) || (at != Rev::default() && held.copy.rev() > at));
        let mut changed = held.len() != before;
        if changed {
            self.shown.take();
        }
        for record in list.into_iter().filter(|record| record.session() == session) {
            changed |= self.apply(record);
        }
        changed
    }

    /// Keeps the records of `session` while it's followed, once more than before.
    pub(crate) fn want(&mut self, session: SessionId) {
        *self.wanted.entry(session).or_default() += 1;
    }

    /// Follows `session` once less, and lets go of its records once nothing follows it. Returns whether it let go of
    /// any.
    pub(crate) fn forget(&mut self, session: &SessionId) -> bool {
        let Some(wanted) = self.wanted.get_mut(session) else { return false };
        *wanted -= 1;
        if *wanted > 0 {
            return false;
        }
        self.wanted.remove(session);
        let held = self.held.remove(session).is_some();
        if held {
            self.shown.take();
        }
        held
    }

    /// What views read of them now.
    pub(in super::super) fn listed(&self) -> Arc<SessionsListed<R>> {
        let listed = self.shown.get_or_init(|| {
            let sessions = self
                .held
                .iter()
                .map(|(session, its)| (session, its.values().map(|held| (held.copy.clone(), held.taken)).collect()));
            Arc::new(SessionsListed::of(sessions))
        });
        listed.clone()
    }
}
