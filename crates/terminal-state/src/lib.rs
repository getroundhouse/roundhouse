//! What a terminal shows, kept from what the program in it writes, and the bytes that draw it again in another
//! terminal, which a client that attaches to a terminal gets before what the program writes next, as "Terminals" in
//! `docs/architecture.md` says. The worker keeps each of its terminals' with it, and a client keeps its own copy the
//! same way, which it draws from a [`drawn::Frame`], turns keys into what the program reads with, and selects in with
//! the pointer.
#![warn(clippy::wildcard_enum_match_arm)]

pub mod drawn;
pub mod keys;
mod mirror;

pub use mirror::{Mirror, MirrorError, Pointer, SelectBy, Size};
