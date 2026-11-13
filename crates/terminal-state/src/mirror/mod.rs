//! A copy of one terminal, the worker's or a client's. Everything the program in it writes goes through it, and it
//! gives back what a fresh terminal needs to show the same, what a view draws, and what the program reads for a key.

#[cfg(test)]
use libghostty_vt::fmt::{Format, Formatter};
#[cfg(test)]
use libghostty_vt::screen::Screen;

#[cfg(test)]
mod tests;

mod drawing;
mod screen;
mod typing;
pub use screen::{Mirror, MirrorError, Size};
#[cfg(test)]
use screen::{TO_SHELL_SCREEN, screen};
