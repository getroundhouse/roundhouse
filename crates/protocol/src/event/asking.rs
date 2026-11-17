//! What a request for approval asks of the person: to let the agent use a tool, to answer its questions, or to approve
//! its plan. The agent's adapter reads it from the agent's own request, since only the adapter knows the agent's tools,
//! so a client draws a question or a plan from this rather than from any agent's tool and its input.

use serde::{Deserialize, Serialize};

use crate::capability::asking::QuestionAnswer;
use crate::capability::calls::{Call, CallKind};

/// What an approval asks. One this build doesn't know reads as `Unknown`, which a client asks about as it would a
/// tool's use, by the request's tool and input.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Asking {
    /// Whether the agent may use a tool, as the request's tool and input say.
    ToolUse {
        /// The call that use makes, typed, as the agent's adapter records it, which is also the call's own, as
        /// recorded. `None` in one this build can't read, which a client asks about as a call of a kind it doesn't
        /// know. It's here rather than beside `asking` on the request because only a tool's use needs it, since
        /// questions and a plan say what they ask by what they are, and because every event takes the room of the
        /// largest, and this is boxed already. A request recorded before adapters typed it said only its call's kind,
        /// as `call`, which the hub's migration turned into this.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        typed: Option<Call>,
    },
    /// Questions asked together, in the order they're to be answered. What's asked is an answer rather than a yes.
    Questions {
        #[serde(default)]
        questions: Vec<Question>,
    },
    /// Whether the agent's plan is ready to carry out, which leaves plan mode.
    Plan {
        /// The plan, in the agent's Markdown.
        #[serde(default)]
        plan: String,
    },
    #[serde(other)]
    Unknown,
}

impl Asking {
    /// What kind of call it asks to make: the kind of a tool's use, and for questions and a plan, the kind they are.
    /// One this build doesn't know is a call of a kind it doesn't know, and so is a tool's use whose call it can't
    /// read.
    pub fn kind(&self) -> CallKind {
        match self {
            Asking::ToolUse { typed } => typed.as_ref().map_or(CallKind::Unknown, Call::kind),
            Asking::Questions { .. } => CallKind::Questions,
            Asking::Plan { .. } => CallKind::Plan,
            Asking::Unknown => CallKind::Unknown,
        }
    }
}

/// One question the agent asks the person.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub text: String,
    /// A word or two that names it, such as `Color`, when the agent gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// What the agent offers to choose from, in its order. A question can always be answered by typing, so it can have
    /// none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<QuestionOption>,
    /// Several of the options can be chosen, rather than one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiple: bool,
}

impl Question {
    /// What `answer` says to this question, a part at a time: the label of each option it picked, leaving out any the
    /// question doesn't offer, then what was typed.
    pub fn said<'a>(&'a self, answer: &'a QuestionAnswer) -> impl Iterator<Item = &'a str> {
        let picked = answer.picked.iter().filter_map(|&option| self.options.get(usize::try_from(option).ok()?));
        picked.map(|option| option.label.as_str()).chain(answer.typed.as_deref())
    }
}

/// One of the answers a question offers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    /// What choosing it means, when the agent says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Question, QuestionAnswer, QuestionOption};

    #[test]
    fn an_answer_says_its_options_in_order_then_what_was_typed_and_nothing_the_question_doesnt_offer() {
        let option = |label: &str| QuestionOption { label: label.into(), description: None };
        let colors = Question {
            text: "Colors?".into(),
            header: None,
            options: vec![option("Red"), option("Green"), option("Blue")],
            multiple: true,
        };
        let answer = QuestionAnswer { picked: vec![0, 2, 7], typed: Some("Teal".into()) };

        assert_eq!(colors.said(&answer).collect::<Vec<_>>(), ["Red", "Blue", "Teal"]);
        assert_eq!(colors.said(&QuestionAnswer::default()).count(), 0);
        assert!(QuestionAnswer::default().is_empty() && !answer.is_empty());
    }
}
