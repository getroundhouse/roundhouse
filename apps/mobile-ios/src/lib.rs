//! The iOS app: one hub's workspaces and chats, designed for a handheld pressed by touch, as "Each client" in
//! `docs/spec/look.md` says. Everything here but the start-up in `main.rs` also builds on the Mac, where its tests draw
//! it.

#![warn(clippy::wildcard_enum_match_arm)]

mod app;
mod assets;
mod chat;
mod chrome;
mod hub_link;
mod safe_area;
mod vault;

#[cfg(feature = "testing")]
pub use app::open_window_connecting;
pub use app::{MobileApp, open_window};
pub use assets::Assets;
#[cfg(any(test, feature = "testing"))]
pub use hub_link::the_retry_waits_are;
#[cfg(feature = "testing")]
pub use safe_area::the_safe_area_is;
#[cfg(target_os = "ios")]
pub use vault::DeviceVault;
#[cfg(any(test, feature = "testing"))]
pub use vault::MemoryVault;
pub use vault::{Vault, VaultError};
