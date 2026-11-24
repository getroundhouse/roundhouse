//! The kinds a client keeps by revision.

use protocol::model::{Level, Observation, ObservationKey, Record, RecordKey, Rev, SessionId};

use super::Revised;
use crate::records::of_session::OfSession;

impl Revised for Record {
    type Key = RecordKey;

    fn key(&self) -> Option<RecordKey> {
        Some(Record::key(self))
    }

    fn rev(&self) -> Rev {
        Record::rev(self)
    }

    /// Every kind kept here is listed at the hub's level. Those listed by session are kept apart, as
    /// `HubRecords::apply_record` says.
    fn level(_: &RecordKey) -> Option<Level> {
        Some(Level::Hub)
    }
}

impl Revised for Observation {
    type Key = ObservationKey;

    /// Every kind this build knows is kept, of every level.
    fn key(&self) -> Option<ObservationKey> {
        Some(Observation::key(self)).filter(|key| *key != ObservationKey::Unknown)
    }

    fn rev(&self) -> Rev {
        Observation::rev(self)
    }

    /// The level it's sent at, as [`ObservationKey::level`] says: those of the hub's level are in the list of
    /// everything, a workspace's in its own list, and an issue's or a pull request's in its.
    fn level(key: &ObservationKey) -> Option<Level> {
        key.level()
    }
}

/// A session's own records, kept for each session followed, by the session and which of its they are.
impl<R: OfSession> Revised for R {
    type Key = (SessionId, R::Id);

    fn key(&self) -> Option<Self::Key> {
        Some((self.session().clone(), self.id()))
    }

    fn rev(&self) -> Rev {
        OfSession::rev(self)
    }

    /// Its session's, whose subscription lists them.
    fn level((session, _): &Self::Key) -> Option<Level> {
        Some(Level::Session { id: session.clone() })
    }
}
