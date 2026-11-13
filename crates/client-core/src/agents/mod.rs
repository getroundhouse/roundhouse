//! What a hub says of each kind of agent it runs, as a client holds it.

#[cfg(test)]
use crate::Result;
#[cfg(test)]
use protocol::model::{AgentDescription, AgentKind};

#[cfg(test)]
mod tests;

mod catalog;
pub use catalog::Agents;
