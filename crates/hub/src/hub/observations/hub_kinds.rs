//! The kinds of what the hub observes of no record but itself: the release the update feed offers it.

use protocol::model::{Observation, ObservationKey, Observed, ReleaseOffer, Rev};

use super::keepers::Unkept;
use super::kept::Table;
use super::kinds::{Form, Kind};
use super::tables::Tables;

/// The release the update feed offers a hub installed as a service, as its last read of the feed found, sent at the
/// hub's level: current while the hub's reads of the feed work, and as of the last that did otherwise. Kept in memory
/// alone, since the hub reads the feed again as it starts.
pub(crate) struct ReleaseOffered;

impl Kind for ReleaseOffered {
    type Of = ();
    type Value = ReleaseOffer;
    const FORM: Form = Form { table: "release_offered", version: 1 };
    const IN_MEMORY_ALONE: bool = true;
    const UNKEPT: Unkept = Unkept::AsOfItsLastRead;

    fn observation((): &(), offered: Observed<ReleaseOffer>, rev: Rev) -> Option<Observation> {
        Some(Observation::ReleaseOffered { offered, rev })
    }

    fn key((): &()) -> Option<ObservationKey> {
        Some(ObservationKey::ReleaseOffered)
    }

    fn table(tables: &mut Tables) -> &mut Table<Self> {
        &mut tables.release_offered
    }
}
