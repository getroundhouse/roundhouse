//! A workspace's terminals as the hub keeps them: each one's record, in memory, since its shell lasts only as long as
//! its worker, and what the worker sends of it, for every client attached. The shells and what they show are the
//! workers', since a worker does a workspace's work, as `docs/principles/parts.md` says.

#[cfg(test)]
mod tests;

mod attaching;
mod open_terminals;
mod opening;
mod terminal_calls;

pub(crate) use attaching::Following;
#[cfg(test)]
pub(crate) use open_terminals::QUEUED_UPDATES;
pub(crate) use open_terminals::{Numbered, Queued, Terminals};
use open_terminals::{Open, Outcome, Stage};
