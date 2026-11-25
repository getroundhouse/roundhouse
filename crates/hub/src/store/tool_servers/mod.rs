//! The tool servers the hub has of its own, as each one's row has it: what the person calls it, where it is, how it
//! knows the person, and which file the secret it knows them by is in. The secret is never in the database, as
//! `crate::tool_servers` says.

#[cfg(test)]
mod tests;

mod rows;
pub(crate) use rows::AddingToolServer;
pub(super) use rows::tool_servers;
