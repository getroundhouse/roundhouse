//! Images written one after another with nothing else between them, each made a paragraph of its own before the kit
//! parses them, so each is drawn as wide as it is, one under another. The kit draws a paragraph that's only
//! an image at its own width, and any image in a paragraph with text in it at the height of the text, and the line
//! break or space between two images is text to it. So a reply with a screenshot on each line would show them as
//! icons. A reply's own images, which `crate::linked_images` draws rather than the text view, are drawn by the same
//! rule. A prompt's text goes through here too, since images on lines one after another there would be as small.
//! What the hub keeps is the text as it was written, and only what's drawn changes.

#[cfg(test)]
mod tests;

mod boundaries;

pub(crate) use boundaries::images_apart;
#[cfg(test)]
use boundaries::{an_image_may_follow_another, is_image, is_space};
