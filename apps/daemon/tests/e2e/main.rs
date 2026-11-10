//! The whole stack with real processes: a real hub binary, real workers, real git worktrees and a
//! real socket. Only the agent is fake. See `examples/fake_claude/`.

#![cfg(test)]

mod approvals;
mod attachments;
mod background;
mod cancelling;
mod chats;
mod client_restart;
mod connection;
mod core_loop;
mod effort;
mod finding_a_hub;
mod launch_flags;
mod misbehaving_worker;
mod model_and_mode;
mod processes_going_away;
mod pull_requests;
mod questions;
mod queueing;
mod reading;
mod setup;
mod steering;
mod subagents;
#[cfg(test)]
mod support;
mod titles;
mod workspaces;
