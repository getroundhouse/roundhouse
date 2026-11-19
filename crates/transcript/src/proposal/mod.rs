//! A proposal, as its tab shows it: a decision an agent asked the person to make that's too large for a question, as a
//! page of its own. On the left, an outline of its parts, which scrolls the page to the part pressed and marks the one
//! at the top. Beside it, the page: the context, then each question, with its question, what compares its options, each
//! option as a card with its letter, label, description and body, and the option the agent recommends, and why. The app
//! draws the band along its top, with its title.
//!
//! What the agent wrote is drawn and selected as a chat's texts are, through `crate::texts` and `crate::selection`, so
//! a text the Markdown parser panics on doesn't end the app, and what's selected is copied as it was written. The page
//! is drawn whole, since a proposal is at most 128 KiB, so it scrolls by GPUI's own handle, with the kit's scrollbar
//! and its bounce at the ends, and `page` does for it what the chat's list does for its rows' texts.

#[cfg(test)]
mod tests;

mod answering;
mod dictating;
mod document;
mod page;
mod view;

pub use answering::ProposalEvent;
pub use view::{ICON, ProposalView};
