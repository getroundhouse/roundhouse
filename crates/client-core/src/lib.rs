//! What every client shares: the connection to a hub. No UI and no async runtime. Calls block, so
//! they belong on a background thread, and subscriptions deliver through a channel that both
//! threads and async tasks can read. This keeps GPUI's executor out of it and leaves a path open
//! for a native mobile client.
//!
//! [`session_log`] holds what a client has of a session's history.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod session_log;

mod client;

pub use client::{Client, ClientError, Result, Subscription, SubscriptionItem};
