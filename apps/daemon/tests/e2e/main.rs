//! The whole stack with real processes: a real hub binary, real workers, real git worktrees, a
//! real socket, and real TLS for paired phones. Only the agent is fake. See `examples/fake_claude/`.

#![cfg(test)]

mod approvals;
mod before_authenticating;
mod cancelling;
mod chats;
mod client_restart;
mod connection;
mod core_loop;
mod finding_a_hub;
mod launch_flags;
mod misbehaving_worker;
mod model_and_mode;
mod pairing;
mod phones;
mod processes_going_away;
mod questions;
mod setup;
#[cfg(test)]
mod support;
mod workspaces;
