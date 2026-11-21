use protocol::versions::Versions;
use protocol::worker::HUB_SPEAKS;
use serde::Serialize;

/// The oldest database a build migrates: the one that `release`, the oldest release it updates from, left once the
/// first `migrations` of the hub's migrations had run. The hub refuses a database from an older release and leaves it
/// as it is, so `release` can still migrate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Floor {
    pub release: &'static str,
    pub migrations: usize,
}

/// This build's floor, or `None` while it updates from any release. It moves only at a break in the worker protocol,
/// as "The database" in `docs/principles/state.md` says.
pub const FLOOR: Option<Floor> = None;

/// What a build says of itself in the update feed, beside its packages: its two floors. `roundhoused worker-versions`
/// prints it, as `{"oldest":3,"newest":4}` and, once the build has a floor, `"updatesFrom":"0.1.300"` too, and
/// `tools/package/publish.py` writes it on the release's full package as `WorkerVersions` and `UpdatesFrom`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Floors {
    /// Which versions of the worker protocol its hub speaks.
    #[serde(flatten)]
    pub workers: Versions,
    /// The oldest release it updates from, if it says one.
    #[serde(rename = "updatesFrom", skip_serializing_if = "Option::is_none")]
    pub release: Option<&'static str>,
}

/// This build's floors.
pub const FLOORS: Floors = Floors {
    workers: HUB_SPEAKS,
    release: match FLOOR {
        Some(floor) => Some(floor.release),
        None => None,
    },
};
