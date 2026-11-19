use super::super::Taken;
use super::super::of_session::OfSession;
use super::copies::{Copies, Group};
use protocol::model::SessionId;
use std::collections::HashMap;
use std::sync::Arc;

/// The records of one kind that the sessions views asked for have, as views read them.
#[derive(Debug)]
pub(in super::super) struct SessionsListed<R: OfSession> {
    /// Each session's, in the order views read them. Each is shared with what client-core holds, since one can hold a
    /// lot, such as all of a file a subagent's call wrote, and with what each frame draws.
    sessions: HashMap<SessionId, Group<Arc<R>>>,
    /// Where each is in its session's.
    at: HashMap<SessionId, HashMap<R::Id, usize>>,
}

impl<R: OfSession> Default for SessionsListed<R> {
    fn default() -> Self {
        SessionsListed { sessions: HashMap::new(), at: HashMap::new() }
    }
}

impl<R: OfSession> SessionsListed<R> {
    /// From each session's records held, with when each copy was taken in.
    pub(in super::super) fn of<'a>(held: impl Iterator<Item = (&'a SessionId, Vec<(Arc<R>, Taken)>)>) -> Self {
        let order = |a: &Arc<R>, b: &Arc<R>| R::order(a, b);
        let sessions: HashMap<SessionId, Group<Arc<R>>> =
            held.map(|(session, its)| (session.clone(), Group::of(its, order))).collect();
        let at = sessions.iter().map(|(session, its)| {
            let at = its.items.iter().enumerate().map(|(at, record)| (record.id(), at));
            (session.clone(), at.collect())
        });
        SessionsListed { at: at.collect(), sessions }
    }

    pub(super) fn of_session(&self, session: &SessionId) -> &[Arc<R>] {
        self.sessions.get(session).map_or(&[], |its| &its.items)
    }

    pub(super) fn find(&self, session: &SessionId, id: &R::Id) -> Option<&Arc<R>> {
        let at = self.at.get(session)?.get(id)?;
        self.sessions.get(session).map(|its| &its.items[*at])
    }

    pub(super) fn copies(&self, session: &SessionId) -> Option<Copies> {
        self.sessions.get(session).map(|its| its.copies)
    }
}
