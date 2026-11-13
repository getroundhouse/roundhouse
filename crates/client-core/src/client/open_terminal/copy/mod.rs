//! The client's copy of a terminal, kept from what the hub sends of it, and what that has the client send the hub:
//! what's typed, by the modes the program set, and the size the view has room for.

mod terminal_copy;
#[cfg(test)]
pub(super) mod tests;

pub(super) use terminal_copy::{TerminalCopy, ToHub};
