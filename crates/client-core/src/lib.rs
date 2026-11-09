//! What every client shares: the connection to a hub. No UI and no async runtime. Calls block, so
//! they belong on a background thread, and subscriptions deliver through a channel that both
//! threads and async tasks can read. This keeps GPUI's executor out of it and leaves a path open
//! for a native mobile client.
//!
//! [`records`] holds what a client has of a hub's records, and [`session_log`] what it has of a
//! session's history. [`RecordsFeed`] and [`OpenSession`] keep them current, each on a thread of its
//! own, so a view only reads them.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod records;
pub mod session_log;

mod client;

pub use client::{
    Client, ClientError, Daemon, Ended, FollowError, HubAdmin, OpenSession, RecordsFeed, Result, Subscription,
    SubscriptionItem, new_message_id, new_request_id, stop_hub,
};
