//! Fetching files over HTTP(S) with `ureq`, on the caller's thread: a body into memory, refused past a limit, or a file
//! that has to be just as long as it should. It knows nothing of what it fetches, and has no UI and no async runtime,
//! so any client on any device can fetch with it.

#![warn(clippy::wildcard_enum_match_arm)]

mod fetcher;
#[cfg(feature = "testing")]
pub mod serving;

pub use fetcher::{FetchError, Fetcher};
