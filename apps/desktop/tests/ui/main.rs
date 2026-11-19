//! The desktop app driven through its real UI, by clicks and typing, against a real hub, real workers
//! and real git worktrees. Only the agent is fake, and no window appears on screen. See
//! `tools/stand-ins/fake-claude/` for what prompts can script. A test of only what the window draws of
//! what a hub holds opens it on a fake hub's fixed data instead, with `Ui::open_on`, and the gallery
//! draws each screen from one, so a screen looks the same every time.

#![cfg(test)]

mod account_moves;
mod agent_selection;
mod all_files;
mod attachments;
mod back_and_forward;
mod cached_transcript;
mod changes;
mod chats;
mod checkout;
mod context;
mod core_loop;
mod dictation;
mod diff_lines;
mod drafts;
mod finding;
#[cfg(target_os = "macos")]
mod gallery;
mod home;
mod home_order;
mod images;
mod kept_while_written;
mod lost_release;
#[cfg(target_os = "macos")]
mod marks;
mod menus;
mod new_workspace;
mod notifications;
mod other_hubs;
mod outline;
mod pairing;
mod pickers;
mod proposals;
mod pull_requests;
mod pushing;
mod questions;
mod queue;
mod quitting;
mod reconnecting;
mod reply_images;
mod restart_now;
mod retrying;
mod scrolling;
mod settings;
mod setup;
mod sidebar;
mod sidebar_drag;
mod sidebar_fold;
mod sidebar_order;
mod sidebar_tabs;
mod sidebars;
mod steering;
mod streaming;
mod subagents;
#[cfg(test)]
mod support;
mod terminals;
mod title_strip;
mod titles;
mod tool_use;
mod tools;
mod turn_card;
mod unread;
mod updating;
mod waits;
mod window_buttons;
mod window_place;
mod workspace_card;
