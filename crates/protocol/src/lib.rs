//! Types shared by clients, hubs, workers and provider adapters. No IO and no async runtime, so everything can
//! depend on this crate, including a future mobile client.
//!
//! The wire format is newline-delimited JSON over any byte stream: a Unix socket between client
//! and hub, and stdio between hub and worker and between hub and provider adapter. Each protocol has a version of
//! its own, and both sides say hello first and use the newest version they both speak, as [`versions`] works out.
//! Fields, enum variants and requests can all be added without a version bump, because unknown fields are ignored,
//! unknown variants decode as `Unknown`, and a hub, a worker or a provider adapter answers a request it doesn't know,
//! or can't read, with [`client::ErrorCode::Unsupported`]. A bump means a real break.

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
pub mod short_text;
pub mod tail;
pub mod versions;
pub mod worker;

pub use build::{BUILD_NUMBER, BUILD_VERSION, STAMPED_BUILD, StampedBuild};
