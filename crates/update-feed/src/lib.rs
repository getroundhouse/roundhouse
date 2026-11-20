//! What a release says in the update feed beyond what Velopack reads, which `tools/package/publish.py` writes: which
//! versions of the worker protocol its hub speaks. The app reads it to take the newest update whose hub speaks every
//! worker at work, so its update crosses one break at a time, as `docs/principles/protocol.md` says, and
//! `tools/update-steps` to hold back a change that would leave it none, as "Updating the app" in
//! `docs/architecture.md` says. It does no IO, so the app and the tools share it without the rest of either.

mod feed;
#[cfg(test)]
mod tests;

pub use feed::{BadVersion, Feed, Release};
