//! Conversation mode's voice on this device, the same for every client that has one: what `client-core` decides to
//! read out, read with Phonon through the speakers by `speaking`, and Phonon's files fetched once speech is on,
//! saying how far meanwhile. What to read, and what to tell the hub, is
//! `client-core`'s, so this is only the join, and an app only draws what it says. No UI and no async runtime, as
//! `docs/principles/layers.md` says.

#![warn(clippy::wildcard_enum_match_arm)]

mod conversations;
mod voices;

pub use conversations::Conversations;
pub use voices::{FILES, Spoken, Voices};
