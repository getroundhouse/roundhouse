//! Drawing tool calls: the lines of a file a call shows, and the diff of a change. What a call is, and what its result
//! says, is `client_core::calls`, which every client reads. The calls that ask the person something are drawn from
//! their approvals instead, a question by `rows::asked` and a plan by `rows::approval`, and
//! `client_core::calls::Recorded::asks_the_person` says which they are.

pub(crate) mod diff;
pub(crate) mod lines;

pub(crate) use client_core::calls::{count, first_line};
