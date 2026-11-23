use super::super::Taken;
use super::super::by_revision::ByRevision;
use super::super::snapshot::SessionsListed;
use super::OfSession;
use protocol::model::{Level, Rev, SessionId};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;

/// The records of one kind that the sessions a client follows have.
#[derive(Debug)]
pub(crate) struct BySession<R: OfSession> {
    /// Each session's, at its session's level. Each is shared with what views read.
    held: ByRevision<R>,
    /// The sessions followed, each with how many subscriptions follow it, whose records of this kind are kept.
    wanted: HashMap<SessionId, usize>,
    /// The newest copy taken in.
    taken: Taken,
    /// What views read of them, made when they next look after a change.
    shown: OnceCell<Arc<SessionsListed<R>>>,
}

impl<R: OfSession> Default for BySession<R> {
    fn default() -> Self {
        BySession {
            held: ByRevision::default(),
            wanted: HashMap::new(),
            taken: Taken::default(),
            shown: OnceCell::new(),
        }
    }
}

impl<R: OfSession> BySession<R> {
    /// Takes in one record, if its session is one followed. Returns whether it changed anything.
    pub(crate) fn apply(&mut self, record: R) -> bool {
        if !self.wanted.contains_key(record.session()) {
            return false;
        }
        self.changed(|held, taken| held.apply(record, taken))
    }

    /// Takes in where a session's records stand, as its subscription lists them at `at`, if it's still followed, as
    /// [`ByRevision::apply_list`] says. Returns whether it changed anything.
    pub(crate) fn apply_list(&mut self, session: &SessionId, list: Vec<R>, at: Rev) -> bool {
        if !self.wanted.contains_key(session) {
            return false;
        }
        let list = list.into_iter().filter(|record| record.session() == session);
        let level = Level::Session { id: session.clone() };
        self.changed(|held, taken| held.apply_list(&level, at, list, taken))
    }

    /// Takes in that `session`'s record `id` is gone as of `rev`, if it's still followed. Returns whether it dropped a
    /// copy.
    pub(crate) fn remove(&mut self, session: &SessionId, id: R::Id, rev: Rev) -> bool {
        if !self.wanted.contains_key(session) {
            return false;
        }
        self.changed(|held, taken| held.remove(&(session.clone(), id), rev, taken))
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
        self.changed(|held, _| held.let_go(&Level::Session { id: session.clone() }))
    }

    /// Starts a new connection, as [`ByRevision::connected_again`] says.
    pub(crate) fn connected_again(&mut self) {
        self.held.connected_again(&mut self.taken);
    }

    /// What views read of them now.
    pub(in super::super) fn listed(&self) -> Arc<SessionsListed<R>> {
        let listed = self.shown.get_or_init(|| {
            let mut sessions: HashMap<&SessionId, Vec<(Arc<R>, Taken)>> = HashMap::new();
            for ((session, _), held) in &self.held.held {
                sessions.entry(session).or_default().push((held.copy.clone(), held.taken));
            }
            Arc::new(SessionsListed::of(sessions.into_iter()))
        });
        listed.clone()
    }

    /// Makes `change` to what's held, letting what views read go when it says it changed it.
    fn changed(&mut self, change: impl FnOnce(&mut ByRevision<R>, &mut Taken) -> bool) -> bool {
        let changed = change(&mut self.held, &mut self.taken);
        if changed {
            self.shown.take();
        }
        changed
    }
}
