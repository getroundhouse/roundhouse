//! What every app draws of a hub's workspaces and chats, other than a chat's transcript, which is `transcript`'s: a
//! chat's and a workspace's marks, a workspace's menu, and the picker of a chat's model with the hub's list of models
//! it follows. What they show comes from `client-core` and the records, so each app draws them the same way, and lays
//! them out where its own screens put them.

#![warn(clippy::wildcard_enum_match_arm)]

mod model_lookup;
mod model_picker;
mod signal;
mod workspace_menu;

pub use model_lookup::FollowedModels;
pub use model_picker::{Picked, model_label, model_picker};
pub use signal::{chat_signal, mark_row, recorded_work, work, workspace_signal};
pub use workspace_menu::{Choose, WorkspaceMenu, marking_and_renaming};
