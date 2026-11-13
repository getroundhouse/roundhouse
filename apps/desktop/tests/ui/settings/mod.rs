//! Settings, which shows in place of the sidebar and the workspace, and what each setting changes.

#![cfg(test)]

mod accounts;
mod search;

mod changing;
pub(crate) use changing::open_settings;
use changing::{KEEP_AWAKE, settings_showing, typing};
