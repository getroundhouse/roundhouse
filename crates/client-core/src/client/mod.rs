//! The connection to a hub.

mod account;
mod admin;
mod agent_list;
mod api;
mod commands;
mod feed;
mod files;
mod follow;
mod github;
mod github_account;
mod github_app;
mod introduction;
mod lines;
mod live_log;
mod open_file;
mod open_session;
mod open_terminal;
mod options;
mod outline;
mod places;
mod prompts;
mod queueing;
mod sent_call;
mod stream;
mod terminals;
mod typed;

pub use admin::{HubAdmin, IdleStop, Restart, restart_hub, stop_hub, stop_hub_if_idle, stop_hub_if_still};
pub use api::{new_attachment_id, new_device_id, new_message_id, new_request_id, new_terminal_id};
pub use commands::{Commands, Reply};
pub use feed::RecordsFeed;
pub use follow::FollowError;
pub use github::{GithubFeed, GithubView};
pub use github_account::new_github_sign_in_id;
pub use introduction::Introduction;
pub(crate) use lines::Place;
pub use live_log::{LiveItem, LiveLog};
pub use open_file::{OpenFile, Opening};
pub use open_session::{OpenSession, Search, Searched, Stretch, View};
pub use open_terminal::{OpenTerminal, Pointer, SelectBy};
pub(crate) use queueing::Shown;

#[cfg(test)]
mod tests;

mod connection;
mod frames;
#[cfg(test)]
use connection::SUBSCRIPTION_CAPACITY;
pub use connection::hub_is_missing;
use connection::subscription_channel;
pub use connection::{Client, ClientError, Ended, Result, Subscription, SubscriptionItem};

mod transport;

mod devices;
pub use devices::PairingWait;

mod remote;

#[cfg(any(test, feature = "testing"))]
pub use remote::pair_with_timeout;
pub use remote::{RemoteHub, pair};
