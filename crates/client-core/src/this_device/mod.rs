//! What an app keeps on the device it runs on, and not on a hub, as `docs/principles/state.md` says: its preferences, and
//! what saves them and its settings. The app says where, and what to say of what couldn't be read or saved.

mod preferences;
mod saving;

pub use preferences::Preferences;
pub use saving::{NotKept, Saver, read_json, write_json};
