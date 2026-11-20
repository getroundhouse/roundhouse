//! Every screen of the app, drawn from made-up data, with no real hub and no window on screen, and checked against its
//! approved image in the spec. Each screen gets a fake hub that answers from fixed data, in `gallery/screens.rs`,
//! `gallery/context_screens.rs`, `gallery/diff_screens.rs`, `gallery/tool_screens.rs`, `gallery/subagent_chats.rs` and
//! `gallery/update_screens.rs`, and the real app connects to it, so what's drawn is exactly what the app draws. Each
//! picture shows only the part of the window its screen is about, as `gallery/frame.rs` says. What each screen looks
//! like now goes in `.context/gallery/` at the repo's root, for looking at. The app icon is drawn and checked the same
//! way, in `gallery/icon.rs`, as are every state of a pull request's bar and of the status mark, and the sheets of the
//! foundations: every colour, every kind of text, every step of space, every corner, every size of a control, of a bar,
//! of a row and of an icon. Each sheet also keeps where it draws each name beside its image, for the design system's
//! page, as `gallery/places.rs` says. Text of each kind is drawn the same way at several scales, in
//! `gallery/baselines.rs`, to check it lines up, and each weight the app asks for is found a face of its own, in
//! `gallery/weights.rs`.

#![cfg(test)]

mod approval_chats;
mod approved;
mod background_chats;
mod banner_kinds;
mod bar_kinds;
mod baselines;
mod cache_screens;
mod call_screens;
mod capture;
mod cards;
mod chats;
mod code_lines;
mod codex_screens;
mod colours;
mod compaction_chats;
mod component_screens;
mod components_approved;
mod context_screens;
mod dialog_widths;
mod dictation_screens;
mod diff_screens;
mod empty_kinds;
mod failure_chats;
mod failures_in_a_row;
mod file_chats;
mod finding;
mod foundation_sheet;
mod frame;
mod git_app_screens;
mod git_screens;
mod headless;
mod home_screens;
mod icon;
mod image_chats;
mod image_files;
mod kinds_of_text;
mod lines;
mod long_chats;
mod mark_states;
mod nested;
mod own_turn_chats;
mod patch;
mod pictures;
mod places;
mod prompt_screens;
mod proposal_chats;
mod pull_requests;
mod question_chats;
mod reference_screens;
mod screens;
mod settings_screens;
mod sidebar_screens;
mod small_things;
mod steps;
mod subagent_chats;
mod terminal_screens;
mod tool_chats;
mod tool_screens;
mod turn_screens;
mod typed_chats;
mod update_screens;
mod weights;
mod workspace_screens;
mod worktree;
mod worlds;

mod screens_approved;
use screens_approved::UPDATE;

mod button_states;

mod controls;

mod key_kinds;
mod list_rows;

mod toggle_states;
mod tooltips;

mod switch_kinds;
mod tab_widths;

mod menu_rows;

mod field_states;

mod heights;

mod icon_sizes;

mod spaces;

mod corners;
