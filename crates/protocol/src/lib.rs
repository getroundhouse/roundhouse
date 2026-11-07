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

/// Which build this is, for telling people. An install stamps the commit it came from with
/// `TRESTLE_BUILD`, and anything else says `-dev`.
pub const BUILD_VERSION: &str = match option_env!("TRESTLE_BUILD") {
    Some(version) => version,
    None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
};
