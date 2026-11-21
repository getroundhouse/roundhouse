//! A terminal someone has open: a window onto it, kept from the pictures and the rows the hub sends of it on the thread
//! it's followed on, which starts again from a picture of what the terminal shows once there's a new connection, or
//! once it's shown again after it was let go for going unshown. A
//! view takes a plain copy of what it shows, and gives the window what it's given, such as keys, which go to the
//! terminal's worker to turn into what the program reads. The window holds only the rows near its view, and asks the
//! hub for the rest, and for a word's ends and text, which only the worker's copy of the terminal has.

mod asking;
mod following;
#[cfg(test)]
mod tests;
mod window;

pub use following::OpenTerminal;
pub use window::{Pointer, SelectBy};
