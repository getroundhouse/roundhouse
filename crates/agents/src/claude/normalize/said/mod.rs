//! What the CLI says itself, rather than its model: a message in the model's place, which says why one of its requests
//! to the model failed, or is a note of the CLI's own, and what a command it ran for the person printed, on a replayed
//! `user` line. Each is the CLI's, so none is taken for the agent's words.

#[cfg(test)]
mod tests;

mod stream;

pub(super) use stream::{FailedRequests, Stream, message, printed, printed_notice, text_of};
