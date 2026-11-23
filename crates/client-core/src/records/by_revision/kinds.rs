//! The kinds a client keeps by revision.

use protocol::model::{Level, Observation, ObservationKey, Record, RecordKey, Rev};

use super::Revised;

impl Revised for Record {
    type Key = RecordKey;

    fn key(&self) -> Option<RecordKey> {
        Some(Record::key(self))
    }

    fn rev(&self) -> Rev {
        Record::rev(self)
    }

    /// Every kind kept here is. Those listed by session are kept apart, as `HubRecords::apply_record` says.
    fn in_every_list(_: &RecordKey) -> bool {
        true
    }
}

impl Revised for Observation {
    type Key = ObservationKey;

    fn key(&self) -> Option<ObservationKey> {
        match self {
            Observation::WorkspaceLines { .. } | Observation::WorkspaceChanges { .. } => Some(Observation::key(self)),
            // Not drawn yet, so not kept.
            Observation::WorkspacePullRequest { .. }
            | Observation::WorkspacePullRequestDetail { .. }
            | Observation::WorkspaceRepositoryUnseen { .. }
            | Observation::Reference { .. }
            | Observation::Unknown => None,
        }
    }

    fn rev(&self) -> Rev {
        Observation::rev(self)
    }

    /// Only those sent at the hub's level are in the list of everything, as [`ObservationKey::level`] says. A
    /// workspace's are in its own list, as `HubRecords::apply_workspace_list` takes it.
    fn in_every_list(key: &ObservationKey) -> bool {
        key.level() == Some(Level::Hub)
    }
}
