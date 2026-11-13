//! Holding a list's order still while someone is using it.

#[cfg(test)]
mod tests;

mod pointer;
#[cfg(any(test, feature = "testing"))]
pub(crate) use pointer::HOLDS_FOR;
pub(crate) use pointer::{Hold, hears_the_pointer, hold_while_open, keep_held};
