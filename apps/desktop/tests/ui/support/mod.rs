//! What the tests share: a running stack, and a window to drive.

#![cfg(test)]

mod asked;
mod attachments;
mod chats;
mod click;
mod daemons;
mod diff;
mod drawn;
mod files;
mod frozen;
mod gate;
mod held;
mod hover;
mod hub;
mod hub_ahead;
mod labels;
/// For dictation, which only a Mac hears.
#[cfg(target_os = "macos")]
pub(crate) mod made_up_model;
mod motion;
mod new_workspace;
mod order;
mod projects;
mod shown;
mod stand_in;
mod stand_in_gh;
mod stand_in_github;
mod workspaces;

pub(crate) use asked::id;
pub(crate) use click::{click, under_the_middle_of};
pub(crate) use daemons::{binaries, hub_environment};
pub(crate) use diff::Diff;
pub(crate) use files::files_holding;
pub(crate) use frozen::Frozen;
pub(crate) use gate::Gate;
pub(crate) use held::Held;
pub(crate) use labels::{MENU, click_menu_item, labelled};

/// For the gallery, which only a Mac draws.
#[cfg(target_os = "macos")]
pub(crate) use labels::first_labelled_in;
#[cfg(target_os = "macos")]
pub(crate) use labels::menu_item;
pub(crate) use order::{
    MORE_THAN_FIT, after_every_workspace, made_in_order, more_than_fit, order, three_workspaces, top, wait_out_setup,
    worked_in,
};
pub(crate) use projects::{project_row, two_projects};
pub(crate) use stand_in::StandInHub;
pub(crate) use stand_in_gh::pull_request;
pub(crate) use workspaces::{home_row, pencil, sidebar_row};

mod stack;
pub(crate) use stack::{CLAUDE_STARTS_WITH, Stack, TIMEOUT, Ui, commit_settings, git, wait_until};
