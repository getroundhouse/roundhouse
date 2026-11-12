//! What an approval asks of the person, for a client to draw, the decisions that answer it, in the forms the adapter
//! that asked takes, and what was answered, for a client to say. A request says what it asks, as its adapter read it.
//! One recorded before adapters said is read from its tool and its input by [`older`], which holds the forms an
//! adapter from before takes as well, so nothing else in a client reads an agent's tools.

use std::borrow::Cow;
use std::collections::BTreeMap;

use protocol::event::{ApprovalDecision, Asking, EventBody, Question, QuestionAnswer};

mod older;
#[cfg(test)]
mod tests;

/// What an approval asks, and how to answer it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval<'a> {
    asking: Cow<'a, Asking>,
    /// It's from before adapters said what each asks, so its adapter may take answers only by text, and a refusal only
    /// in a client's words.
    from_before: bool,
}

impl<'a> Approval<'a> {
    /// What the request `body` asks, or `None` for any other event.
    pub fn of(body: &'a EventBody) -> Option<Self> {
        let EventBody::ApprovalRequested { tool_name, input, asking, .. } = body else { return None };
        Some(match asking.as_deref() {
            Some(asking) => Approval { asking: Cow::Borrowed(asking), from_before: false },
            None => Approval { asking: Cow::Owned(older::asking(tool_name, input)), from_before: true },
        })
    }

    pub fn asking(&self) -> &Asking {
        &self.asking
    }

    /// The questions it asks, or `None` where it asks something else.
    pub fn questions(&self) -> Option<&[Question]> {
        match &*self.asking {
            Asking::Questions { questions } => Some(questions),
            Asking::ToolUse | Asking::Plan { .. } | Asking::Unknown => None,
        }
    }

    /// No, with no words of the person's: the tool isn't to be used, the plan wants more work, or the questions are
    /// left unanswered. The adapter tells the agent so, or for a request from before, the words clients used then do.
    pub fn refused(&self) -> ApprovalDecision {
        let message = self.from_before.then(|| older::refusal(&self.asking).to_string());
        ApprovalDecision::Deny { message, interrupt: false }
    }

    /// The person's answers to its questions, one for each, in the order they were asked, with one that has nothing in
    /// it for a question they left. A request from before has them by each question's text.
    pub fn answered(&self, answers: Vec<QuestionAnswer>) -> ApprovalDecision {
        if !self.from_before {
            return ApprovalDecision::AnswerByPosition { answers };
        }
        let questions = self.questions().unwrap_or_default();
        ApprovalDecision::AnswerByText { answers: older::by_text(questions, &answers) }
    }

    /// The same, holding what it asks, for keeping past the event it was read from.
    pub fn into_owned(self) -> Approval<'static> {
        Approval { asking: Cow::Owned(self.asking.into_owned()), from_before: self.from_before }
    }
}

/// What the person answered an approval's questions, as the record that closed it keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answered {
    /// One for each question, in the order they were asked.
    ByPosition(Vec<QuestionAnswer>),
    /// By each question's text, with several choices for one joined with a comma and a space, as a client from before
    /// answered, and the hub recorded what such a client sent.
    ByText(BTreeMap<String, String>),
}

impl Answered {
    /// What the record `body` keeps of the answers, or `None` where it keeps none, as for any decision but an answer,
    /// one recorded before the hub kept answers, and any other event.
    pub fn of(body: &EventBody) -> Option<Self> {
        let EventBody::ApprovalResolved { answers_by_text, answers_by_position, .. } = body else { return None };
        match (answers_by_position, answers_by_text) {
            (Some(answers), _) => Some(Answered::ByPosition(answers.clone())),
            (None, Some(answers)) => Some(Answered::ByText(answers.clone())),
            (None, None) => None,
        }
    }

    /// What was answered to `question`, asked at `at` among the questions, in words, or `None` where nothing was.
    pub fn to(&self, at: usize, question: &Question) -> Option<Cow<'_, str>> {
        match self {
            Answered::ByPosition(answers) => {
                let said: Vec<&str> = question.said(answers.get(at)?).collect();
                (!said.is_empty()).then(|| Cow::Owned(said.join(", ")))
            }
            Answered::ByText(answers) => answers.get(&question.text).map(|answer| Cow::Borrowed(answer.as_str())),
        }
    }
}
