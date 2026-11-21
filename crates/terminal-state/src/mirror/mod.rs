//! The worker's copy of one terminal. Everything the program in it writes goes through it, and it gives back what a
//! fresh terminal needs to show the same, the rows a client shows, and what the program reads for what's pressed.

#[cfg(test)]
use libghostty_vt::fmt::{Format, Formatter};
#[cfg(test)]
use libghostty_vt::screen::Screen;

#[cfg(test)]
mod clicking;
#[cfg(test)]
mod tests;

mod compressing;
mod rows;
mod screen;
mod typing;
pub use compressing::QUIET_BEFORE_COMPRESSING;
pub use rows::Tracked;
#[cfg(test)]
use screen::{HISTORY_BYTES, HISTORY_LINES, TO_SHELL_SCREEN, screen};
pub use screen::{Mirror, MirrorError};
