//! `propose` and `get_proposal`: a decision too large for a question, which the agent writes as a structure and the
//! person reads and answers in a tab of its own, and reading one back. The hub keeps each, as
//! `protocol::model::Proposal` says.

#[cfg(test)]
mod tests;

mod calls;
mod described;
mod input;
pub(super) use calls::{GET, PROPOSE, REVISE, definitions, get, propose, revise};
#[cfg(test)]
use described::describe;
#[cfg(test)]
use input::proposed;
