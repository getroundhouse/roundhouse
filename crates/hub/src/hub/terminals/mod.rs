//! A workspace's terminals as the hub keeps them: each one's record, in memory, since its shell lasts only as long as
//! its worker, and what the worker sends of it, for every client watching. The shells and what they show are the
//! workers', since a worker does a workspace's work, as `docs/principles/parts.md` says.

#[cfg(test)]
mod tests;

mod copy;
mod open_terminals;
mod opening;
mod reading;
mod terminal_calls;
mod watching;

use open_terminals::{Open, Outcome, Stage};
pub(crate) use open_terminals::{Queued, Terminals};
pub(crate) use watching::Watcher;
