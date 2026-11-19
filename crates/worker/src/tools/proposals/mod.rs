//! `propose`, `revise_proposal`, `withdraw_proposal` and `get_proposal`: a decision too large for a question, which the
//! agent writes as a structure and the person reads and answers in a tab of its own, revising and withdrawing one, and
//! reading one back. The hub keeps each, as
//! `protocol::model::Proposal` says.

#[cfg(test)]
mod tests;

mod calls;
mod described;
mod input;
pub(super) use calls::{GET, PROPOSE, REVISE, WITHDRAW, definitions, get, propose, revise, withdraw};
#[cfg(test)]
use described::describe;
#[cfg(test)]
use input::proposed;
