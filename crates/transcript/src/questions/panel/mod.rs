//! What the agent is waiting on the person for, asked in the composer's place: its questions, one at a time, each
//! option a numbered row, a row to type another answer into, and under them a dot for each question and the arrow that
//! sends. A request to use a tool is asked the same way, as one question that's allowed or denied. [`Asks`] says which
//! it is, and each way they differ.

mod asks;
mod heading;
mod kept;
mod options;
mod pager;
mod selectable;
mod tool_use;
mod typing;

pub(crate) use asks::Asks;

mod question_panel;
pub use question_panel::QuestionPanel;
pub(crate) use question_panel::SETTLE;
