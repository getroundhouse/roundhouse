//! Keeps `third_party/gpui-kit`, the app's copy of Longbridge's gpui-kit, on the upstream release [`Pin::ours`] pins,
//! with our changes to it on top.
//!
//! The copy is what the app builds of upstream's repository at a release: gpui-kit and the crates under it, upstream's
//! workspace manifest, which they take their versions, dependencies and lints from, cut down to list only the packages
//! the copy keeps, rustfmt's settings, the license, and the few files elsewhere in the repository that the kept crates'
//! code reads, so their tests build in the copy. [`sync`] brings the copy to a release, merging what upstream changed
//! with what we changed, as git merges two branches, and [`diff`] prints our changes, to offer them upstream.
//! `third_party/README.md` says how the copy is changed, and how a release is taken.

#![warn(clippy::wildcard_enum_match_arm)]

mod copy;
mod cut;
mod diff;
mod error;
mod files;
mod git;
mod link;
mod members;
mod merge;
mod notice;
mod pin;
mod reads;
mod record;
mod scratch;
mod sync;
mod upstream;
mod versions;

pub use copy::COPY;
pub use diff::diff;
pub use error::Error;
pub use files::CopyPath;
pub use git::Git;
pub use notice::NOTICE;
pub use pin::{Commit, Kept, Pin, Release, UPSTREAM};
pub use reads::Read;
pub use record::{RECORD, Record};
pub use sync::{Synced, sync};
pub use versions::Versions;
