//! The tool servers the hub has of its own: adding one and removing it, with what it knows the person by,
//! a key or a sign-in, in a file of its own, in a directory apart from every login's, which `crate::login_files` keeps
//! in the order it says. Everything else about a server is on its row in the database. The secret stays out of it since
//! the copies of it from before each migration would keep one once its server was removed. A sign-in's file says all of
//! how it stands, and its server's record says what the file does. Every worker is sent them all, as
//! [`ToolServers::sent`] says, with the keys and the access tokens, which go nowhere else.

#[cfg(test)]
pub(crate) mod tests;

mod kept;
mod renewals;
mod secret;
mod sending;
mod servers;
mod sign_ins;
mod signed_in_by;
mod wanted;
pub(crate) use renewals::{Checked, Due, Held};
pub(crate) use sending::SentToolServers;
pub(crate) use servers::{ToolServerChange, ToolServerRemoved, ToolServers};
pub(crate) use sign_ins::SigningIn;
pub(crate) use signed_in_by::SignedInBy;
pub(crate) use wanted::Wanted;
