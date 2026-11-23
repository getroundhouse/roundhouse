//! What a view reads of the hub's observations. One the client doesn't hold reads as not known.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use protocol::model::{ChangeSummary, LineCounts, Observation, ObservationKey, Observed, WorkspaceId};

use super::super::Taken;
use super::{Changes, Snapshot};

/// Every observation held, as views read them, each shared with what client-core holds, since one can hold a lot, such
/// as a workspace's changed files, and every other is read again whenever one changes.
#[derive(Debug, Default)]
pub(in super::super) struct ObservationsListed {
    workspace_lines: HashMap<WorkspaceId, Arc<Observation>>,
    /// Each workspace's changed files, with which copy they are, so a view can tell they changed without comparing
    /// them.
    workspace_changes: HashMap<WorkspaceId, (Arc<Observation>, Taken)>,
    /// The workspaces whose own level the hub has listed since they were followed.
    workspaces_listed: HashSet<WorkspaceId>,
    /// What GitHub says, of every level, which what the client holds of GitHub reads, as `client::github` says.
    github: HashMap<ObservationKey, Arc<Observation>>,
}

/// What a view reads of an observation the client doesn't hold.
const NOT_KNOWN: Observed<LineCounts> = Observed::NotKnown;

/// The same, of a workspace's changed files, from a hub that sends its level.
const CHANGES_NOT_KNOWN: Observed<ChangeSummary> = Observed::NotKnown;

impl ObservationsListed {
    /// From every observation held, with when each was taken in, and the workspaces whose own level has been listed.
    pub(in super::super) fn of(
        held: impl Iterator<Item = (Arc<Observation>, Taken)>,
        workspaces_listed: HashSet<WorkspaceId>,
    ) -> Self {
        let mut listed = ObservationsListed { workspaces_listed, ..Self::default() };
        for (observation, taken) in held {
            match &*observation {
                Observation::WorkspaceLines { workspace_id, .. } => {
                    listed.workspace_lines.insert(workspace_id.clone(), observation);
                }
                Observation::WorkspaceChanges { workspace_id, .. } => {
                    listed.workspace_changes.insert(workspace_id.clone(), (observation, taken));
                }
                Observation::WorkspacePullRequest { .. }
                | Observation::WorkspacePullRequestDetail { .. }
                | Observation::WorkspaceRepositoryUnseen { .. }
                | Observation::Reference { .. } => {
                    listed.github.insert(observation.key(), observation);
                }
                // Never held, as `Revised` says.
                Observation::Unknown => {}
            }
        }
        listed
    }
}

impl Snapshot {
    /// How many lines the workspace's changes add and remove against its base, in all. Not known until the hub has
    /// sent it, and for good from a hub from before observations.
    pub fn workspace_lines(&self, workspace: &WorkspaceId) -> &Observed<LineCounts> {
        let held = self.observations.workspace_lines.get(workspace).map(Arc::as_ref);
        let Some(Observation::WorkspaceLines { lines, .. }) = held else { return &NOT_KNOWN };
        lines
    }

    /// Every file the workspace changes against its base, as the hub's summary of them says. `None` where the hub
    /// hasn't listed the workspace's own level since it was followed, which is from a hub from before workspaces had a
    /// level, or for a workspace nothing on this connection follows. Not known where the hub has read none yet.
    pub fn workspace_changes(&self, workspace: &WorkspaceId) -> Option<&Observed<ChangeSummary>> {
        let listed = &self.observations;
        if !listed.workspaces_listed.contains(workspace) {
            return None;
        }
        let held = listed.workspace_changes.get(workspace).map(|(changes, _)| changes.as_ref());
        let Some(Observation::WorkspaceChanges { changes, .. }) = held else { return Some(&CHANGES_NOT_KNOWN) };
        Some(changes)
    }
}

impl Snapshot {
    /// The observation of GitHub `key` names, if the hub has sent it, for what the client holds of GitHub to read.
    pub(crate) fn github_observation(&self, key: &ObservationKey) -> Option<&Observation> {
        self.observations.github.get(key).map(Arc::as_ref)
    }
}

impl Changes<'_> {
    /// Whether the workspace's changed files changed, came or went, or whether the hub sends them did.
    pub fn workspace_changes(&self, workspace: &WorkspaceId) -> bool {
        let copy = |snapshot: &Snapshot| {
            let listed = &snapshot.observations;
            let taken = listed.workspace_changes.get(workspace).map(|&(_, taken)| taken);
            (listed.workspaces_listed.contains(workspace), taken)
        };
        copy(self.now) != copy(self.then)
    }
}
