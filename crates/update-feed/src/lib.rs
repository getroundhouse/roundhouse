//! The update feed: where it's published, its signature, and what a release says in it beyond what Velopack reads,
//! which `tools/package/publish.py` writes: which versions of the worker protocol its hub speaks. The app reads it to
//! take the newest update whose hub speaks every worker at work, so its update crosses one break at a time, as
//! `docs/principles/protocol.md` says, and `tools/update-steps` to hold back a change that would leave it none, as "A
//! break in the worker protocol" in `docs/standards/merging.md` says. It does no IO, so the app, a hub and the tools share it without the rest of any.

mod feed;
#[cfg(feature = "testing")]
pub mod fixture;
mod signature;
#[cfg(test)]
mod tests;

pub use feed::{BadChecksum, BadVersion, Feed, HubArchive, Release, is_release};
pub use signature::{HUB_CHANNEL, KEY, URL, Unsigned, UpdateFeedKey};
