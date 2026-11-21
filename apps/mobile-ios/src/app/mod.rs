//! The app's one window: pairing with a hub, its workspaces, a chat, the hub's own screen and starting a workspace.
//! What's open is kept apart from how it's laid out, in `open.rs`, and the compact layout shows it a screen at a time.

mod hub_info;
mod mobile_app;
mod new_workspace;
mod open;
mod pairing;
mod workspace_menu;
mod workspaces;

#[cfg(feature = "testing")]
pub use mobile_app::open_window_connecting;
use mobile_app::{Creating, Followed, Pairing};
pub use mobile_app::{MobileApp, open_window};
use open::Panel;
