//! The connection to a hub.

mod account;
mod admin;
mod agent_list;
mod api;
mod commands;
mod feed;
mod files;
mod follow;
mod lines;
mod open_file;
mod open_session;
mod options;
mod outline;
mod places;
mod prompts;
mod queueing;
mod start;
mod stream;
mod typed;

pub use admin::{HubAdmin, IdleStop, Restart, restart_hub, stop_hub, stop_hub_if_idle, stop_hub_if_still};
pub use api::{new_attachment_id, new_message_id, new_request_id};
pub use commands::{Commands, Reply};
pub use feed::RecordsFeed;
pub use follow::FollowError;
pub(crate) use lines::Place;
pub use open_file::{OpenFile, Opening};
pub use open_session::{OpenSession, Search, Searched, Stretch, View};
#[cfg(test)]
use protocol::client::{
    CLIENT_PROTOCOL_VERSION, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HelloReply, HubFrame, HubNotice,
    ReplyOutcome, subscriptions,
};
#[cfg(test)]
use protocol::frame;
pub(crate) use queueing::Shown;
pub use start::Daemon;
#[cfg(test)]
use std::io::{BufRead, BufReader, Write};
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
mod tests;

mod connection;
#[cfg(test)]
use connection::SUBSCRIPTION_CAPACITY;
pub use connection::{Client, ClientError, Ended, Result, Subscription, SubscriptionItem};
use connection::{hub_is_missing, subscription_channel};
