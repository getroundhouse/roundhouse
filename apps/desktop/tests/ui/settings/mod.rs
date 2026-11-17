//! Settings, which shows in place of the sidebar and the workspace, and what each setting changes.

#![cfg(test)]

mod account_card;
mod account_rows;
mod agents;
mod agents_page;
mod git;
mod git_app;
mod git_app_rows;
mod git_blocked;
mod git_sign_in;
mod made_up;
mod search;

mod changing;
pub(crate) use changing::open_settings;
use changing::{KEEP_AWAKE, settings_showing};
