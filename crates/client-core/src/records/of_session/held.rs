use super::super::snapshot::SessionsListed;
use super::super::{Kept, Taken};
use super::OfSession;
use protocol::model::SessionId;
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

/// The records of one kind that the sessions a view asked for have.
#[derive(Debug)]
pub(crate) struct BySession<R: OfSession> {
    /// By session, then by record. Each is shared with what views read.
    held: HashMap<SessionId, BTreeMap<R::Id, Kept<Arc<R>>>>,
    /// The sessions a view asked for, whose records of this kind are kept.
    wanted: BTreeSet<SessionId>,
    /// Of those, the ones listed on this connection.
    listed: BTreeSet<SessionId>,
    /// The hub is from before this kind had records, so there's nothing to list.
    unkept: bool,
    /// The newest copy taken in.
    taken: Taken,
    /// What views read of them, made when they next look after a change.
    shown: OnceCell<Arc<SessionsListed<R>>>,
}

impl<R: OfSession> Default for BySession<R> {
    fn default() -> Self {
        BySession {
            held: HashMap::new(),
            wanted: BTreeSet::new(),
            listed: BTreeSet::new(),
            unkept: false,
            taken: Taken::default(),
            shown: OnceCell::new(),
        }
    }
}

impl<R: OfSession> BySession<R> {
    /// Takes in one record, if its session is one a view asked for. Returns whether it changed anything.
    pub(crate) fn apply(&mut self, record: R) -> bool {
        if !self.wanted.contains(record.session()) {
            return false;
        }
        let session = self.held.entry(record.session().clone()).or_default();
        // As for every record. A hub that keeps these records keeps revisions, so a copy without one only comes from a
        // hub that doesn't, which has none to send.
        let held = session.get(&record.id()).map(|held| &held.record);
        if held.is_some_and(|held| !record.rev().supersedes(held.rev()) || **held == record) {
            return false;
        }
        let taken = self.taken.next();
        session.insert(record.id(), Kept { record: Arc::new(record), taken });
        self.shown.take();
        true
    }

    /// Takes in a list of a session's records, if a view still wants them. One let go of while it was being listed,
    /// and wanted again since, is listed again. Returns whether it changed anything.
    pub(crate) fn apply_list(&mut self, session: &SessionId, list: Vec<R>) -> bool {
        if !self.wanted.contains(session) {
            return false;
        }
        self.listed.insert(session.clone());
        let mut changed = false;
        for record in list.into_iter().filter(|record| record.session() == session) {
            changed |= self.apply(record);
        }
        changed
    }

    /// Keeps the records of `session` from now on. Returns whether they weren't kept already.
    pub(crate) fn want(&mut self, session: SessionId) -> bool {
        self.wanted.insert(session)
    }

    /// Lets go of the records of `session`, which no view shows any more.
    pub(crate) fn forget(&mut self, session: &SessionId) {
        self.wanted.remove(session);
        self.listed.remove(session);
        if self.held.remove(session).is_some() {
            self.shown.take();
        }
    }

    /// `session` needs no list on this connection, such as when the hub has no such session.
    pub(crate) fn skip(&mut self, session: SessionId) {
        self.listed.insert(session);
    }

    /// The sessions whose records are wanted and haven't been listed on this connection.
    pub(crate) fn to_list(&self) -> Vec<SessionId> {
        if self.unkept {
            return Vec::new();
        }
        self.wanted.difference(&self.listed).cloned().collect()
    }

    /// Lists every wanted session's records again, since notices may have gone missing, such as on a new connection.
    /// That may be to another hub, such as a newer one after a restart, so whether it keeps them is asked again too.
    pub(crate) fn relist(&mut self) {
        self.listed.clear();
        self.unkept = false;
    }

    /// The hub doesn't keep this kind of record.
    pub(crate) fn unkept(&mut self) {
        self.unkept = true;
    }

    /// Whether the hub keeps this kind of record. Until it's been asked, it's taken to.
    #[cfg(test)]
    pub(super) fn kept(&self) -> bool {
        !self.unkept
    }

    /// What views read of them now.
    pub(in super::super) fn listed(&self) -> Arc<SessionsListed<R>> {
        let listed = self.shown.get_or_init(|| {
            let sessions = self
                .held
                .iter()
                .map(|(session, its)| (session, its.values().map(|held| (held.record.clone(), held.taken)).collect()));
            Arc::new(SessionsListed::of(sessions))
        });
        listed.clone()
    }
}
