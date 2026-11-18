//! Protocol 3's answer to a request for approval, which a Claude run of a build from before claims takes in place of a
//! decision, since it never says it takes one.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::capability::approvals::{AfterDenial, Asks, Decision, Remember, Request};
use crate::capability::asking::{self, PlanContext};
use crate::model::{ApprovalId, SessionId};

/// The person's answer to the request `approval_id`, as protocol 3 sends it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AgentApproval {
    pub session_id: SessionId,
    pub approval_id: ApprovalId,
    pub decision: ApprovalDecision,
}

/// What the person decided, in protocol 3's form, which `crates/protocol/tests/compatibility/` spells out.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ApprovalDecision {
    /// The use goes ahead as asked, or the plan is carried out.
    Allow,
    /// It doesn't, with the person's words if they gave some, and `interrupt` to stop the turn too.
    Deny {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        interrupt: bool,
    },
    /// The answers to the request's questions, by each question's text, with what was chosen and typed for one joined
    /// with a comma and a space, as every Claude adapter of protocol 3 takes them. A question with no answer is left
    /// out.
    Answer { answers: BTreeMap<String, String> },
}

impl AgentApproval {
    /// `decision`, the answer to `request`, as protocol 3 sends it, if it has a form for it, as
    /// [`ApprovalDecision::of`] says.
    pub fn of(session_id: SessionId, approval_id: ApprovalId, decision: &Decision, request: &Request) -> Option<Self> {
        Some(AgentApproval { session_id, approval_id, decision: ApprovalDecision::of(decision, request)? })
    }
}

impl ApprovalDecision {
    /// `decision`, the answer to `request`, in protocol 3's form, if it has one: a use allowed once, a plan approved in
    /// the conversation it was planned in, a denial that keeps no rule, a plan kept to, and answers, which are a denial
    /// where none was answered, as clients of that protocol said it.
    pub fn of(decision: &Decision, request: &Request) -> Option<Self> {
        Some(match (decision, &request.asks) {
            (
                Decision::Allow { remember: Remember::Once } | Decision::ApprovePlan { context: PlanContext::Kept, .. },
                _,
            ) => ApprovalDecision::Allow,
            (Decision::Deny { words, then: then @ (AfterDenial::Continue | AfterDenial::Stop), keep: None }, _) => {
                ApprovalDecision::Deny { message: words.clone(), interrupt: *then == AfterDenial::Stop }
            }
            (Decision::KeepPlanning { words }, _) => {
                ApprovalDecision::Deny { message: words.clone(), interrupt: false }
            }
            (Decision::Answer { answers }, Asks::Questions { questions }) => {
                let answers = asking::by_text(questions, answers);
                if answers.is_empty() {
                    ApprovalDecision::Deny { message: None, interrupt: false }
                } else {
                    ApprovalDecision::Answer { answers }
                }
            }
            _ => return None,
        })
    }
}
