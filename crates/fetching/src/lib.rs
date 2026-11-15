//! Fetching files over HTTP(S) with `ureq`, on the caller's thread: a body into memory, refused past a limit, a file
//! that has to be just as long as it should, or a big file of known size and SHA-256 that goes on from where a stopped
//! fetch got to, into a folder one fetch at a time writes to. It knows nothing of what it fetches, and has no UI and no
//! async runtime, so any client on any device can fetch with it.

#![warn(clippy::wildcard_enum_match_arm)]

mod checked;
mod fetcher;
mod lock;
#[cfg(feature = "testing")]
pub mod serving;

pub use checked::{CheckError, Expected, Fetched};
pub use fetcher::{FetchError, Fetcher};
pub use lock::FolderLock;
