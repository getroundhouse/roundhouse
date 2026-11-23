//! Each kind of observation, and which one an observation is.

use serde::{Deserialize, Serialize};

use super::Observed;
use crate::model::{ChangeSummary, Level, LineCounts, Rev, WorkspaceId};

/// One observation, of any kind, with what it's of, its revision and what it says, as a list or the feed gives it. A
/// client keeps whichever copy has the newer revision, as it keeps a record, by [`Observation::key`]. A kind this build
/// doesn't know reads as `Unknown`, which a client doesn't keep.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Observation {
    /// How many lines a workspace's changes add and remove against its base, in all. Sent at the hub's level, since a
    /// sidebar draws it for every workspace.
    WorkspaceLines { workspace_id: WorkspaceId, lines: Observed<LineCounts>, rev: Rev },
    /// Every file a workspace changes against its base, with each file's lines and version, as its worker summed them
    /// up. Sent at the workspace's level, since only the workspace's own pane draws it. It's current while something
    /// keeps it up to date, and otherwise as of the last read, which the hub makes when the lines are read.
    WorkspaceChanges { workspace_id: WorkspaceId, changes: Observed<ChangeSummary>, rev: Rev },
    #[serde(other)]
    Unknown,
}

/// Which observation, of which kind, as [`RecordKey`](crate::model::RecordKey) says which record. A kind this build
/// doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationKey {
    WorkspaceLines {
        workspace_id: WorkspaceId,
    },
    WorkspaceChanges {
        workspace_id: WorkspaceId,
    },
    #[serde(other)]
    Unknown,
}

impl Observation {
    /// Which one it is. `Unknown` for a kind this build doesn't know.
    pub fn key(&self) -> ObservationKey {
        match self {
            Observation::WorkspaceLines { workspace_id, .. } => {
                ObservationKey::WorkspaceLines { workspace_id: workspace_id.clone() }
            }
            Observation::WorkspaceChanges { workspace_id, .. } => {
                ObservationKey::WorkspaceChanges { workspace_id: workspace_id.clone() }
            }
            Observation::Unknown => ObservationKey::Unknown,
        }
    }

    /// The revision of this copy, which says whether it replaces another, as [`Rev::supersedes`] does. None for a kind
    /// this build doesn't know.
    pub fn rev(&self) -> Rev {
        match self {
            Observation::WorkspaceLines { rev, .. } | Observation::WorkspaceChanges { rev, .. } => *rev,
            Observation::Unknown => Rev::default(),
        }
    }
}

impl ObservationKey {
    /// The level the hub sends it at, and so whose subscriptions it reaches. One at the hub's level is listed in
    /// [`Records`](crate::model::Records), which a client reads as it connects. One of another level, such as a
    /// workspace's or a session's, reaches a client only while it follows that, so the hub's list that lacks it doesn't
    /// say it's gone, and only that level's own list does. A kind sent at a workspace's level has a key that names the workspace, so this can say which
    /// workspace's. `None` for one of a kind this build doesn't know, which is of no level it can follow.
    pub fn level(&self) -> Option<Level> {
        match self {
            ObservationKey::WorkspaceLines { .. } => Some(Level::Hub),
            ObservationKey::WorkspaceChanges { workspace_id } => Some(Level::Workspace { id: workspace_id.clone() }),
            ObservationKey::Unknown => None,
        }
    }
}
