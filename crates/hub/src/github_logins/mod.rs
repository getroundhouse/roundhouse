//! The logins the hub holds for GitHub accounts, and which account is the default. Each token is a file of its own, in
//! a directory apart from agents' logins, which `crate::login_files` keeps in the order it says, and everything else
//! about the login is on its account's row in the database, as `crate::logins` does for agents' accounts.

#[cfg(test)]
use crate::store::Store;
#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::client::github_accounts::GithubAccountConnect;
#[cfg(test)]
use protocol::model::{GithubAccountId, GithubToken, GithubUser, GithubUserId};
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
mod tests;

mod accounts;
pub(crate) use accounts::{Connect, GithubAccountChange, GithubLogins};
