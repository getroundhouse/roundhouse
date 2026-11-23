use super::by_revision::ByRevision;
use super::of_session::BySession;
use super::snapshot::{Listed, ObservationsListed, Snapshot};
use crate::Agents;
use protocol::client::{HubNotice, ObservationNotice, RecordNotice};
use protocol::model::{Observation, Proposal, Record, RecordKey, Records, ReplyHeard, Rev, SessionId, Subagent};
use std::cell::OnceCell;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct HubRecords {
    pub(super) records: ByRevision<Record>,
    /// Those of every level the hub sent, kept by the same rule as records.
    pub(super) observations: ByRevision<Observation>,
    /// The newest list taken in. Everything up to it is already reflected.
    pub(super) listed: Rev,
    pub(super) subagents: BySession<Subagent>,
    pub(super) proposals: BySession<Proposal>,
    pub(super) replies_heard: BySession<ReplyHeard>,
    /// The newest copy taken in.
    pub(super) taken: Taken,
    /// What views read of the records, made when they next look after a change.
    pub(super) shown: OnceCell<Arc<Listed>>,
    /// And of the observations.
    pub(super) shown_observations: OnceCell<Arc<ObservationsListed>>,
    /// As the hub said on the newest connection, shared with every snapshot made since.
    pub(super) agents: Arc<Agents>,
}

/// A copy held, and when it was taken in.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Kept<T> {
    pub(super) copy: T,
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
    /// Takes in a list of every record, and of the observations sent at the hub's level. A record or an observation of
    /// that level missing from it is gone, unless the copy held is newer than the list. Returns whether anything
    /// changed.
    pub fn apply_list(&mut self, mut list: Records) -> bool {
        let at = list.rev;
        // Older than one already taken in, so everything in it is too.
        if at != Rev::default() && at < self.listed {
            return false;
        }
        let observations = std::mem::take(&mut list.observations);
        let records = self.records.apply_list(at, list.into_records(), &mut self.taken);
        let observed = self.observations.apply_list(at, observations, &mut self.taken);
        self.listed = self.listed.max(at);
        if records {
            self.shown.take();
        }
        if observed {
            self.shown_observations.take();
        }
        records || observed
    }

    /// Takes in what the feed said. Returns whether anything changed. `Resync` changes nothing here:
    /// it asks for a new list.
    pub fn apply_notice(&mut self, notice: HubNotice) -> bool {
        let notice = match RecordNotice::try_from(notice) {
            Ok(RecordNotice::Copy(record)) => return self.apply_record(record),
            Ok(RecordNotice::Removed { record, rev }) => return self.remove(&record, rev),
            Err(notice) => notice,
        };
        let observed = match ObservationNotice::try_from(notice) {
            Ok(ObservationNotice::Copy(observation)) => {
                self.observations.apply(observation, self.listed, &mut self.taken)
            }
            Ok(ObservationNotice::Removed { observation, rev }) => {
                self.observations.remove(&observation, rev, self.listed)
            }
            Err(_) => false,
        };
        if observed {
            self.shown_observations.take();
        }
        observed
    }

    /// Takes in one record, from a notice or a reply. Returns whether it changed anything.
    pub fn apply_record(&mut self, record: Record) -> bool {
        match record {
            // Kept apart, since only the subagents and proposals of the sessions a view shows are kept, each session's
            // listed on its own rather than with everything, so a list of everything says nothing of them.
            Record::Subagent(subagent) => self.subagents.apply(subagent),
            Record::Proposal(proposal) => self.proposals.apply(proposal),
            Record::ReplyHeard(heard) => self.replies_heard.apply(heard),
            listed @ (Record::Repo(_)
            | Record::Workspace(_)
            | Record::Session(_)
            | Record::Comment(_)
            | Record::HubSettings(_)
            | Record::HubUpdate(_)
            | Record::Account(_)
            | Record::NewWorkspaceDraft(_)
            | Record::GithubAccount(_)
            | Record::GithubSignIn(_)
            | Record::FoundGithubLogin(_)
            | Record::GithubApp(_)
            | Record::GithubReach(_)
            | Record::Terminal(_)
            | Record::ModelList(_)) => self.apply_listed(listed),
        }
    }

    /// Takes in one of the records a list of everything has.
    fn apply_listed(&mut self, record: Record) -> bool {
        let changed = self.records.apply(record, self.listed, &mut self.taken);
        if changed {
            self.shown.take();
        }
        changed
    }

    pub(super) fn remove(&mut self, key: &RecordKey, rev: Rev) -> bool {
        let changed = self.records.remove(key, rev, self.listed);
        if changed {
            self.shown.take();
        }
        changed
    }

    /// The records as they are now, for a view to read and keep. It's made once for each change, however many views
    /// ask for it.
    pub fn snapshot(&self) -> Snapshot {
        let records = self
            .shown
            .get_or_init(|| Arc::new(Listed::of(self.records.held.values().map(|held| (&held.copy, held.taken)))));
        let observations = self
            .shown_observations
            .get_or_init(|| Arc::new(ObservationsListed::of(self.observations.held.values().map(|held| &held.copy))));
        Snapshot::new(
            records.clone(),
            observations.clone(),
            self.subagents.listed(),
            self.proposals.listed(),
            self.replies_heard.listed(),
            self.agents.clone(),
        )
    }

    pub(crate) fn subagents_mut(&mut self) -> &mut BySession<Subagent> {
        &mut self.subagents
    }

    pub(crate) fn proposals_mut(&mut self) -> &mut BySession<Proposal> {
        &mut self.proposals
    }

    pub(crate) fn replies_heard_mut(&mut self) -> &mut BySession<ReplyHeard> {
        &mut self.replies_heard
    }

    /// Keeps the records of `session` that are listed by session from now on, of every kind. Returns whether any
    /// weren't kept already.
    pub(crate) fn want_session(&mut self, session: &SessionId) -> bool {
        let subagents = self.subagents.want(session.clone());
        let proposals = self.proposals.want(session.clone());
        self.replies_heard.want(session.clone()) || subagents || proposals
    }

    /// Lets go of the records of `session` that are listed by session, of every kind.
    pub(crate) fn forget_session(&mut self, session: &SessionId) {
        self.subagents.forget(session);
        self.proposals.forget(session);
        self.replies_heard.forget(session);
    }

    /// Lists every wanted session's records of every kind again, as [`BySession::relist`] says.
    pub(crate) fn relist_sessions(&mut self) {
        self.subagents.relist();
        self.proposals.relist();
        self.replies_heard.relist();
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
