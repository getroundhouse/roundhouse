//! What changed between two images, as the Difference comparison shows it: the image after, faded to grey, with each
//! pixel that differs from the one before tinted a colour of its own, which what's there shows through, and the places
//! that changed, which an outline goes around so they show even when the image is drawn too small for its pixels to. A
//! pixel differs when any of its channels does at all, as the gallery decides a screen no longer matches its image.
//! Both are laid from their top left, as the other comparisons draw them, and only what both cover is compared: what
//! only one covers is set apart, since every pixel of it would count as changed and say nothing about what did.

#[cfg(test)]
mod tests;

mod pixels;
#[cfg(test)]
pub(crate) use pixels::Place;
#[cfg(test)]
use pixels::{CELL, Changes, joined, uncovered};
pub(crate) use pixels::{Difference, colour};
