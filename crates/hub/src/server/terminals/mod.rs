//! What a connection does itself for a client's terminals: watches one for a client that watches, and sends on what's
//! pressed and new sizes in the order they came.

#[cfg(test)]
mod tests;

mod in_order;
mod watching;

pub(super) use in_order::{Answering, in_order};
pub(super) use watching::watch;
