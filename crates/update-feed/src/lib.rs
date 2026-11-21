//! The update feed: where it's published, its signature, and what a release says in it beyond what Velopack reads.
//! That's its two floors: which versions of the worker protocol its hub speaks, which `tools/package/publish.py`
//! writes, and the oldest release it updates from, if it says one. The app and a hub installed as a service read them
//! to take the newest release past both, so an update crosses one break in the worker protocol at a time, as
//! `docs/principles/protocol.md` says, and reaches no hub that can't migrate its database. `tools/update-steps` reads
//! them to hold back a change that would leave an app no release to step to, as "A break in the worker protocol" in
//! `docs/standards/merging.md` says. It also holds this build's floor, the oldest release it updates from, which the
//! hub's store reads to refuse a database older than that. It does no IO, so the app, a hub and the tools share it
//! without the rest of any.

mod feed;
#[cfg(any(test, feature = "testing"))]
pub mod fixture;
mod floor;
mod signature;
#[cfg(test)]
mod tests;

pub use feed::{BadChecksum, BadVersion, Feed, HubArchive, HubRelease, Release, Running, Stranded, is_release};
pub use floor::{FLOOR, FLOORS, Floor, Floors};
pub use signature::{HUB_CHANNEL, KEY, URL, Unsigned, UpdateFeedKey};
