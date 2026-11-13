//! Feeding one subscription to a session's log: every event by `seq`, or the agent's own or one subagent's by place.

#[cfg(test)]
use protocol::client::HubFrame;
#[cfg(test)]
use protocol::model::Seq;
#[cfg(test)]
use tokio::sync::mpsc;

#[cfg(test)]
mod tests;

mod following;
pub(super) use following::{Following, pump};
