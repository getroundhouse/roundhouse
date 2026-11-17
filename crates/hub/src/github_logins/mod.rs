//! The logins the hub holds for GitHub accounts, the keys of apps of your own, which account is the default, what a
//! sign-in with GitHub keeps once it's approved, what GitHub's profile of each account's user says, and so what every
//! worker gives its processes for github.com, its commit identity too. Each login and key is a file of its own, in a
//! directory apart from agents' logins, which `crate::login_files` keeps in the order it says, and everything else
//! about the login is on its account's row in the database, and about the key on its app's, as `crate::logins` does
//! for agents' accounts.

#[cfg(test)]
use crate::github::TokenOwner;
#[cfg(test)]
use crate::store::Store;
#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::client::github_accounts::GithubAccountConnect;
#[cfg(test)]
use protocol::model::{GithubAccountId, GithubSignInStatus, GithubToken, GithubUser, GithubUserId};
#[cfg(test)]
pub(crate) use renewals::RENEWED_WITH_LEFT;
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
pub(crate) mod tests;

mod accounts;
pub(crate) use accounts::{Connect, GithubAccountChange, GithubLogins};
mod apps;
pub(crate) use apps::{AppChange, AppRemoved};
mod credential;
mod installations;
#[cfg(test)]
pub(crate) use installations::MINTED_WITH_LEFT;
pub(crate) use installations::{Looked, MintDue};
mod profiles;
#[cfg(test)]
pub(crate) use profiles::PROFILE_READ_EVERY;
pub(crate) use profiles::ProfileDue;
mod renewals;
pub(crate) use renewals::{Checked, Due};
mod secret;
pub(crate) use secret::{GithubLogin, InstallationToken, SignedIn};
mod sign_ins;
pub(crate) use sign_ins::{Approved, SignInChange, SignInEnd};
