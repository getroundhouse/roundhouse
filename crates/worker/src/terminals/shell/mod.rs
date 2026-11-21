//! One terminal's shell: what it's started as, in a PTY, and the threads that serve it.

mod command;
mod front;
mod serving;
#[cfg(test)]
mod tests;
mod threads;
mod watching;

pub(super) use serving::start;
