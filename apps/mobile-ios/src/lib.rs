//! The iOS app: one hub's workspaces and chats, on a phone. Everything here but the start-up in `main.rs` also
//! builds on the Mac, where the tests draw it.

#![warn(clippy::wildcard_enum_match_arm)]

mod app;
mod chat;
mod chrome;
mod hub_link;
mod vault;

pub use app::{MobileApp, open_window};
pub use vault::{Vault, VaultError};

#[cfg(target_os = "ios")]
pub use vault::DeviceVault;

#[cfg(feature = "testing")]
pub use vault::MemoryVault;
