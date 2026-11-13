//! How a text drawn from the kit's blocks draws the images it points to: as the transcript's own elements rather than
//! the kit's, so a click on one opens it over the window. As the kit parses the text, it offers each paragraph, and
//! then each thing in it, to what [`to_read`] gives it. That takes a paragraph that's only an image, which is what an
//! image on a line of its own is once `crate::images_apart` has set several apart, and an image among a paragraph's
//! words. What [`to_draw`] gives the kit draws the first as wide as it is, up to the column's, and the second at the
//! height of the text. Both are drawn as the kit draws its own, and [`Holds`](placement::Holds) says which a paragraph's images are, by
//! the kit's rule. `crate::texts` parses a text this way only when it has images to look up, and keeps whose images
//! each was parsed for, since a reply is parsed for the kit's while it's written.
//!
//! Every other image is left to the kit, which draws it and opens nothing: one in a link, which goes where its link
//! does, one in a heading, a table or a footnote, one that refers to an address given elsewhere, as
//! `![The board][board]` does, which isn't offered with its address, and those of a paragraph that holds images and no
//! words but isn't one image alone, as [`Holds::TheKits`](placement::Holds::TheKits) says. The kit offers an image with nothing to say what
//! it's in, so what reads a paragraph, which is offered whole before anything in it, notes the images among its words,
//! and only those are taken.

#[cfg(test)]
mod tests;

mod placement;
#[cfg(test)]
pub(crate) use placement::taken_at;
pub(super) use placement::to_draw;
#[cfg(test)]
use placement::{ALONE, AMONG_WORDS, Holds, Pointed, in_the_middle};
pub(crate) use placement::{may_take, to_read};
