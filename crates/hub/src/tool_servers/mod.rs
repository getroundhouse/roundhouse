//! The tool servers the hub has of its own: adding one and removing it, with what it knows the person by,
//! such as a key, in a file of its own, in a directory apart from every login's, which `crate::login_files` keeps in the
//! order it says. Everything else about a server is on its row in the database. The secret stays out of it since the
//! copies of it from before each migration would keep one once its server was removed. Every worker is sent them all, as
//! [`ToolServers::sent`] says, with the keys, which go nowhere else.

#[cfg(test)]
mod tests;

mod secret;
mod sending;
mod servers;
pub(crate) use sending::SentToolServers;
pub(crate) use servers::{ToolServerChange, ToolServerRemoved, ToolServers};
