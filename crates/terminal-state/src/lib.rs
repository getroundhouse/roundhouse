//! What a terminal shows, kept from what the program in it writes, and the bytes that draw it again in another
//! terminal, which a client that attaches to a terminal gets before what the program writes next, as "Live access" in
//! `docs/architecture.md` plans. Nothing calls it yet.
#![warn(clippy::wildcard_enum_match_arm)]

mod mirror;

pub use mirror::{Mirror, MirrorError, Size};
