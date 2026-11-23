//! What every client shares: the connection to a hub. No UI and no async runtime. Calls block, so
//! they belong on a background thread, and subscriptions deliver through a channel that both
//! threads and async tasks can read. This keeps GPUI's executor out of it and leaves a path open
//! for a native mobile client.
//!
//! A [`link::Link`] connects to a hub, connects again whenever the connection goes, and keeps a [`RecordsFeed`] on
//! each connection, by one rule for every client, so an app only draws how it stands.
//!
//! [`records`] holds what a client has of a hub's records, and [`session_log`] what it has of a
//! session's history. [`RecordsFeed`] and [`OpenSession`] keep them current, each on a thread of its
//! own, so a view only reads them, and [`OpenTerminal`] keeps a copy of what a terminal shows the
//! same way, for a view to draw. [`RecordsFeed`] also takes in the record a call's reply carries, before the call
//! returns. [`Approval`] says what an approval asks, and how to answer it, however old its request, [`calls`] what each
//! tool call in a session's history is, [`refusals`] what the hub not doing what was asked means, and [`workspace_order`]
//! the orders workspaces can be listed in, each the same for every client.

#![warn(clippy::wildcard_enum_match_arm)]

pub mod calls;
pub mod conversation;
pub mod drawn;
pub mod file_text;
pub mod hints;
pub mod link;
pub mod marking;
pub mod moving;
pub mod prompt_cache;
pub mod records;
pub mod refusals;
pub mod session_log;
pub mod standing;
pub mod waits;
pub mod workspace_lines;
pub mod workspace_order;

mod agents;
mod approval;
mod attachments;
mod client;
mod drafts;
#[cfg(any(test, feature = "testing"))]
pub mod fake_hub;
mod new_chat;
mod sending;
#[cfg(any(test, feature = "testing"))]
pub mod stand_in_hub;

pub use agents::Agents;
pub use approval::{Answered, Approval, Given, ToolUseAnswer, ToolUseAsked, claims_of_the_asker};
pub use attachments::{FileToSend, NotAttached};
pub use drafts::{CameBack, DraftChange, DraftKey, Drafts, FilesFor, KeptDraft, NotReadBack, ReadBack};
pub use new_chat::new_chat_settings;
pub use sending::{NotTaken, Taking, Written};

pub use client::{
    Client, ClientError, Commands, Ended, FollowError, FollowedWorkspace, GithubFeed, GithubView, HubAdmin, IdleStop,
    Introduction, LiveItem, LiveLog, OpenFile, OpenSession, OpenTerminal, Opening, PairingWait, Pointer, RecordsFeed,
    Reply, Restart, Result, Search, Searched, SelectBy, ShownOfGithub, Stretch, Subscription, SubscriptionItem, View,
    hub_is_missing, new_attachment_id, new_device_id, new_github_sign_in_id, new_message_id, new_request_id,
    new_terminal_id, restart_hub, stop_hub, stop_hub_if_idle, stop_hub_if_still,
};

#[cfg(any(test, feature = "testing"))]
pub use client::pair_with_timeout;
pub use client::{RemoteHub, pair};
