//! The GitHub accounts the hub holds a login for, as each account's row has them: who it is on GitHub, which file the
//! token is in, and how and when the login was connected and ends. The token is never in the database, as
//! `crate::github_logins` says.

#[cfg(test)]
use crate::login_files::LoginFile;
#[cfg(test)]
use protocol::model::{GithubAccountId, GithubLoginKind, GithubUser, GithubUserId};

#[cfg(test)]
mod tests;

mod rows;
pub(super) use rows::github_accounts;
pub(crate) use rows::{GithubConnected, GithubConnecting};
