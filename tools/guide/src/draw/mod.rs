//! Draws a page's Markdown as HTML.

mod callout;

mod html;
pub(crate) use html::{draw, escape};
