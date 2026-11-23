//! What one hub says of GitHub, of every workspace's own pull request and of what this client's views show: each view
//! says what it shows, client-core follows all of it at once, and keeps what the hub sends, so a view only reads it,
//! and hears when what it shows changes.

mod held;
mod source;
#[cfg(test)]
mod tests;
mod views;

pub use views::{GithubFeed, GithubView};
