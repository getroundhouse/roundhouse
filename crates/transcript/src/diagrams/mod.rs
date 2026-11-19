//! Mermaid diagrams the agent writes, as a fenced code block whose language is `mermaid`, which the transcript draws as
//! pictures, as GitHub does. Each client draws one from its text, so nothing is stored, and the hub and the agent
//! never see the picture. It's drawn as SVG by `merman`, in Rust, with no browser or JavaScript, on another thread,
//! within [`DEADLINE`], and then decoded as any SVG image is, so it's drawn and opened as an image the agent points to
//! is. One that can't be drawn, such as one with a mistake in it, or a kind of diagram this doesn't draw, is drawn as
//! the code block it was written as.
//!
//! The diagram is drawn in the app's colours, as [`Palette`] says, so it's drawn again when the appearance changes.
//! Its words are measured as they'll be drawn, by the font `measuring` finds, which the SVG names, since `merman`
//! sizes each box to the words it measures, and words drawn wider than they were measured would be cut off.

mod drawing;
mod measuring;
mod palette;

#[cfg(test)]
mod tests;

pub(crate) use drawing::{LANGUAGE, is_diagram, svg};
pub(crate) use palette::Palette;
