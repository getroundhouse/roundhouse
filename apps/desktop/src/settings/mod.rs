//! What someone has chosen in Settings, and in a few places that choose how the app looks there and then, such as the
//! order of the sidebar, and Settings itself. A setting of the app's own belongs to this Mac rather than
//! to a hub, since the app can show several hubs, so they're kept in a file in the app's data directory and read once,
//! as the app opens. A setting a hub carries out is one of that hub's records, which Settings shows and changes.

#[cfg(test)]
use protocol::model::RepoId;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::collections::BTreeSet;

mod choice;
mod page;
pub(crate) mod speech_speed;
#[cfg(test)]
mod tests;

#[cfg(test)]
use crate::sounds::Sound;
pub(crate) use choice::Choice;
#[cfg(test)]
use client_core::workspace_order::Sort;
pub(crate) use page::{Asking, FIND_KEY, KEY_CONTEXT, OfHub, PageId, page};

mod app_settings;
#[cfg(test)]
use app_settings::SettingsFile;
pub(crate) use app_settings::{Settings, change, get, load, observe, observe_in_app};
