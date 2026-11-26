//! The chips one hub's views show: each view says which it shows, and client-core follows every chip at once, keeping
//! what the hub sends of them in the copy of the hub's records, beside what the hub's level sends of every workspace's
//! own pull request and what a workspace's own level sends of its pull request's checks, as `client::workspace_level`
//! does. A view reads all of it from a [`crate::Snapshot`].

mod held;
mod source;
#[cfg(test)]
mod tests;
mod views;

pub use views::{GithubFeed, GithubView};
