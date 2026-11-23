//! What the `video` crate's build script compiles of libvpx, for the target it builds for: which of the configs in
//! `third_party/libvpx`, which flags each C file takes, and how x86's assembly is assembled. `tools/libvpx-sync.sh`
//! writes a config for each target, so the build runs no configure or make. This is apart from the build script so its
//! tests run, which a build script's don't, as `docs/standards/rust.md` says.

#![warn(clippy::wildcard_enum_match_arm)]

mod config;
mod flags;
mod sources;

pub use config::{Config, UnsupportedTarget};
pub use flags::{Compiler, flags};
pub use sources::{Sources, sources};
