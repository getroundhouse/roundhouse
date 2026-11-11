//! The desktop app driven through its real UI, by clicks and typing, against a real hub, real workers
//! and real git worktrees. Only the agent is fake, and no window appears on screen. See
//! `apps/daemon/examples/fake_claude/` for what prompts can script. The gallery is the exception: it draws
//! each screen from a fake hub's fixed data, so a screen looks the same every time.

#![cfg(test)]

mod all_files;
mod attachments;
mod changes;
mod chats;
mod checkout;
mod context;
mod core_loop;
#[cfg(target_os = "macos")]
mod gallery;
mod home;
#[cfg(target_os = "macos")]
mod marks;
mod menus;
mod new_workspace;
mod other_hubs;
mod outline;
mod pickers;
mod pull_requests;
mod questions;
mod queue;
mod quitting;
mod reconnecting;
mod reply_images;
mod settings;
mod setup;
mod sidebar;
mod sidebar_tabs;
mod steering;
mod streaming;
#[cfg(test)]
mod support;
mod title_strip;
mod titles;
mod tools;
mod unread;
mod updating;
mod window_place;
