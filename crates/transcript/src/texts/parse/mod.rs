//! A text parsed by the kit's own Markdown parser, as its text view would, with each leaf keyed by its place in the
//! text: each paragraph, heading, code block and table cell. The kit reports where it drew each keyed leaf, and paints
//! the ranges it's given for one behind it.

#[cfg(test)]
mod tests;

mod leaf;
mod markdown;
mod top_blocks;
pub(crate) use leaf::Leaf;
pub(crate) use markdown::ATOM;
#[cfg(test)]
pub(super) use markdown::parse;
pub(super) use markdown::{Again, Whose, again, gone_on, joined, parse_shown, rest};
pub(crate) use markdown::{Parsed, leaves};
pub(crate) use top_blocks::{TopBlock, top_blocks, top_blocks_and_items};
