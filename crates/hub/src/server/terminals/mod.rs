//! What a connection does itself for a client's terminals: watches one for a client that watches, and sends on what's
//! pressed and new sizes in the order they came.

#[cfg(test)]
mod tests;

mod watching;

pub(super) use watching::{Watching, watch};
