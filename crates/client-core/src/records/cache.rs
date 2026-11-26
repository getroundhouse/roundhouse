use super::by_revision::ByRevision;
use super::of_session::BySession;
use super::snapshot::{Listed, ObservationsListed, Snapshot};
use crate::Agents;
use protocol::client::listed::{ReferenceObservations, SessionRecords, WorkspaceObservations};
use protocol::client::{HubNotice, ObservationNotice, RecordNotice};
use protocol::model::{Level, Observation, Proposal, Record, RecordKey, Records, ReplyHeard, Rev, SessionId, Subagent};
use std::cell::OnceCell;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct HubRecords {
    pub(super) records: ByRevision<Record>,
    /// Those of every level the hub sent, kept by the same rule as records. Which workspaces' own levels have been
    /// listed since they were last let go of says that what's held of them is what the hub keeps, rather than nothing
    /// the hub sends.
    pub(super) observations: ByRevision<Observation>,
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
/// views which copies they've already seen, and what came on the connection now from what an earlier one left, which a
/// record's revision can't, since a hub whose revisions started again gives a newer copy an older one.
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
        let observations = std::mem::take(&mut list.observations);
        let records = self.records.apply_list(&Level::Hub, at, list.into_records(), &mut self.taken);
        let observed = self.observations.apply_list(&Level::Hub, at, observations, &mut self.taken);
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
            Ok(ObservationNotice::Copy(observation)) => self.observations.apply(observation, &mut self.taken),
            Ok(ObservationNotice::Removed { observation, rev }) => {
                self.observations.remove(&observation, rev, &mut self.taken)
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
            | Record::GithubApp(_)
            | Record::ToolServer(_)
            | Record::Terminal(_)
            | Record::ModelList(_)) => self.apply_listed(listed),
        }
    }

    /// Takes in one of the records a list of everything has.
    fn apply_listed(&mut self, record: Record) -> bool {
        let changed = self.records.apply(record, &mut self.taken);
        if changed {
            self.shown.take();
        }
        changed
    }

    /// Takes in that the record `key` is gone as of `rev`. Returns whether it dropped a copy.
    pub(super) fn remove(&mut self, key: &RecordKey, rev: Rev) -> bool {
        match key {
            RecordKey::Subagent { session_id, subagent } => self.subagents.remove(session_id, subagent.clone(), rev),
            RecordKey::ReplyHeard { session_id, message } => {
                self.replies_heard.remove(session_id, message.clone(), rev)
            }
            // It names no session, so no level's subscription is sent it, as `HubNotice::level` says.
            RecordKey::Proposal { .. } => false,
            listed @ (RecordKey::Repo { .. }
            | RecordKey::Workspace { .. }
            | RecordKey::Session { .. }
            | RecordKey::Comment { .. }
            | RecordKey::HubSettings
            | RecordKey::NewWorkspaceDraft
            | RecordKey::Account { .. }
            | RecordKey::GithubAccount { .. }
            | RecordKey::GithubSignIn { .. }
            | RecordKey::GithubApp { .. }
            | RecordKey::ToolServer { .. }
            | RecordKey::Terminal { .. }
            | RecordKey::ModelList(_)
            | RecordKey::HubUpdate
            | RecordKey::Unknown) => {
                let changed = self.records.remove(listed, rev, &mut self.taken);
                if changed {
                    self.shown.take();
                }
                changed
            }
        }
    }

    /// The records as they are now, for a view to read and keep. It's made once for each change, however many views
    /// ask for it.
    pub fn snapshot(&self) -> Snapshot {
        let records = self
            .shown
            .get_or_init(|| Arc::new(Listed::of(self.records.held.values().map(|held| (&*held.copy, held.taken)))));
        let observations = self.shown_observations.get_or_init(|| {
            let held = self.observations.held.values().map(|held| (held.copy.clone(), held.taken));
            let workspaces_listed = self.observations.levels_listed().filter_map(|level| match level {
                Level::Workspace { id } => Some(id.clone()),
                Level::Hub
                | Level::Session { .. }
                | Level::Terminal { .. }
                | Level::Reference { .. }
                | Level::Unknown => None,
            });
            Arc::new(ObservationsListed::of(held, workspaces_listed.collect()))
        });
        Snapshot::new(
            records.clone(),
            observations.clone(),
            self.subagents.listed(),
            self.proposals.listed(),
            self.replies_heard.listed(),
            self.agents.clone(),
        )
    }

    /// Keeps the records of `session` that are listed by session, of every kind, while it's followed, once more than
    /// before, as a session's subscription does.
    pub(crate) fn want_session(&mut self, session: &SessionId) {
        self.subagents.want(session.clone());
        self.proposals.want(session.clone());
        self.replies_heard.want(session.clone());
    }

    /// Follows `session` once less, letting go of its records of every kind, and its observations, once nothing follows
    /// it, since they reach a client only while it follows the session. Returns whether it let go of any.
    pub(crate) fn forget_session(&mut self, session: &SessionId) -> bool {
        let subagents = self.subagents.forget(session);
        let proposals = self.proposals.forget(session);
        let heard = self.replies_heard.forget(session);
        let observed = !self.subagents.follows(session) && self.let_go_of(&Level::Session { id: session.clone() });
        heard || subagents || proposals || observed
    }

    /// Takes in where a session's own records and observations stand, as its subscription lists them, while it's
    /// followed. Returns whether anything changed.
    pub(crate) fn apply_session_list(&mut self, listed: SessionRecords) -> bool {
        let SessionRecords { session_id, subagents, proposals, replies_heard, observations, rev } = listed;
        let observed = self.subagents.follows(&session_id)
            && self.observations.apply_list(
                &Level::Session { id: session_id.clone() },
                rev,
                observations,
                &mut self.taken,
            );
        if observed {
            self.shown_observations.take();
        }
        let subagents = self.subagents.apply_list(&session_id, subagents, rev);
        let proposals = self.proposals.apply_list(&session_id, proposals, rev);
        self.replies_heard.apply_list(&session_id, replies_heard, rev) || subagents || proposals || observed
    }

    /// Lets go of the observations held only through `level`, which a client stops following, as of a workspace
    /// whose pane has closed, since nothing tells it of them while it doesn't follow it. Returns whether anything
    /// changed.
    pub(crate) fn let_go_of(&mut self, level: &Level) -> bool {
        let changed = self.observations.let_go(level);
        if changed {
            self.shown_observations.take();
        }
        changed
    }

    /// Takes in where a workspace's own level stands, as its subscription lists it first, and again after falling
    /// behind, as [`ByRevision::apply_list`] says. Returns whether anything changed, which the first list of a workspace
    /// always does, since views then read that the hub sends its level.
    pub(crate) fn apply_workspace_list(&mut self, listed: WorkspaceObservations) -> bool {
        let WorkspaceObservations { workspace_id, observations, rev } = listed;
        let level = Level::Workspace { id: workspace_id };
        let changed = self.observations.apply_list(&level, rev, observations, &mut self.taken);
        if changed {
            self.shown_observations.take();
        }
        changed
    }

    /// Takes in where an issue's or a pull request's level stands, as its subscription lists it, as
    /// [`HubRecords::apply_workspace_list`] takes a workspace's. Returns whether anything changed.
    pub(crate) fn apply_reference_list(&mut self, listed: ReferenceObservations) -> bool {
        let ReferenceObservations { reference, observations, rev } = listed;
        let level = Level::Reference { reference };
        let changed = self.observations.apply_list(&level, rev, observations, &mut self.taken);
        if changed {
            self.shown_observations.take();
        }
        changed
    }

    /// Starts a new connection, maybe to another hub, after which the first list of each level replaces what's held of
    /// it, whatever the revisions, as [`ByRevision::apply_list`] says, so a hub whose revisions started again, as after
    /// its database is replaced from a copy, is shown as soon as it lists them. What's taken in on one connection is
    /// kept by revision.
    pub(crate) fn connected_again(&mut self) {
        self.records.connected_again(&mut self.taken);
        self.observations.connected_again(&mut self.taken);
        self.subagents.connected_again();
        self.proposals.connected_again();
        self.replies_heard.connected_again();
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
