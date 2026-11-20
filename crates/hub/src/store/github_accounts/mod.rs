//! The GitHub accounts the hub holds a login for, as each account's row has them: who it is on GitHub, which file the
//! token is in, and how and when the login was connected and ends, and what GitHub's profile of its user said, which
//! its commit identity is made from. The token is never in the database, as `crate::github_logins` says.

#[cfg(test)]
use crate::login_files::LoginFile;
#[cfg(test)]
use protocol::model::{GithubAccountId, GithubLoginKind, GithubUser, GithubUserId};

#[cfg(test)]
mod tests;

mod installations;
mod profiles;
mod rows;
pub(crate) use rows::{GithubConnected, GithubConnecting, GithubRemoved};
pub(super) use rows::{connect_login, github_accounts, signed_in_as};
