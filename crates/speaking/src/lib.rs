//! Reading text out on this device: cutting each text into phrases as it comes, saying them one after another with
//! Phonon through the speakers, saying how far each text has been heard as each phrase's last sample goes to them, and
//! stopping. It knows nothing of chats or the hub: `client-core` decides what's read and tells the hub how far it got.
//! No UI and no async runtime, as `docs/principles/layers.md` says.

#![warn(clippy::wildcard_enum_match_arm)]

mod phrases;
mod reader;
mod speakable;
mod speaking;
#[cfg(test)]
mod tests;
mod update;

pub use speaking::Speaking;
pub use update::{SpeakingError, TextId, Update};
