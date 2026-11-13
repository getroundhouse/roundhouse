//! How each row of a file's diff is drawn: a line with its numbers, how it changed, and its code, a fold with the
//! arrows that open it, and a comment on the band it sits on. Lines added and removed are tinted green and red, as
//! review tools tint them, and the words that changed within them more strongly.

use code::code;

mod code;

mod painting;
pub(super) use painting::{Drawing, FOLD_HEIGHT, SIGN, row};

#[cfg(test)]
mod tests;

#[cfg(test)]
use painting::BETWEEN_ARROWS;
pub(super) use painting::gutter;
