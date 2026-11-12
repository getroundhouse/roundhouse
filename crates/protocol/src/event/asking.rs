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

/// One of the answers a question offers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    /// What choosing it means, when the agent says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
