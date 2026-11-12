//! Types shared by clients, hubs, workers and provider adapters. No IO and no async runtime, so everything can
//! depend on this crate, including a future mobile client.
//!
//! The wire format is newline-delimited JSON over any byte stream: a Unix socket between client
//! and hub, and stdio between hub and worker and between hub and provider adapter. Both sides say hello first and
//! compare versions: [`PROTOCOL_VERSION`] for clients and workers, and [`provider::PROVIDER_PROTOCOL_VERSION`] for
//! adapters. Fields, enum variants and requests can all be added without a version bump,
//! because unknown fields are ignored, unknown variants decode as `Unknown`, and a hub answers a
//! request it doesn't know with [`client::ErrorCode::Unsupported`]. A bump means a real break.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod attachment;
mod build;
pub mod client;
pub mod event;
pub mod find;
pub mod frame;
pub mod model;
pub mod paths;
pub mod prompt;
pub mod provider;
pub mod tail;
pub mod worker;

pub const PROTOCOL_VERSION: u32 = 3;

pub use build::{BUILD_NUMBER, BUILD_VERSION, STAMPED_BUILD, StampedBuild};
