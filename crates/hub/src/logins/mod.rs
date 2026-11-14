//! The logins the hub holds for agents' accounts. Each token is a file of its own, which `crate::login_files` keeps,
//! and everything else about the login is on its account's row in the database. The token stays out of the database
//! since the copies of it from before each migration would keep a token once it was disconnected.
//!
//! The writes are in an order chosen so that whatever stops the hub, or the machine, leaves either a file no row names
//! or a row naming a file that's gone. As the hub starts, [`Logins::tidy`] mends both. So a login the hub has said it
//! holds is never lost, and a token it was told to forget never comes back.

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

pub(crate) use accounts::{AccountChange, Connect, Handed, Logins};

mod defaults;
