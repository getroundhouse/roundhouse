//! Every screen of the app, drawn from made-up data, with no real hub and no window on screen, and checked against
//! its approved image in the spec. Each screen gets a fake hub that answers from fixed data, in `gallery/screens.rs`,
//! `gallery/context_screens.rs`, `gallery/diff_screens.rs`, `gallery/tool_screens.rs`, `gallery/subagent_chats.rs` and
//! `gallery/update_screens.rs`, and the real app connects to it, so what's drawn is exactly what the app draws. Each picture shows only the part of
//! the window its screen is about, as `gallery/frame.rs` says. What each screen looks like now goes in
//! `.context/gallery/` at the repo's root, for looking at. The app icon is drawn and checked the same way, in
//! `gallery/icon.rs`, as are every state of a pull request's bar and of the status mark, every colour, every kind
//! of text, every corner and every size of control. Text of each kind is drawn the same way at several scales, in
//! `gallery/baselines.rs`, to check it lines up.

#![cfg(test)]

mod approval_chats;
mod approved;
mod background_chats;
mod baselines;
mod cache_screens;
mod call_screens;
mod capture;
mod chats;
mod codex_screens;
mod colours;
mod compaction_chats;
mod context_screens;
mod diff_screens;
mod events;
mod failure_chats;
mod fake_hub;
mod file_chats;
mod finding;
mod frame;
mod headless;
mod home_screens;
mod icon;
mod image_chats;
mod image_files;
mod kinds_of_text;
mod long_chats;
mod look_screens;
mod mark_states;
mod nested;
mod patch;
mod pictures;
mod pull_requests;
mod question_chats;
mod screens;
mod settings_screens;
mod sidebar_screens;
mod steps;
mod subagent_chats;
mod subagent_records;
mod tool_chats;
mod tool_screens;
mod turn_screens;
mod update_screens;
mod workspace_screens;
mod worktree;
mod world;
mod worlds;

mod screens_approved;
use screens_approved::UPDATE;

mod controls;

mod corners;
