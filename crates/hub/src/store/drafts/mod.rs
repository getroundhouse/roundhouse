//! What's typed and attached in each session's composer and not sent, and letting go of a file nothing will send, and
//! what's typed and not sent in answer to each session's question and in a comment on each workspace's diff.

#[cfg(test)]
mod tests;

mod draft_rows;
pub(super) use draft_rows::forget_drafts;
