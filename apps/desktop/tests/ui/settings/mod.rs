//! Settings, which shows in place of the sidebar and the workspace, and what each setting changes.

#![cfg(test)]

mod agents;
mod git;
mod git_sign_in;
mod search;

mod changing;
pub(crate) use changing::open_settings;
use changing::{KEEP_AWAKE, settings_showing, typing};
