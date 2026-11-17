//! What a terminal shows, kept from what the program in it writes, and the bytes that draw it again in another
//! terminal, which a client that attaches to a terminal gets before what the program writes next, as "Terminals" in
//! `docs/architecture.md` says. The worker keeps each of its terminals' with it, and a client will keep its own copy
//! the same way to draw it.
#![warn(clippy::wildcard_enum_match_arm)]

mod mirror;

pub use mirror::{Mirror, MirrorError, Size};
