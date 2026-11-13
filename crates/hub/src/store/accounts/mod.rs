//! The accounts agents run under, each a record: who it is, as the worker last read it, and what agents under it have
//! heard of its limits. A limit is the account's rather than a session's, so it's kept by account, whichever session's
//! agent heard it.

#[cfg(test)]
use super::{Store, lock};
#[cfg(test)]
use protocol::model::{AccountId, AgentAccount, LimitWindow, ModelLabel, UsageLimit};

#[cfg(test)]
mod tests;

mod account_rows;
pub use account_rows::Launched;
pub(super) use account_rows::{account_or_missing, accounts, name_account, used_by_session};
