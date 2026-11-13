//! What the agent asks the person, apart from approving a tool's use: questions, a plan, and what a tool server needs.
//!
//! Each comes as a request for approval, whose [`Asks`](super::approvals::Asks) says which, and is answered with a
//! [`Decision`](super::approvals::Decision), so a chat waits on the person the same way whatever is asked.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Claimed;
use super::area::area;
use super::setup::ServerName;

area! { "asking the person";
    /// The agent asks the person questions and waits for every answer before it carries on. Each question has its
    /// words, and can have a header, options to choose one or several of, something to show beside an option, and an
    /// answer typed in, as the question says, which a client draws. Answers come back by position, one for each
    /// question, and a question left without one wasn't answered, which the agent is told. A secret answer, one typed
    /// out of sight, such as a password, reaches the agent and nothing else: the hub records that the question was
    /// answered, not what with, and keeps no draft of it. The agent asks in every approval mode.
    Questions => questions: Claimed,
    /// In plan mode, the agent asks to have its plan approved before it changes anything, with the plan's words.
    /// Approving it takes the agent out of plan mode into the mode chosen with it, and it starts on the plan in the
    /// same turn. Not approving it keeps it planning, with the person's words when they gave some. An agent that only
    /// writes its plan and ends its turn, as Codex does, gets this from its adapter, which holds that turn's end back
    /// and asks, and on a decision starts the agent's next turn, out of plan mode or in it, as the same turn, so a turn
    /// ends only once its plan is decided, as with Claude.
    PlanApproval => plan_approval: Claimed,
    /// An approved plan can start in a fresh conversation, which holds only the plan, so the context has room for the
    /// work, as Claude's and Codex's "clear context" choices do. The fresh conversation's ID comes in an `AgentInit`,
    /// as at a launch.
    FreshPlan => fresh_plan: Claimed,
    /// A tool server asks the person, through the agent, for what it needs: a form of typed fields, or a page to open,
    /// such as one to sign in. The answer goes back to the server, or the person declines, or cancels what the server
    /// was doing.
    ServerRequests => server_requests: Claimed,
}

/// One question, as the agent asked it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub choose: Choose,
    #[serde(default)]
    pub typed: TypedAnswer,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Choose {
    #[default]
    One,
    Several,
    #[serde(other)]
    Unknown,
}

/// Whether an answer can be typed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TypedAnswer {
    /// Only an option can be chosen.
    No,
    #[default]
    Yes,
    /// Yes, out of sight as it's typed, and never recorded, as [`Capability::Questions`] says.
    Secret,
    #[serde(other)]
    Unknown,
}

/// What the conversation holds as the agent starts on an approved plan.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PlanContext {
    /// Everything it held while planning.
    Kept,
    /// The plan alone, in a fresh conversation, as [`Capability::FreshPlan`] says.
    Fresh,
    #[serde(other)]
    Unknown,
}

/// What a tool server asks the person.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ServerRequest {
    pub server: ServerName,
    pub message: String,
    pub wants: ServerWants,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "wants", rename_all = "snake_case")]
pub enum ServerWants {
    /// Fields to fill in, in order.
    Form { fields: Vec<Field> },
    /// A page for the person to open, and come back from once they've done what it asks.
    Page { url: String },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// What the answer names it, which the server reads.
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub holds: Holds,
    pub required: Required,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "holds", rename_all = "snake_case")]
pub enum Holds {
    Text,
    Number,
    YesOrNo,
    Choice {
        options: Vec<String>,
        choose: Choose,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Required {
    Yes,
    No,
    #[serde(other)]
    Unknown,
}

/// The answer to a tool server's request.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum ServerAnswer {
    /// What was filled in, by each field's key, or nothing for a page the person has finished with.
    Accept { values: BTreeMap<String, Value> },
    /// The person won't give it, and the server carries on without.
    Decline,
    /// The person stops what the server was doing.
    Cancel,
}
