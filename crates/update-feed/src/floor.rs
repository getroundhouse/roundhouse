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
