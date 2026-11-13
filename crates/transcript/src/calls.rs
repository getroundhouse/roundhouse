//! Drawing tool calls: the lines of a file a call shows, and the diff of a change. What a call is, and what its result
//! says, is `client_core::calls`, which every client reads. The calls that ask the person something are drawn from their
//! approvals instead, a question by `rows::asked` and a plan by `rows::approval`, and [`drawn_from_its_approval`]
//! says which they are.

pub(crate) mod diff;
pub(crate) mod lines;

pub(crate) use client_core::calls::{count, first_line};
use protocol::event::CallKind;

/// Whether a call asks the person something, and so is drawn from its approval, which has everything that was asked,
/// rather than from the call. `said` is the kind its adapter said it is, if it said, and `name` its tool.
pub(crate) fn drawn_from_its_approval(said: Option<&CallKind>, name: &str) -> bool {
    client_core::calls::asks_the_person(said, name)
}
