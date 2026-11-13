//! How tall each row of a session is, as far as that's known: the height a row was last measured at, and for a row
//! never measured, what the measured ones come to on average.
//!
//! A row's height is kept, not worked out again, so nothing here costs more for a longer session than finding a
//! place in it does. A row is only measured when it's drawn. One measured at another width keeps the height it had,
//! which is a better guess than the average, until it's drawn again.

#[cfg(test)]
use gpui_kit::{Pixels, px};

#[cfg(test)]
mod tests;

mod measurements;
pub(super) use measurements::Heights;
#[cfg(test)]
use measurements::{FIRST_GUESS, LEAST_GUESS, count, narrow};
