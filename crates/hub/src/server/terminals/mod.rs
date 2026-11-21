//! What a connection does itself for a client's terminals: follows one for a client that attaches, watches one for a
//! client that watches, and sends on what's typed and new sizes in the order they came.

#[cfg(test)]
mod tests;

mod backlog;
mod following;
mod watching;

pub(super) use following::{Answering, attach, in_order};
pub(super) use watching::watch;
