//! Finding text in a chat. What's looked for is a [`Needle`], which the hub and every client match the same way: the
//! hub to say which of a chat's events hold it, and a client to say where it is in what the chat shows of them.
//!
//! A chat shows Markdown as it renders it, so `**parser** is` shows as `parser is`, and a line break in a paragraph as
//! a space. The hub doesn't render it, so it looks past the characters Markdown marks text with, in [`MARKS`], and
//! past how much whitespace there is, and says an event holds `parser is` whether it's written plain, across bold,
//! italics, code or an escape, or across a line. A phrase across a link, which renders as its words alone, isn't found
//! that way.

#[cfg(test)]
use crate::event::{ContentBlock, EventBody, FileChange};
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::ops::Range;

mod loose;
#[cfg(test)]
mod tests;

mod search;
pub use search::{Found, MARKS, Needle, NeedleError};
