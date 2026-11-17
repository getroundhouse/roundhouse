//! What the person decides of a request for approval, and what of it a run takes.

use serde::{Deserialize, Serialize};

use super::{Access, Asks, Capability, Request, Rule, RuleEffect};
use crate::capability::asking::QuestionAnswer;
use crate::capability::asking::{self, PlanContext, ServerAnswer};
use crate::capability::{Capabilities, Capability as Any};

/// What a decision can do besides allowing a use once or refusing it, which every request takes. A request takes a
/// decision only when it takes every part the decision has.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DecisionPart {
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
    /// The use goes ahead as it was asked, since a call isn't edited before it's allowed: one the person would have go
    /// otherwise is denied, in words that say what to do instead where the agent takes words.
    Allow { remember: Remember },
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
    /// The plan is approved, and the agent starts on it in the mode its adapter goes on in after a plan, the one it
    /// has besides plan mode, so a decision names none.
    ApprovePlan { context: PlanContext },
    /// The plan isn't approved, and the agent keeps planning, with the person's words when they gave some.
    KeepPlanning {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        words: Option<String>,
    },
    /// What was granted of the access asked for, and for how long.
    Grant { access: Vec<Access>, lasts: Lasts },
    /// The answer to a tool server's request.
    Respond(ServerAnswer),
    /// One this build doesn't know, from the log, which a client shows as answered without saying how, and which is
    /// never sent on.
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "remember", rename_all = "snake_case")]
pub enum Remember {
    Once,
    Conversation,
    Rule(Rule),
    #[serde(other)]
    Unknown,
}

impl Remember {
    /// The rule it keeps, if it keeps one.
    pub fn rule(&self) -> Option<&Rule> {
        if let Remember::Rule(rule) = self { Some(rule) } else { None }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AfterDenial {
    /// The agent carries on, trying something else.
    Continue,
    /// The turn stops.
    Stop,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lasts {
    Turn,
    Conversation,
    #[serde(other)]
    Unknown,
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
            Decision::Unknown => Vec::new(),
        }
    }

    /// The parts it has, each of which the run's requests have to take.
    pub fn parts(&self) -> Vec<DecisionPart> {
        let mut parts = Vec::new();
        if let Decision::Allow { remember } = self {
            parts.extend(match remember {
                Remember::Once => None,
                Remember::Conversation => Some(DecisionPart::ForConversation),
                Remember::Rule(_) => Some(DecisionPart::KeepAllow),
                Remember::Unknown => Some(DecisionPart::Unknown),
            });
        }
        if let Decision::Deny { words, then, keep } = self {
            parts.extend(words.as_ref().map(|_| DecisionPart::Words));
            parts.extend(match then {
                AfterDenial::Continue => None,
                AfterDenial::Stop => Some(DecisionPart::Stop),
                AfterDenial::Unknown => Some(DecisionPart::Unknown),
            });
            parts.extend(keep.as_ref().map(|_| DecisionPart::KeepDeny));
        }
        parts
    }

    /// Whether a run that claims `claims` takes it: every capability it needs, every part it has, a kept rule where
    /// rules can be kept, with the decision's own effect, and nothing this build doesn't know, which the adapter
    /// couldn't tell its agent.
    pub fn allowed(&self, claims: &Capabilities) -> bool {
        let requests = claims.approvals.requests.as_ref();
        let parts = self.parts().iter().all(|part| requests.is_some_and(|r| r.parts.contains(part)));
        let kept = self.rule().is_none_or(|(rule, effect)| {
            rule.is_known() && rule.effect == effect && requests.is_some_and(|r| r.kept.contains(&rule.kept))
        });
        let known = match self {
            Decision::ApprovePlan { context } => *context != PlanContext::Unknown,
            Decision::Grant { access, lasts } => !access.contains(&Access::Unknown) && *lasts != Lasts::Unknown,
            Decision::Respond(answer) => *answer != ServerAnswer::Unknown,
            Decision::Allow { .. }
            | Decision::Deny { .. }
            | Decision::Answer { .. }
            | Decision::KeepPlanning { .. } => true,
            Decision::Unknown => false,
        };
        claims.has_all(self.needs()) && parts && kept && known
    }

    /// Whether it answers `request`, as a decision of a kind the request takes: a call allowed or refused; each
    /// question answered as [`Question::takes`] says, one answer to each, which is left empty for one the person chose
    /// not to answer, so that's said one way, which every adapter takes; a plan approved or not; a host reached or not;
    /// access granted, of what was asked, or refused; or a tool server answered. A call keeps only a rule the request
    /// suggests, as [`Request::suggests`] says, and a request that comes whatever the mode keeps none. One this build
    /// doesn't know can only be refused.
    ///
    /// [`Question::takes`]: crate::capability::asking::Question::takes
    pub fn answers(&self, request: &Request) -> bool {
        let keeps_none = |rule: Option<&Rule>| rule.is_none() || !request.always;
        let suggested = |rule: Option<&Rule>| keeps_none(rule) && rule.is_none_or(|rule| request.suggests(rule));
        match (&request.asks, self) {
            (Asks::Call { .. }, Decision::Allow { remember }) => suggested(remember.rule()),
            (Asks::Call { .. }, Decision::Deny { keep, .. }) => suggested(keep.as_ref()),
            (Asks::Network { .. }, Decision::Allow { remember }) => keeps_none(remember.rule()),
            (Asks::Questions { questions }, Decision::Answer { answers }) => {
                answers.len() == questions.len() && questions.iter().zip(answers).all(|(q, a)| q.takes(a))
            }
            (Asks::Access { access: asked }, Decision::Grant { access, .. }) => {
                access.iter().all(|granted| asked.contains(granted))
            }
            (Asks::Network { .. } | Asks::Access { .. }, Decision::Deny { keep, .. }) => keeps_none(keep.as_ref()),
            (Asks::Plan { .. }, Decision::ApprovePlan { .. } | Decision::KeepPlanning { .. })
            | (Asks::Server(_), Decision::Respond(_))
            | (Asks::Unknown, Decision::Deny { keep: None, .. }) => true,
            _ => false,
        }
    }

    /// It with every secret answer to `request`'s questions taken out, as the hub records it, leaving each such
    /// question unanswered.
    #[must_use]
    pub fn without_secrets(&self, request: &Request) -> Decision {
        let mut decision = self.clone();
        if let Decision::Answer { answers } = &mut decision {
            for at in request.secret_questions() {
                if let Some(answer) = answers.get_mut(at) {
                    *answer = QuestionAnswer::default();
                }
            }
        }
        decision
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
            | Decision::Respond(_)
            | Decision::Unknown => None,
        }
    }
}
