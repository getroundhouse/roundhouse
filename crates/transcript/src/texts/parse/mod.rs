//! A text parsed by the kit's own Markdown parser, as its text view would, with each leaf keyed by its place in the
//! text: each paragraph, heading, code block and table cell. The kit reports where it drew each keyed leaf, and paints
//! the ranges it's given for one behind it.

#[cfg(test)]
mod tests;

mod markdown;
pub(crate) use markdown::{ATOM, Parsed, leaves};
pub(super) use markdown::{Again, ImagesAre, again, gone_on, joined, parse, rest};
