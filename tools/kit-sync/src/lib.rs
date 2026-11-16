//! Keeps `third_party/gpui-kit/vendor`, the app's copy of Longbridge's gpui-kit, on the upstream release [`Pin::ours`]
//! pins, with our changes kept alongside as patches in `third_party/gpui-kit/patches`.
//!
//! The copy is what the app builds of upstream's repository at a release: gpui-kit and the crates under it, upstream's
//! workspace manifest, which they take their versions, dependencies and lints from, cut down to list only the packages
//! the copy keeps, rustfmt's settings, the license, and the few files elsewhere in the repository that the kept crates'
//! code reads, so their tests build in the copy. [`patch`] records each of our changes in its own patch file, [`sync`]
//! brings the copy and every patch onto a new release, [`drop_patch`] takes a patch out, and [`verify`] checks offline
//! that the copy matches the pinned release plus those patches. `third_party/README.md` says how the copy is changed
//! and how a release is taken.

#![warn(clippy::wildcard_enum_match_arm)]

mod cli;
mod copy;
mod cut;
mod digest;
mod error;
mod files;
mod git;
mod link;
mod members;
mod merge;
mod notice;
mod patch;
mod patches;
mod pin;
mod reads;
mod record;
mod scratch;
mod sync;
mod upstream;
mod verify;
mod versions;

pub use cli::run;
pub use copy::COPY;
pub use error::Error;
pub use files::CopyPath;
pub use git::Git;
pub use notice::NOTICE;
pub use patch::{Patched, drop_patch, patch};
pub use patches::{PATCHES, PatchName};
pub use pin::{Commit, Kept, Pin, Release, UPSTREAM};
pub use reads::Read;
pub use record::{RECORD, Record, Settling, Tree};
pub use sync::{Synced, sync};
pub use verify::verify;
pub use versions::Versions;
