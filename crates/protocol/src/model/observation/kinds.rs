//! Each kind of observation, and which one an observation is.

use serde::{Deserialize, Serialize};

use super::Observed;
use crate::model::{LineCounts, Rev, WorkspaceId};

/// One observation, of any kind, with what it's of, its revision and what it says, as a list or the feed gives it. A
/// client keeps whichever copy has the newer revision, as it keeps a record, by [`Observation::key`]. A kind this build
/// doesn't know reads as `Unknown`, which a client doesn't keep.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Observation {
    /// How many lines a workspace's changes add and remove against its base, in all. Sent at the hub's level, since a
    /// sidebar draws it for every workspace.
    WorkspaceLines { workspace_id: WorkspaceId, lines: Observed<LineCounts>, rev: Rev },
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
            Observation::Unknown => ObservationKey::Unknown,
        }
    }

    /// The revision of this copy, which says whether it replaces another, as [`Rev::supersedes`] does. None for a kind
    /// this build doesn't know.
    pub fn rev(&self) -> Rev {
        match self {
            Observation::WorkspaceLines { rev, .. } => *rev,
            Observation::Unknown => Rev::default(),
        }
    }
}

impl ObservationKey {
    /// Whether the hub sends it at its own level, to every client that follows the hub, and so lists it in
    /// [`Records`](crate::model::Records), which a client reads as it connects. One of another level, such as a
    /// workspace's or a session's, reaches a client only while it follows that, so a list that lacks it doesn't say
    /// it's gone. One of a kind this build doesn't know is of no level it can follow.
    pub fn at_hub_level(&self) -> bool {
        match self {
            ObservationKey::WorkspaceLines { .. } => true,
            ObservationKey::Unknown => false,
        }
    }
}
