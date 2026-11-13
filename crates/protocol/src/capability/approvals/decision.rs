//! What the person decides of a request for approval, and what of it a run takes.

use serde::{Deserialize, Serialize};

use super::{Access, ApprovalMode, Capability, Rule, RuleEffect};
use crate::capability::asking::{self, PlanContext, ServerAnswer};
use crate::capability::calls::Call;
use crate::capability::{Capabilities, Capability as Any};
use crate::event::QuestionAnswer;

/// What a decision can do besides allowing a use once or refusing it, which every request takes. A request takes a
/// decision only when it takes every part the decision has.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DecisionPart {
    /// The use goes ahead with its call changed, as the person edited it.
    Edited,
    /// The same use isn't asked about again in this conversation.
    ForConversation,
    /// A rule is kept, so the same use is allowed without asking from now on.
    KeepAllow,
    /// The refusal carries the person's words, which the agent reads before it carries on.
    Words,
    /// The refusal stops the turn.
    Stop,
    /// A rule is kept, so the same use is refused without asking from now on.
    KeepDeny,
    #[serde(other)]
    Unknown,
}

/// What the person decided, which the adapter tells the agent in the agent's own form.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Allow {
        /// The call as the person edited it, which the adapter gives the agent as its tool's input.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        edited: Option<Call>,
        remember: Remember,
    },
    Deny {
        /// What the agent is told, in the person's words. `None` leaves the words to the adapter.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        words: Option<String>,
        then: AfterDenial,
        /// A rule to keep, so the same use is refused without asking from now on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keep: Option<Rule>,
    },
    /// The answers to the request's questions, one for each, in order.
    Answer { answers: Vec<QuestionAnswer> },
    /// The plan is approved, and the agent starts on it in `then`.
    ApprovePlan { then: ApprovalMode, context: PlanContext },
    /// The plan isn't approved, and the agent keeps planning, with the person's words when they gave some.
    KeepPlanning {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        words: Option<String>,
    },
    /// What was granted of the access asked for, and for how long.
    Grant { access: Vec<Access>, lasts: Lasts },
    /// The answer to a tool server's request.
    Respond(ServerAnswer),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "remember", rename_all = "snake_case")]
pub enum Remember {
    Once,
    Conversation,
    Rule(Rule),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AfterDenial {
    /// The agent carries on, trying something else.
    Continue,
    /// The turn stops.
    Stop,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lasts {
    Turn,
    Conversation,
}

impl Decision {
    /// The capabilities it needs its run to claim.
    pub fn needs(&self) -> Vec<Any> {
        match self {
            Decision::Allow { .. } | Decision::Deny { .. } => vec![Capability::Requests.into()],
            Decision::Answer { .. } => vec![asking::Capability::Questions.into()],
            Decision::ApprovePlan { context: PlanContext::Fresh, .. } => {
                vec![asking::Capability::PlanApproval.into(), asking::Capability::FreshPlan.into()]
            }
            Decision::ApprovePlan { .. } | Decision::KeepPlanning { .. } => {
                vec![asking::Capability::PlanApproval.into()]
            }
            Decision::Grant { .. } => vec![Capability::AccessRequests.into()],
            Decision::Respond(_) => vec![asking::Capability::ServerRequests.into()],
        }
    }

    /// The parts it has, each of which the run's requests have to take.
    pub fn parts(&self) -> Vec<DecisionPart> {
        let mut parts = Vec::new();
        if let Decision::Allow { edited, remember } = self {
            parts.extend(edited.as_ref().map(|_| DecisionPart::Edited));
            parts.extend(match remember {
                Remember::Once => None,
                Remember::Conversation => Some(DecisionPart::ForConversation),
                Remember::Rule(_) => Some(DecisionPart::KeepAllow),
            });
        }
        if let Decision::Deny { words, then, keep } = self {
            parts.extend(words.as_ref().map(|_| DecisionPart::Words));
            parts.extend(matches!(then, AfterDenial::Stop).then_some(DecisionPart::Stop));
            parts.extend(keep.as_ref().map(|_| DecisionPart::KeepDeny));
        }
        parts
    }

    /// Whether a run that claims `claims` takes it: every capability it needs, every part it has, a kept rule where
    /// rules can be kept, with the decision's own effect, for a plan, a mode the run offers to start on it in that
    /// doesn't plan, and nothing this build doesn't know, which the adapter couldn't tell its agent.
    pub fn allowed(&self, claims: &Capabilities) -> bool {
        let requests = claims.approvals.requests.as_ref();
        let parts = self.parts().iter().all(|part| requests.is_some_and(|r| r.parts.contains(part)));
        let kept = self.rule().is_none_or(|(rule, effect)| {
            rule.is_known() && rule.effect == effect && requests.is_some_and(|r| r.kept.contains(&rule.kept))
        });
        let offers = |mode| claims.approvals.modes.as_ref().is_some_and(|modes| modes.offers(mode));
        let known = match self {
            Decision::Allow { edited, .. } => edited.as_ref().is_none_or(Call::is_known),
            Decision::ApprovePlan { then, context } => offers(*then) && !then.plan && *context != PlanContext::Unknown,
            Decision::Grant { access, .. } => !access.contains(&Access::Unknown),
            Decision::Deny { .. } | Decision::Answer { .. } | Decision::KeepPlanning { .. } | Decision::Respond(_) => {
                true
            }
        };
        claims.has_all(self.needs()) && parts && kept && known
    }

    /// The rule it keeps, if it keeps one, and the effect the rule has to have.
    fn rule(&self) -> Option<(&Rule, RuleEffect)> {
        match self {
            Decision::Allow { remember: Remember::Rule(rule), .. } => Some((rule, RuleEffect::Allow)),
            Decision::Deny { keep: Some(rule), .. } => Some((rule, RuleEffect::Deny)),
            Decision::Allow { .. }
            | Decision::Deny { .. }
            | Decision::Answer { .. }
            | Decision::ApprovePlan { .. }
            | Decision::KeepPlanning { .. }
            | Decision::Grant { .. }
            | Decision::Respond(_) => None,
        }
    }
}
