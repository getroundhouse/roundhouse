//! The whole stack with real processes: a real hub binary, real workers, real git worktrees and a
//! real socket. Only the agent is fake. See `examples/fake_claude/`.

#![cfg(test)]

mod allowing_tools;
mod approvals;
mod attaching;
mod attachments;
mod background;
mod cancelling;
mod carrying_on;
mod chats;
mod claiming;
mod client_restart;
mod compacting;
mod connection;
mod context;
mod core_loop;
mod drafts;
mod effort;
mod failing;
mod feed;
mod finding_a_hub;
mod hub_copies;
mod hub_settings;
mod instructions;
mod launch_flags;
mod logins;
mod misbehaving_worker;
mod model_and_mode;
mod moves_cut_short;
mod new_workspace;
mod other_builds;
mod processes_going_away;
mod prompt_cache;
mod providers;
mod pull_requests;
mod questions;
mod queueing;
mod reading;
mod setup;
#[cfg(target_os = "macos")]
mod staying_awake;
mod steering;
mod stopping_if_idle;
mod stopping_the_hub;
mod subagents;
#[cfg(test)]
mod support;
mod terminals;
mod titles;
mod tools;
mod workspaces;
