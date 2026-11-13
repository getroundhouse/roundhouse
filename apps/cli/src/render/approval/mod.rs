//! How a request for approval prints: a tool's use by what it's for and what the tool was given, with why the agent
//! asks and which subagent does, questions with their options numbered, and a plan whole, each with the command that
//! answers it, and once it's closed, what became of it and what was answered.

#[cfg(test)]
mod tests;

mod requested;
#[cfg(test)]
use requested::{requested, resolved};
