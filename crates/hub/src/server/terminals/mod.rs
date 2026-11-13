//! What a connection does itself for a client's terminals: follows one for a client that attaches, and sends on what's
//! typed and new sizes in the order they came.

#[cfg(test)]
mod tests;

mod following;

pub(super) use following::{Answering, attach, in_order};
