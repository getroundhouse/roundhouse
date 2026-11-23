//! The kinds a client keeps by revision.

use protocol::model::{Observation, ObservationKey, Record, RecordKey, Rev};

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
            Observation::WorkspaceLines { .. } => Some(Observation::key(self)),
            Observation::Unknown => None,
        }
    }

    fn rev(&self) -> Rev {
        Observation::rev(self)
    }

    /// Only those sent at the hub's level are in the list, as [`ObservationKey::at_hub_level`] says.
    fn in_every_list(key: &ObservationKey) -> bool {
        key.at_hub_level()
    }
}
