//! Which subscriptions each notice goes to, by the level it's sent at.

use super::HubNotice;
use crate::model::{Level, RecordKey};

impl HubNotice {
    /// The level whose subscriptions are sent it. The records of one session, which are its subagents, its proposals
    /// and how each reply read out ended, go to that session's level. Each observation goes to the level its kind is
    /// sent at, as [`ObservationKey::level`](crate::model::ObservationKey::level) says. Every other record goes to the
    /// hub's level. `None` for what's sent at no level: `Resync`, which no subscription is sent, since one that falls
    /// behind is sent where it stands again, `Unknown`, and the removal of a proposal, which names no session, and
    /// which the hub never sends.
    pub fn level(&self) -> Option<Level> {
        let session = |id: &crate::model::SessionId| Some(Level::Session { id: id.clone() });
        match self {
            HubNotice::Subagent(subagent) => session(&subagent.session_id),
            HubNotice::Proposal(proposal) => session(&proposal.session_id),
            HubNotice::ReplyHeard(heard) => session(&heard.session_id),
            HubNotice::Removed { record, .. } => match record {
                RecordKey::Subagent { session_id, .. } | RecordKey::ReplyHeard { session_id, .. } => {
                    session(session_id)
                }
                RecordKey::Proposal { .. } | RecordKey::Unknown => None,
                RecordKey::Repo { .. }
                | RecordKey::Workspace { .. }
                | RecordKey::Session { .. }
                | RecordKey::Comment { .. }
                | RecordKey::HubSettings
                | RecordKey::NewWorkspaceDraft
                | RecordKey::Account { .. }
                | RecordKey::GithubAccount { .. }
                | RecordKey::GithubSignIn { .. }
                | RecordKey::FoundGithubLogin { .. }
                | RecordKey::GithubApp { .. }
                | RecordKey::GithubReach { .. }
                | RecordKey::Terminal { .. }
                | RecordKey::ModelList(_)
                | RecordKey::HubUpdate => Some(Level::Hub),
            },
            HubNotice::Observation(observation) => observation.key().level(),
            HubNotice::ObservationRemoved { observation, .. } => observation.level(),
            HubNotice::Repo(_)
            | HubNotice::Workspace(_)
            | HubNotice::Session(_)
            | HubNotice::Comment(_)
            | HubNotice::HubSettings(_)
            | HubNotice::Account(_)
            | HubNotice::NewWorkspaceDraft(_)
            | HubNotice::GithubAccount(_)
            | HubNotice::GithubSignIn(_)
            | HubNotice::FoundGithubLogin(_)
            | HubNotice::GithubApp(_)
            | HubNotice::GithubReach(_)
            | HubNotice::Terminal(_)
            | HubNotice::ModelList(_)
            | HubNotice::HubUpdate(_) => Some(Level::Hub),
            HubNotice::Resync | HubNotice::Unknown => None,
        }
    }
}
