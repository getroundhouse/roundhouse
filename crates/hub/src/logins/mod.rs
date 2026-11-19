//! The logins the hub holds for agents' accounts. Each token is a file of its own, which `crate::login_files` keeps,
//! in the order it says, and everything else about the login is on its account's row in the database. The token stays
//! out of the database since the copies of it from before each migration would keep a token once it was disconnected.

#[cfg(test)]
use crate::login_files::LoginFile;
#[cfg(test)]
use crate::store::Store;
#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::client::accounts::AccountConnect;
#[cfg(test)]
use protocol::model::{AccountId, AgentKind, LoginDescription, LoginToken};
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
mod tests;

mod accounts;
mod automatic;
mod reserves;
mod targets;

pub(crate) use accounts::{AccountChange, Connect, Handed, Logins};
