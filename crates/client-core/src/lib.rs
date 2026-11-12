//! What every client shares: the connection to a hub. No UI and no async runtime. Calls block, so
//! they belong on a background thread, and subscriptions deliver through a channel that both
//! threads and async tasks can read. This keeps GPUI's executor out of it and leaves a path open
//! for a native mobile client.
//!
//! [`records`] holds what a client has of a hub's records, and [`session_log`] what it has of a
//! session's history. [`RecordsFeed`] and [`OpenSession`] keep them current, each on a thread of its
//! own, so a view only reads them.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod file_text;
pub mod records;
pub mod session_log;

mod attachments;
mod client;
mod drafts;
mod sending;
#[cfg(test)]
mod stand_in_hub;

pub use attachments::{FileToSend, NotAttached};
pub use drafts::Drafts;
pub use sending::{NotTaken, Taking, Written};

pub use client::{
    Client, ClientError, Commands, Daemon, Ended, FollowError, HubAdmin, IdleStop, OpenFile, OpenSession, Opening,
    RecordsFeed, Reply, Result, Search, Searched, Stretch, Subscription, SubscriptionItem, View, new_attachment_id,
    new_message_id, new_request_id, stop_hub, stop_hub_if_idle, stop_hub_if_still,
};
