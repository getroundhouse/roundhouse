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

mod page;
#[cfg(test)]
mod tests;

#[cfg(test)]
use crate::sounds::Sound;
#[cfg(test)]
use crate::workspace_order::Sort;
pub(crate) use page::{Asking, FIND_KEY, KEY_CONTEXT, OfHub, page};

mod preferences;
#[cfg(test)]
use preferences::SettingsFile;
pub(crate) use preferences::{Settings, change, get, load, observe};
