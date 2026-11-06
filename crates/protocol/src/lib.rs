//! Types shared by clients, hubs and workers. No IO and no async runtime, so everything can
//! depend on this crate, including a future mobile client.
//!
//! The wire format is newline-delimited JSON over any byte stream: a Unix socket between client
//! and hub, stdio between hub and worker. Both sides say hello first and compare
//! [`PROTOCOL_VERSION`]. Fields and enum variants can be added without a version bump, because
//! unknown fields are ignored and unknown variants decode as `Unknown`.

pub mod client;
pub mod event;
pub mod frame;
pub mod model;
pub mod paths;
pub mod tail;
pub mod worker;

pub const PROTOCOL_VERSION: u32 = 1;
