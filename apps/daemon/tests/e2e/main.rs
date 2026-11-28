//! The whole stack with real processes: a real hub binary, real workers, real git worktrees and a
//! real socket. Only the agent is fake. See `tools/stand-ins/fake-claude/`.
//!
//! Those only on Unix reach the hub or what it runs through a Unix socket, a signal or `ps`, which the tests have no
//! way of on Windows yet, but `attaching` and `hub_copies`, whose worker an earlier build left listening on a Unix
//! socket, as only Unix's did.

#![cfg(test)]

mod accounts;
mod allowing_tools;
mod approvals;
#[cfg(unix)]
mod attaching;
mod attachments;
#[cfg(unix)]
mod background;
mod cancelling;
mod carrying_on;
#[cfg(unix)]
mod changes_watch;
mod chat_accounts;
mod chats;
mod claiming;
#[cfg(unix)]
mod client_restart;
mod compacting;
#[cfg(unix)]
mod connection;
mod context;
mod core_loop;
mod dialing;
mod disconnected;
mod drafts;
mod environment;
mod failing;
mod feed;
#[cfg(unix)]
mod finding_a_hub;
mod git_credential;
mod github_accounts;
#[cfg(unix)]
mod hub_copies;
mod hub_settings;
mod instructions;
mod launch_flags;
mod logins;
mod misbehaving_worker;
mod model_and_mode;
mod model_discovery;
mod moves_cut_short;
mod new_workspace;
#[cfg(unix)]
mod other_builds;
#[cfg(unix)]
mod processes_going_away;
mod prompt_cache;
mod proposals;
mod providers;
mod pull_requests;
mod questions;
mod queueing;
mod reading;
mod records_kept;
mod repos;
#[cfg(unix)]
mod setup;
#[cfg(target_os = "macos")]
mod staying_awake;
#[cfg(unix)]
mod steering;
mod stopping_if_idle;
#[cfg(unix)]
mod stopping_the_hub;
mod subagents;
#[cfg(test)]
mod support;
#[cfg(unix)]
mod terminals;
mod titles;
mod tool_reloads;
mod tool_server_checks;
mod tool_server_edits;
mod tool_server_saves;
mod tool_servers;
#[cfg(unix)]
mod tools;
mod worker_state;
mod worker_versions;
#[cfg(unix)]
mod workspace_lines;
mod workspaces;
