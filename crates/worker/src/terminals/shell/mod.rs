//! One terminal's shell: what it's started as, in a PTY, and the threads that serve it.

mod asked;
mod command;
mod front;
mod serving;
#[cfg(test)]
mod tests;
mod threads;
mod told;
mod watching;

pub(super) use asked::Asked;
pub(super) use serving::start;
