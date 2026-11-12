//! What a request for approval asks of the person: to let the agent use a tool, to answer its questions, or to approve
//! its plan. The agent's adapter reads it from the agent's own request, since only the adapter knows the agent's tools,
//! so a client draws a question or a plan from this rather than from any agent's tool and its input.

use serde::{Deserialize, Serialize};

/// What an approval asks. One this build doesn't know reads as `Unknown`, which a client asks about as it would a
/// tool's use, by the request's tool and input.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Asking {
    /// Whether the agent may use a tool, as the request's tool and input say.
    ToolUse,
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

/// What the person answered one question, by position: the options they chose, by their place among its options, in
/// the order it offers them, and what they typed as an answer of their own. One with neither wasn't answered.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestionAnswer {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub picked: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed: Option<String>,
}

impl QuestionAnswer {
    /// Nothing chosen and nothing typed, which is no answer.
    pub fn is_empty(&self) -> bool {
        self.picked.is_empty() && self.typed.is_none()
    }
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
