//! What a view reads of the hub's observations. One the client doesn't hold reads as not known.

use std::collections::HashMap;

use protocol::model::{LineCounts, Observation, Observed, WorkspaceId};

use super::Snapshot;

/// Every observation held, as views read them.
#[derive(Debug, Default)]
pub(in super::super) struct ObservationsListed {
    workspace_lines: HashMap<WorkspaceId, Observed<LineCounts>>,
}

/// What a view reads of an observation the client doesn't hold.
const NOT_KNOWN: Observed<LineCounts> = Observed::NotKnown;

impl ObservationsListed {
    /// From every observation held.
    pub(in super::super) fn of<'a>(held: impl Iterator<Item = &'a Observation>) -> Self {
        let mut listed = ObservationsListed::default();
        for observation in held {
            match observation {
                Observation::WorkspaceLines { workspace_id, lines, .. } => {
                    listed.workspace_lines.insert(workspace_id.clone(), lines.clone());
                }
                // Never held, as `Revised` says.
                Observation::WorkspaceChanges { .. } | Observation::Unknown => {}
            }
        }
        listed
    }
}

impl Snapshot {
    /// How many lines the workspace's changes add and remove against its base, in all. Not known until the hub has
    /// sent it, and for good from a hub from before observations.
    pub fn workspace_lines(&self, workspace: &WorkspaceId) -> &Observed<LineCounts> {
        self.observations.workspace_lines.get(workspace).unwrap_or(&NOT_KNOWN)
    }
}
