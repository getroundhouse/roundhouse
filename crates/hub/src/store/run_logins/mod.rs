//! The account whose login the hub gave each run of a session's agent it launched under one, which is how the hub
//! knows which account such a run is under, since a token doesn't say whose it is and the worker isn't told.

#[cfg(test)]
use super::{Store, lock};
#[cfg(test)]
use protocol::model::{AccountId, RunId, SessionId};

#[cfg(test)]
mod tests;

mod launch_rows;
#[cfg(test)]
pub(crate) use launch_rows::RunLogin;
