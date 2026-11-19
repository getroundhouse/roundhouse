//! A hub whose every new workspace has a stand-in for its worker, for the hub's own tests of what it does as it makes
//! a workspace, and with one once it's made, such as giving it its first chat and sending that chat a prompt.
//!
//! A hub asks a workspace's provider for its instance before it wants the instance's worker, and takes the worker it
//! has for a workspace, in its map of them, rather than start one. So the provider here puts a stand-in for the worker
//! in that map as it's asked for the instance, as a test does for `w` with [`serving`](super::tests::serving), and the
//! hub never asks it to start a worker, or how to reach one.

mod chats;
mod provider;

pub(super) use chats::{TIMEOUT, a_chat, a_turn, answering_first, logged_in, logged_in_as, the_agent_says, titling};
pub(super) use provider::{Asked, hub_whose_stand_ins_answer_first, hub_with_environment, hub_with_stand_in_workers};
