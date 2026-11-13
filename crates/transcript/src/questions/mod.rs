//! Questions the agent asks the user. They arrive as a request for approval, which its adapter says asks questions, but
//! what they need is an answer: allowing one without an answer tells the agent the user didn't reply. While one waits,
//! it's asked in the composer's place, by [`QuestionPanel`], and once it's no longer waiting the transcript draws it as
//! a call, as `rows::asked` says. What each question is, and how an answer is sent, is `client_core::Approval`'s.
//!
//! A request to use a tool is asked there too, as a question of the panel's own making, whose answer allows or denies
//! it, and [`Asks`] says which of the two a panel asks. Once it's no longer waiting, its call's own line is what the
//! transcript keeps of it.

mod choice;
mod panel;

pub(crate) use panel::Asks;
pub use panel::QuestionPanel;
#[cfg(feature = "testing")]
pub(crate) use panel::SETTLE;
