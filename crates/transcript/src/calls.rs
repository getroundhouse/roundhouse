//! Drawing tool calls: the lines of a file a call shows, and the diff of a change. What a call is, and what its result
//! says, is `client_core::calls`, which every client reads. The two calls that ask the person something are drawn from
//! their approvals instead, a question by `rows::asked` and a plan by `rows::approval`, and [`drawn_from_its_approval`]
//! says which they are.

pub(crate) mod diff;
pub(crate) mod lines;

pub(crate) use client_core::calls::{count, first_line};

/// The tool that asks the person questions.
const QUESTIONS: &str = "AskUserQuestion";
/// The tool that asks to leave plan mode, with the plan.
pub(crate) const PLAN: &str = "ExitPlanMode";

/// Whether a call to `name` asks the person something, and so is drawn from its approval, which has everything that
/// was asked, rather than from the call.
pub(crate) fn drawn_from_its_approval(name: &str) -> bool {
    name == QUESTIONS || name == PLAN
}
