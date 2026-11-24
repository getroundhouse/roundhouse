//! The GitHub accounts the hub holds a login for, as each account's row has them: which user it is on GitHub, by its
//! number, which file the token is in, and how and when the login was connected and ends. The token is never in the
//! database, as `crate::github_logins` says. What GitHub calls the user, and what its profile says, which the account's
//! commit identity is made from, the hub only observes, as `crate::hub::observations::GithubUsername` says.

#[cfg(test)]
use crate::login_files::LoginFile;
#[cfg(test)]
use protocol::model::{GithubAccountId, GithubLoginKind, GithubUser, GithubUserId};

#[cfg(test)]
mod tests;

mod installations;
mod rows;
pub(crate) use rows::{GithubConnected, GithubConnecting, GithubRemoved};
pub(super) use rows::{connect_login, github_accounts, signed_in_as};
