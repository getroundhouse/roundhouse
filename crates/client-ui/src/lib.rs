//! What every app draws of a hub's workspaces and chats, other than a chat's transcript, which is `transcript`'s: a
//! chat's and a workspace's marks, and the picker of a chat's model with the hub's list of models it follows. What
//! they show comes from `client-core` and the records, so each app draws them the same way, and lays them out where
//! its own screens put them.

#![warn(clippy::wildcard_enum_match_arm)]

mod model_lookup;
mod model_picker;
mod signal;

pub use model_lookup::FollowedModels;
pub use model_picker::{Picked, model_label, model_picker};
pub use signal::{chat_signal, recorded_work, waiting_on_you, work, workspace_signal};
