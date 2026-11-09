//! The whole stack with real processes: a real hub binary, real workers, real git worktrees and a
//! real socket. Only the agent is fake. See `examples/fake_claude/`.

#![cfg(test)]

mod approvals;
mod background;
mod cancelling;
mod chats;
mod client_restart;
mod connection;
mod core_loop;
mod finding_a_hub;
mod launch_flags;
mod misbehaving_worker;
mod model_and_mode;
mod processes_going_away;
mod questions;
mod reading;
mod setup;
#[cfg(test)]
mod support;
mod workspaces;
