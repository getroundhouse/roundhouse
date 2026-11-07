//! The desktop app driven through its real UI, by clicks and typing, against a real hub, real workers
//! and real git worktrees. Only the agent is fake, and no window appears on screen. See
//! `apps/daemon/examples/fake_claude/` for what prompts can script.

#![cfg(test)]

mod chats;
mod core_loop;
mod pickers;
mod questions;
mod reconnecting;
mod setup;
mod streaming;
#[cfg(test)]
mod support;
