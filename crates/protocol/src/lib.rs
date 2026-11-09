//! Types shared by clients, hubs and workers. No IO and no async runtime, so everything can
//! depend on this crate, including a future mobile client.
//!
//! The wire format is newline-delimited JSON over any byte stream: a Unix socket between client
//! and hub, stdio between hub and worker. Both sides say hello first and compare
//! [`PROTOCOL_VERSION`]. Fields, enum variants and requests can all be added without a version bump,
//! because unknown fields are ignored, unknown variants decode as `Unknown`, and a hub answers a
//! request it doesn't know with [`client::ErrorCode::Unsupported`]. A bump means a real break.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod client;
pub mod event;
pub mod frame;
pub mod model;
pub mod paths;
pub mod tail;
pub mod worker;

pub const PROTOCOL_VERSION: u32 = 3;

/// The build version an install stamped with `ROUNDHOUSE_BUILD`, if one did. It names a directory, so a build stamped
/// with one that can't doesn't compile.
pub const STAMPED_VERSION: Option<&str> = match option_env!("ROUNDHOUSE_BUILD") {
    Some(version) if paths::is_plain_name(version) => Some(version),
    Some(_) => panic!("ROUNDHOUSE_BUILD has to be one plain name, without a /, since it names a directory"),
    None => None,
};

/// Which build this is, for telling people. An install stamps the commit it came from, and anything else says `-dev`.
pub const BUILD_VERSION: &str = match STAMPED_VERSION {
    Some(version) => version,
    None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
};

/// Puts builds in order, which a commit hash can't, so the app can tell a hub is older than it. An install stamps
/// when its commit was made with `ROUNDHOUSE_BUILD_NUMBER`, and anything else has none, so it's never replaced.
pub const BUILD_NUMBER: Option<u64> = match option_env!("ROUNDHOUSE_BUILD_NUMBER") {
    Some(number) => match u64::from_str_radix(number, 10) {
        Ok(number) => Some(number),
        Err(_) => panic!("ROUNDHOUSE_BUILD_NUMBER has to be a number"),
    },
    None => None,
};

// A build with a number and no version would say `-dev` whatever its code, yet still be put in order and get a copy
// of the daemon kept for it by name.
const _: () = assert!(
    STAMPED_VERSION.is_some() == BUILD_NUMBER.is_some(),
    "ROUNDHOUSE_BUILD and ROUNDHOUSE_BUILD_NUMBER are set together or not at all"
);
