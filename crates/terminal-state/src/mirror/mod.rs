//! The worker's copy of one terminal. Everything the program in it writes goes through it, and it gives back what a
//! fresh terminal needs to show the same.

#[cfg(test)]
use libghostty_vt::fmt::{Format, Formatter};
#[cfg(test)]
use libghostty_vt::screen::Screen;

#[cfg(test)]
mod tests;

mod screen;
pub use screen::{Mirror, MirrorError, Size};
#[cfg(test)]
use screen::{TO_SHELL_SCREEN, screen};
