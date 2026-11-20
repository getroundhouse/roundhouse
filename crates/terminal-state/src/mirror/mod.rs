//! A copy of one terminal, the worker's or a client's. Everything the program in it writes goes through it, and it
//! gives back what a fresh terminal needs to show the same, what a view draws, and what the program reads for a key.

#[cfg(test)]
use libghostty_vt::fmt::{Format, Formatter};
#[cfg(test)]
use libghostty_vt::screen::Screen;

#[cfg(test)]
mod tests;

mod drawing;
mod place;
mod screen;
mod selecting;
mod typing;
#[cfg(test)]
use screen::{HISTORY_BYTES, HISTORY_LINES, TO_SHELL_SCREEN, screen};
pub use screen::{Mirror, MirrorError, Size};
pub use selecting::{Pointer, SelectBy};
