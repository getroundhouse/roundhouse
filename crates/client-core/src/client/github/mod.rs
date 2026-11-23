//! What one hub says of GitHub, of every workspace's own pull request and of the chips this client's views show: each
//! view says what it shows, client-core follows every chip at once, and keeps what the hub sends, so a view only reads
//! it, and hears when what it shows changes. What GitHub says of a workspace that only its own level sends comes while
//! the workspace is followed there, as `client::workspace_level` does.

mod held;
mod source;
#[cfg(test)]
mod tests;
mod views;

pub(in crate::client) use held::{github_keys, notice_key, own_keys};
pub use views::{GithubFeed, GithubView, ShownOfGithub};
