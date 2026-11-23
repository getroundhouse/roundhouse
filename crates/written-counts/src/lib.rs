//! A count of things as a person reads it, in one of two forms. It's whole, with a comma between each three figures,
//! as `1,212` files, wherever it's written out. It's shortened only where there's room for a few figures, as a
//! workspace's lines in the sidebar and Home are, `+37k`. It has no UI and none of the workspace's other crates, so
//! both `client-core`, which words what a client says, and `look`, which draws a count, write it the same.

#![warn(clippy::wildcard_enum_match_arm)]

#[cfg(test)]
mod tests;

mod shortened;
mod whole;

pub use shortened::shortened;
pub use whole::{Unsigned, whole};
