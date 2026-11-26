//! What every app draws of a hub's workspaces and chats, other than a chat's transcript, which is `transcript`'s: a
//! chat's and a workspace's marks, a workspace's menu, the picker of a chat's model with the hub's list of models it
//! follows, and what's said of a hub that's across a protocol break. What they show comes from `client-core` and the records, so each app draws them the same way, and lays
//! them out where its own screens put them.

#![warn(clippy::wildcard_enum_match_arm)]

mod model_lookup;
mod model_picker;
mod needs_updating;
mod signal;
mod workspace_menu;

pub use model_lookup::FollowedModels;
pub use model_picker::{Picked, model_label, model_picker};
pub use needs_updating::{NeedsUpdating, needs_updating};
pub use signal::{chat_signal, mark_row, recorded_work, sign, work, work_not_known, workspace_signal};
pub use workspace_menu::{Choose, WorkspaceMenu, marking_and_renaming};
