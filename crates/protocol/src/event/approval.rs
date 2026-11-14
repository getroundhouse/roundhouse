//! What became of a request for approval, and what the person decided.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::QuestionAnswer;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    Allowed,
    Denied,
    /// Nobody answered: the turn was interrupted, or the agent went away.
    Cancelled,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allow {
        /// Replaces the tool's input. `None` runs it as requested.
        #[serde(default)]
        updated_input: Option<Value>,
        /// Agent-defined rules to remember, taken from `suggestions`.
        #[serde(default)]
        updated_permissions: Option<Value>,
    },
    Deny {
        /// What the agent is told, in the person's own words. `None` leaves the words to the agent's adapter, which
        /// says no as fits what was asked: a tool not to be used, a plan to keep working on, or questions the person
        /// chose not to answer. An adapter from before that takes only words, so a client leaves them out only where
        /// the request said what it asks, which only a newer adapter does.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// Also stops the turn rather than letting the agent try something else.
        #[serde(default)]
        interrupt: bool,
    },
    /// The agent asked the person questions, and this is what they answered, by the question's text, with several
    /// choices for one joined with a comma and a space. It's the form from before
    /// [`ApprovalDecision::AnswerByPosition`]: clients from before send it, so the hub and the adapter still take it,
    /// and a client sends it only where the request didn't say what it asks, since an adapter from before takes no
    /// other.
    #[serde(rename = "answer")]
    AnswerByText { answers: BTreeMap<String, String> },
    /// What the person answered the questions a request asked, by position: one for each, in the order they were
    /// asked. One left out, or with nothing in it, wasn't answered. The agent's adapter tells the agent in the agent's
    /// own form.
    AnswerByPosition { answers: Vec<QuestionAnswer> },
}

/// Questions whose answers must reach only the agent: neither the log nor shared drafts keep them.
pub fn secret_questions(body: &super::EventBody) -> impl Iterator<Item = (usize, &str)> {
    let mut questions = &[][..];
    if let super::EventBody::ApprovalRequested { request: Some(request), .. } = body
        && let crate::capability::approvals::Asks::Questions { questions: asked } = &request.asks
    {
        questions = asked.as_slice();
    }
    questions
        .iter()
        .enumerate()
        .filter(|(_, q)| q.typed == crate::capability::asking::TypedAnswer::Secret)
        .map(|(index, q)| (index, q.text.as_str()))
}
