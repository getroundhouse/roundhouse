//! Each subagent's record, for the sessions a view is showing. They aren't among every record, since each holds what
//! its subagent was asked and its report in full, so a session's are listed once a view asks for them, and the feed
//! keeps them current from then on.
//!
//! No IO. As with every record, a list and the feed's notices can arrive in any order, and each subagent keeps
//! whichever copy has the newer revision, as [`protocol::model::Rev::supersedes`] says. The hub never removes a
//! subagent's record, so one goes only when no view wants its session any more.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use protocol::model::{SessionId, Subagent, ToolUseId};

use super::snapshot::SubagentsListed;
use super::{Kept, Taken};

#[derive(Debug, Default)]
pub(crate) struct Subagents {
    /// By session, then by the call that started each. Each is shared with what views read.
    held: HashMap<SessionId, BTreeMap<ToolUseId, Kept<Arc<Subagent>>>>,
    /// The sessions a view asked for, whose subagents are kept.
    wanted: BTreeSet<SessionId>,
    /// Of those, the ones listed on this connection.
    listed: BTreeSet<SessionId>,
    /// The hub is from before subagents had records, so there's nothing to list.
    unkept: bool,
    /// The newest copy taken in.
    taken: Taken,
    /// What views read of them, made when they next look after a change.
    shown: OnceCell<Arc<SubagentsListed>>,
}

impl Subagents {
    /// Takes in one subagent's record, if its session is one a view asked for. Returns whether it changed anything.
    pub(crate) fn apply(&mut self, subagent: Subagent) -> bool {
        if !self.wanted.contains(&subagent.session_id) {
            return false;
        }
        let session = self.held.entry(subagent.session_id.clone()).or_default();
        // As for every record. A hub that keeps subagents' records keeps revisions, so a copy without one only comes
        // from a hub that doesn't, which has none to send.
        let held = session.get(&subagent.call).map(|held| &held.record);
        if held.is_some_and(|held| !subagent.rev.supersedes(held.rev) || **held == subagent) {
            return false;
        }
        let taken = self.taken.next();
        session.insert(subagent.call.clone(), Kept { record: Arc::new(subagent), taken });
        self.shown.take();
        true
    }

    /// Takes in a list of a session's subagents, if a view still wants them. One let go of while it was being listed,
    /// and wanted again since, is listed again. Returns whether it changed anything.
    pub(crate) fn apply_list(&mut self, session: &SessionId, list: Vec<Subagent>) -> bool {
        if !self.wanted.contains(session) {
            return false;
        }
        self.listed.insert(session.clone());
        let mut changed = false;
        for subagent in list.into_iter().filter(|subagent| subagent.session_id == *session) {
            changed |= self.apply(subagent);
        }
        changed
    }

    /// Keeps the subagents of `session` from now on. Returns whether they weren't kept already.
    pub(crate) fn want(&mut self, session: SessionId) -> bool {
        self.wanted.insert(session)
    }

    /// Lets go of the subagents of `session`, which no view shows any more.
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

    /// The sessions whose subagents are wanted and haven't been listed on this connection.
    pub(crate) fn to_list(&self) -> Vec<SessionId> {
        if self.unkept {
            return Vec::new();
        }
        self.wanted.difference(&self.listed).cloned().collect()
    }

    /// Lists every wanted session's subagents again, since notices may have gone missing, such as on a new
    /// connection. That may be to another hub, such as a newer one after a restart, so whether it keeps them is asked
    /// again too.
    pub(crate) fn relist(&mut self) {
        self.listed.clear();
        self.unkept = false;
    }

    /// The hub doesn't keep subagents' records.
    pub(crate) fn unkept(&mut self) {
        self.unkept = true;
    }

    /// Whether the hub keeps subagents' records. Until it's been asked, it's taken to.
    #[cfg(test)]
    fn kept(&self) -> bool {
        !self.unkept
    }

    /// What views read of them now.
    pub(super) fn listed(&self) -> Arc<SubagentsListed> {
        let listed = self.shown.get_or_init(|| {
            let sessions = self
                .held
                .iter()
                .map(|(session, its)| (session, its.values().map(|held| (held.record.clone(), held.taken)).collect()));
            Arc::new(SubagentsListed::of(sessions))
        });
        listed.clone()
    }
}

#[cfg(test)]
mod tests;
