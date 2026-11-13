//! The logins the hub holds for agents' accounts, as each account's row has them: whose ID the account has, which file
//! the token is in, and when the login was connected and ends. The token is never in the database, as `crate::logins`
//! says.

#[cfg(test)]
use super::lock;
#[cfg(test)]
use crate::login_files::LoginFile;
#[cfg(test)]
use protocol::model::{AccountId, AgentKind};

#[cfg(test)]
mod tests;

mod login_rows;
#[cfg(test)]
pub(crate) use login_rows::LoginRow;
pub(crate) use login_rows::{Connecting, Held, IdFrom};
pub(super) use login_rows::{LOGIN_COLUMNS, login_at};
