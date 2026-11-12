//! Types shared by clients, hubs, workers and provider adapters. No IO and no async runtime, so everything can
//! depend on this crate, including a future mobile client.
//!
//! The wire format is newline-delimited JSON over any byte stream: a Unix socket between client
//! and hub, and stdio between hub and worker and between hub and provider adapter. Each protocol has a version of
//! its own, [`client::CLIENT_PROTOCOL_VERSION`], [`worker::WORKER_PROTOCOL_VERSION`] and
//! [`provider::PROVIDER_PROTOCOL_VERSION`], and both sides say hello first and use the newest version they both
//! speak, which [`newest_shared`] works out. Fields, enum variants and requests can all be added without a version bump,
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

/// The newest version of a protocol that `ours` and a peer's hello both name, if any. A hello names the newest
/// version its side speaks, and the oldest too unless it's from before hellos named a range, in which case it speaks
/// only the one.
pub fn newest_shared(ours: std::ops::RangeInclusive<u32>, newest: u32, oldest: Option<u32>) -> Option<u32> {
    let shared = (*ours.end()).min(newest);
    (shared >= (*ours.start()).max(oldest.unwrap_or(newest))).then_some(shared)
}

/// Which versions a hello names, in words, for a refusal to say.
pub fn versions_named(newest: u32, oldest: Option<u32>) -> String {
    match oldest {
        Some(oldest) if oldest < newest => format!("protocols {oldest} to {newest}"),
        _ => format!("protocol {newest}"),
    }
}


pub use build::{BUILD_NUMBER, BUILD_VERSION, STAMPED_BUILD, StampedBuild};

#[cfg(test)]
mod tests {
    use super::newest_shared;

    #[test]
    fn the_newest_version_both_sides_speak_is_chosen() {
        assert_eq!(newest_shared(3..=3, 3, None), Some(3), "a peer from before ranges");
        assert_eq!(newest_shared(3..=3, 4, Some(2)), Some(3), "a newer peer that still speaks this one");
        assert_eq!(newest_shared(2..=4, 3, Some(1)), Some(3), "an older peer");
        assert_eq!(newest_shared(3..=3, 4, None), None);
        assert_eq!(newest_shared(3..=3, 5, Some(4)), None, "a newer peer that no longer speaks this one");
        assert_eq!(newest_shared(4..=5, 3, Some(2)), None, "one older than this side speaks");
    }
}
