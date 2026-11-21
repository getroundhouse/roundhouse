//! The worker's copy of one terminal. Everything the program in it writes goes through it, and it gives back the rows
//! a client shows, and what the program reads for what's pressed.

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
use screen::{HISTORY_BYTES, HISTORY_LINES};
pub use screen::{Mirror, MirrorError};
